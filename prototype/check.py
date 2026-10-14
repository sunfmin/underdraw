# /// script
# requires-python = ">=3.12"
# ///
# Prototype checker for docs/format/expressions.md. Throwaway: the real one is the Rust CLI.
# Usage: uv run prototype/check.py drafts/airbnb-listing
import json, re, sys, pathlib, copy
d = pathlib.Path(sys.argv[1]); screen = json.loads((d/"screen.json").read_text())
errs = []
def merge(a, b):
    for k, v in b.items():
        a[k] = merge(a.get(k, {}), v) if isinstance(v, dict) and isinstance(a.get(k), dict) else v
    return a
fx = {}
for name, f in screen["fixtures"].items():
    f = copy.deepcopy(f); base = f.pop("$patch", None)
    fx[name] = merge(copy.deepcopy(fx[base]), f) if base else f
enums = {"status": [x.strip() for x in re.match(r"enum\((.*)\)", screen["data"]["status"]).group(1).split(",")]}
conds = screen["conditions"]

def get(data, path):
    v = data
    for seg in path.split("."):
        if not isinstance(v, dict) or seg not in v: return None, False
        v = v[seg]
    return v, True

# ---- 条件表达式：词法 + 递归下降，直接对一份数据求值，顺便做类型检查 ----
TOK = re.compile(r'\s*(\|\||&&|==|!=|<=|>=|<|>|!|\(|\)|"[^"]*"|-?\d+(?:\.\d+)?|[A-Za-z_][\w.]*)')
def lex(s):
    out, i = [], 0
    while i < len(s):
        m = TOK.match(s, i)
        if not m or m.end() == i:
            if s[i:].strip(): raise SyntaxError(f"无法识别: {s[i:]!r}")
            break
        out.append(m.group(1)); i = m.end()
    return out
def evaluate(name, data, stack=()):
    if name in stack: raise SyntaxError(f"循环引用: {' → '.join(stack + (name,))}")
    toks = lex(conds[name]["when"]); pos = [0]
    peek = lambda: toks[pos[0]] if pos[0] < len(toks) else None
    def take(): pos[0] += 1; return toks[pos[0] - 1]
    def atom():
        t = take()
        if t == "(":
            v = expr_()
            if take() != ")": raise SyntaxError("缺少 )")
            return ("bool", v)
        if t.startswith('"'): return ("str", t[1:-1])
        if re.fullmatch(r"-?\d+(\.\d+)?", t): return ("num", float(t))
        if t in ("true", "false"): return ("bool", t == "true")
        if t == "null": return ("null", None)
        if t in conds: return ("bool", evaluate(t, data, stack + (name,)))
        if "." in t or t in data or t in screen["data"]:
            v, _ = get(data, t)
            return ("path:" + t, v)
        raise SyntaxError(f"未知名字: {t}")
    def compare():
        a = atom()
        if peek() in ("==", "!=", "<", "<=", ">", ">="):
            op = take(); b = atom()
            for x, y in ((a, b), (b, a)):   # 枚举字面量必须是成员
                if x[0].startswith("path:") and x[0][5:] in enums and y[0] == "str" and y[1] not in enums[x[0][5:]]:
                    raise SyntaxError(f"{y[1]!r} 不是 {x[0][5:]} 的成员 {enums[x[0][5:]]}")
            av, bv = a[1], b[1]
            if op in ("<", "<=", ">", ">="):
                if av is None or bv is None: return False
                return {"<": av < bv, "<=": av <= bv, ">": av > bv, ">=": av >= bv}[op]
            return (av == bv) if op == "==" else (av != bv)
        if a[0].startswith("path:") and a[1] is not None and not isinstance(a[1], bool):
            raise SyntaxError(f"{a[0][5:]} 不是 Bool，不能直接当条件（没有真假值转换）")
        return bool(a[1])
    def not_():
        if peek() == "!": take(); return not not_()
        return compare()
    def and_():
        v = not_()
        while peek() == "&&": take(); v = not_() and v
        return v
    def expr_():
        v = and_()
        while peek() == "||": take(); v = and_() or v
        return v
    v = expr_()
    if pos[0] != len(toks): raise SyntaxError(f"多余内容: {toks[pos[0]:]}")
    return v

table = {}
for c in conds:
    try: table[c] = {f: evaluate(c, data) for f, data in fx.items()}
    except SyntaxError as e: errs.append(f"条件 {c}: {e}")

# ---- 遍历节点：使用处、模板、cases ----
FORMATS = {"money", "date", "date:short", "count"} | {f"number:{n}" for n in range(4)}
USE = re.compile(r"!?([A-Za-z_][\w.]*)")
DATE = re.compile(r"\d{4}-\d{2}-\d{2}$")
default = fx["default"]
def check_use(u, scope, where):
    m = USE.fullmatch(u)
    if not m: errs.append(f"{where}: 使用处只能写 !?条件名 或 Bool 路径，得到 {u!r}"); return
    n = m.group(1)
    if n in conds: return
    v, ok = get(scope, n)
    if not ok: errs.append(f"{where}: 未知条件或路径 {n}")
    elif not isinstance(v, bool): errs.append(f"{where}: {n} 不是 Bool，请改成具名条件")
TEXT_PROPS = {"text", "title", "subtitle", "label", "detail"}
def check_tpl(s, scope, where, key=""):
    if re.search(r"\{[^}]*[?:&|=<>!][^}]*\}", re.sub(r"\|[\w:]+\}", "}", s)):
        errs.append(f"{where}: 模板里不能有运算 {s!r}")
    for path, fmt in re.findall(r"\{([\w.]+)(?:\|([\w:]+))?\}", s):
        if path == "index": continue
        v, ok = get(scope, path)
        if not ok: errs.append(f"{where}: 路径不存在 {path}"); continue
        if fmt and fmt not in FORMATS: errs.append(f"{where}: 未知格式 {fmt}"); continue
        whole = s == "{" + path + "}" and key not in TEXT_PROPS   # 文字属性只收字符串，不能原样传值
        kind = ("money" if isinstance(v, dict) and "currency" in v else "date" if isinstance(v, str) and DATE.match(v)
                else "array" if isinstance(v, list) else "float" if isinstance(v, float) else "plain")
        need = {"money": "money", "date": "date", "array": "count", "float": "number"}.get(kind)
        if need and not whole and not (fmt or "").startswith(need):
            errs.append(f"{where}: {path} 是 {kind}，需要 |{need}")
SKIP = {"$comment", "$note", "$why", "assumed", "id", "kind", "ref", "only", "each", "on"}
def walk(n, scope, where, key=""):
    if isinstance(n, list):
        for x in n: walk(x, scope, where, key)
        return
    if not isinstance(n, dict): return
    where = n.get("id", where)
    if "ref" in n: walk(screen["components"][n["ref"]], scope, where); return
    if "each" in n:
        arr, ok = get(scope, n["each"])
        if not ok or not isinstance(arr, list): errs.append(f"{where}: each 只能遍历数组: {n['each']}")
        elif arr: scope = {**scope, "item": arr[0]}
    for k, v in n.items():
        if k in ("if", "disabled") and isinstance(v, str): check_use(v, scope, f"{where}.{k}")
        elif k == "cases":
            if not v or "else" not in v[-1]: errs.append(f"{where}: cases 最后一个分支必须是 else")
            for b in v:
                if "if" in b: check_use(b["if"], scope, f"{where}.cases")
                walk(b.get("then", b.get("else")), scope, where, key)
                if isinstance(b.get("then", b.get("else")), str): check_tpl(b.get("then", b.get("else")), scope, where, key)
        elif k in SKIP and k != "on": continue
        elif isinstance(v, str): check_tpl(v, scope, f"{where}.{k}", k)
        else: walk(v, scope, where, k)
walk(screen["root"], default, "root")

# ---- 条件覆盖：每个条件都要在某份 fixture 里为真、另一份里为假 ----
for c, row in table.items():
    if len(set(row.values())) < 2:
        errs.append(f"条件覆盖: {c} 在所有 fixture 里都是 {list(row.values())[0]}，有状态没法渲染给用户看")

w = max(map(len, fx))
print("条件真值表（✓ = 真）".ljust(20) + " ".join(f.ljust(w) for f in fx))
for c, row in table.items():
    print(f"{c} ({conds[c]['label']})".ljust(20) + " ".join(("✓" if row[f] else "·").ljust(w) for f in fx))
print("\n错误:" if errs else "\n错误: 无"); [print(" -", e) for e in errs]
sys.exit(1 if errs else 0)
