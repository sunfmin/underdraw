# /// script
# requires-python = ">=3.12"
# ///
# Prototype checker for docs/format/data.md and docs/format/expressions.md.
# Throwaway: the real one is the Rust CLI. Usage: uv run prototype/check.py drafts/airbnb-listing
import copy, json, pathlib, re, sys
from dataclasses import dataclass, replace

errs: list[str] = []
def err(where, msg): errs.append(f"{where}: {msg}")

# ---------- 类型 ----------
BUILTIN = {"string", "int", "number", "bool", "date", "url", "image", "icon", "money"}
FIELD = re.compile(r"^(?P<base>[A-Za-z]\w*)(?P<arr>\[\])?(?P<null>\?)?"
                   r"(?:\s+(?P<lo>-?\d+(?:\.\d+)?)?\.\.(?P<hi>-?\d+(?:\.\d+)?)?)?"
                   r"(?:\s*=\s*(?P<default>.+))?$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

@dataclass(frozen=True)
class T:
    base: str
    arr: bool = False
    null: bool = False
    lo: float | None = None
    hi: float | None = None
    has_default: bool = False
    default: object = None

    def elem(self): return replace(self, arr=False, null=False, lo=None, hi=None, has_default=False, default=None)
    def __str__(self): return self.base + ("[]" if self.arr else "") + ("?" if self.null else "")

def parse_field(spec, where) -> T | None:
    if isinstance(spec, dict):
        unknown = set(spec) - {"type", "doc", "derived"}
        if unknown: err(where, f"字段只能有 type / doc / derived，多了 {sorted(unknown)}")
        spec = spec.get("type", "")
    m = FIELD.match(spec.strip())
    if not m: err(where, f"看不懂的类型 {spec!r}"); return None
    g = m.groupdict()
    t = T(g["base"], bool(g["arr"]), bool(g["null"]),
          float(g["lo"]) if g["lo"] else None, float(g["hi"]) if g["hi"] else None)
    if g["default"] is not None:
        try: t = replace(t, has_default=True, default=json.loads(g["default"]))
        except json.JSONDecodeError: err(where, f"缺省值不是 JSON 字面量: {g['default']}")
    if t.null and not t.has_default: t = replace(t, has_default=True, default=None)   # T? 缺省为 null
    return t

d = pathlib.Path(sys.argv[1]); screen = json.loads((d / "screen.json").read_text())
types: dict[str, dict[str, T] | list[str]] = {}
for name, body in screen.get("types", {}).items():
    if not re.fullmatch(r"[A-Z]\w*", name): err(f"types.{name}", "类型名必须大写开头")
    if isinstance(body, str):
        m = re.fullmatch(r"enum\((.+)\)", body)
        if m: types[name] = [x.strip() for x in m.group(1).split(",")]
        else: err(f"types.{name}", "命名类型只能是对象或 enum(...)")
    else:
        types[name] = {f: t for f, s in body.items() if (t := parse_field(s, f"types.{name}.{f}"))}
types["$Data"] = {f: t for f, s in screen["data"].items() if (t := parse_field(s, f"data.{f}"))}
for name, body in types.items():
    for f, t in (body.items() if isinstance(body, dict) else []):
        if t.base not in BUILTIN and t.base not in types: err(f"types.{name}.{f}", f"未知类型 {t.base}")
        if t.arr and t.has_default and t.default is not None: err(f"types.{name}.{f}", "数组不能有缺省值")

# ---------- fixture：合并、补缺省值、按类型校验 ----------
def merge(a, b):
    for k, v in b.items():
        a[k] = merge(a.get(k, {}), v) if isinstance(v, dict) and isinstance(a.get(k), dict) else v
    return a

def in_range(t, n, where, what):
    if (t.lo is not None and n < t.lo) or (t.hi is not None and n > t.hi):
        err(where, f"{what} {n} 超出范围 {t.lo if t.lo is not None else ''}..{t.hi if t.hi is not None else ''}")

def validate(v, t: T, where):
    if v is None:
        if not t.null: err(where, f"{t} 不能为 null")
        return v
    if t.arr:
        if not isinstance(v, list): err(where, f"应为数组 {t}"); return v
        in_range(t, len(v), where, "元素个数")
        return [validate(x, t.elem(), f"{where}[{i}]") for i, x in enumerate(v)]
    b = t.base
    ok = {"string": isinstance(v, str), "url": isinstance(v, str), "image": isinstance(v, str), "icon": isinstance(v, str),
          "int": isinstance(v, int) and not isinstance(v, bool),
          "number": isinstance(v, (int, float)) and not isinstance(v, bool),
          "bool": isinstance(v, bool), "date": isinstance(v, str) and bool(DATE.match(v)),
          "money": isinstance(v, dict) and set(v) == {"amount", "currency"}}.get(b)
    if ok is not None:
        if not ok: err(where, f"{v!r} 不是 {b}")
        elif b == "string": in_range(t, len(v), where, "长度")
        elif b in ("int", "number"): in_range(t, v, where, "值")
        return v
    body = types[b]
    if isinstance(body, list):
        if v not in body: err(where, f"{v!r} 不是 {b} 的成员 {body}")
        return v
    if not isinstance(v, dict): err(where, f"应为 {b} 对象"); return v
    for k in set(v) - set(body): err(where, f"{b} 没有字段 {k}")
    out = {}
    for f, ft in body.items():
        if f in v: out[f] = validate(v[f], ft, f"{where}.{f}")
        elif ft.has_default: out[f] = copy.deepcopy(ft.default)
        else: err(where, f"缺少必填字段 {f}（{ft}）")
    return out

raw, bases = {}, {}
for name, f in screen["fixtures"].items():
    f = copy.deepcopy(f); base = f.pop("$patch", None); bases[name] = base
    raw[name] = merge(copy.deepcopy(raw[base]), f) if base else f
fx = {}
for n, f in raw.items():
    before = len(errs); fx[n] = validate(f, T("$Data"), f"fixtures.{n}")
    if bases[n]:   # 从基础 fixture 继承来的错误只报一次
        seen = set(errs[:before])
        errs[before:] = [e for e in errs[before:] if e.replace(f"fixtures.{n}", f"fixtures.{bases[n]}", 1) not in seen]

# ---------- 路径 ----------
# scope: 根名 → 类型。路径类型里，任何一段可为空，整条路径就可能为空。
def path_type(path, scope) -> tuple[T | None, list[str]]:
    segs = path.split("."); t = scope.get(segs[0]); nullable = []
    if t is None: return None, []
    for i, seg in enumerate(segs[1:], 1):
        if t.null: nullable.append(".".join(segs[:i]))
        body = types.get(t.base) if not t.arr else None
        if not isinstance(body, dict) or seg not in body: return None, []
        t = body[seg]
    if t.null: nullable.append(path)
    return t, nullable

def get(data, path):
    v = data
    for seg in path.split("."):
        if not isinstance(v, dict) or seg not in v: return None
        v = v[seg]
    return v

ROOT = {k: t for k, t in types["$Data"].items()}

# ---------- 条件：语法树 → 类型检查 → 求值 ----------
conds = screen.get("conditions", {})
TOK = re.compile(r'\s*(\|\||&&|==|!=|<=|>=|<|>|!|\(|\)|"[^"]*"|-?\d+(?:\.\d+)?|[A-Za-z_][\w.]*)')
def parse_when(s):
    toks, i = [], 0
    while i < len(s):
        m = TOK.match(s, i)
        if not m or m.end() == i:
            if s[i:].strip(): raise SyntaxError(f"无法识别 {s[i:]!r}")
            break
        toks.append(m.group(1)); i = m.end()
    pos = [0]
    peek = lambda: toks[pos[0]] if pos[0] < len(toks) else None
    def take(): pos[0] += 1; return toks[pos[0] - 1]
    def atom():
        t = take()
        if t == "(":
            e = expr()
            if take() != ")": raise SyntaxError("缺少 )")
            return e
        if t.startswith('"'): return ("lit", "string", t[1:-1])
        if re.fullmatch(r"-?\d+(\.\d+)?", t): return ("lit", "number", float(t))
        if t in ("true", "false"): return ("lit", "bool", t == "true")
        if t == "null": return ("lit", "null", None)
        if t in conds: return ("cond", t)
        return ("path", t)
    def compare():
        a = atom()
        if peek() in ("==", "!=", "<", "<=", ">", ">="): op = take(); return ("cmp", op, a, atom())
        return a
    def not_(): return ("not", not_()) if peek() == "!" and take() else compare()
    def chain(op, sub):
        items = [sub()]
        while peek() == op: take(); items.append(sub())
        return items[0] if len(items) == 1 else (op, items)
    def and_(): return chain("&&", not_)
    def expr(): return chain("||", and_)
    e = expr()
    if pos[0] != len(toks): raise SyntaxError(f"多余内容 {toks[pos[0]:]}")
    return e

ast = {}
for c, body in conds.items():
    try: ast[c] = parse_when(body["when"])
    except SyntaxError as e: err(f"conditions.{c}", str(e))

def atom_type(a):
    if a[0] == "lit": return a[1]
    if a[0] == "cond": return "bool"
    t, _ = path_type(a[1], ROOT)
    return t

def typecheck(e, where, stack=()):
    k = e[0]
    if k in ("&&", "||"):
        for x in e[1]: typecheck(x, where, stack)
    elif k == "not": typecheck(e[1], where, stack)
    elif k == "cond":
        if e[1] in stack: err(where, f"循环引用 {' → '.join(stack + (e[1],))}")
    elif k == "path":
        t = atom_type(e)
        if t is None: err(where, f"路径不存在 {e[1]}")
        elif t.base != "bool" or t.arr or t.null: err(where, f"{e[1]} 是 {t}，不能直接当条件（没有真假值转换）")
    elif k == "lit":
        if e[1] != "bool": err(where, f"字面量 {e[2]!r} 不能当条件")
    elif k == "cmp":
        _, op, a, b = e
        ta, tb = atom_type(a), atom_type(b)
        for x, tx in ((a, ta), (b, tb)):
            if x[0] == "path" and tx is None: err(where, f"路径不存在 {x[1]}"); return
        def kind(t):
            if isinstance(t, str): return t
            if t.arr: return "array"
            return "enum" if isinstance(types.get(t.base), list) else "number" if t.base in ("int", "number") else t.base
        ka, kb = kind(ta), kind(tb)
        if "null" in (ka, kb):
            other = tb if ka == "null" else ta
            if op not in ("==", "!="): err(where, "null 只能用 == / != 比较")
            elif isinstance(other, T) and not other.null: err(where, f"{other} 不可能为 null，这个比较恒定")
            return
        if "enum" in (ka, kb):
            et, lit = (ta, b) if ka == "enum" else (tb, a)
            if lit[0] != "lit" or lit[1] != "string": err(where, "枚举只能和字符串字面量比较")
            elif lit[2] not in types[et.base]: err(where, f"{lit[2]!r} 不是 {et.base} 的成员 {types[et.base]}")
            return
        if ka != kb: err(where, f"不能比较 {ta} 和 {tb}")
        elif op in ("<", "<=", ">", ">=") and ka not in ("number", "date"): err(where, f"{ka} 不能比大小")

for c, e in ast.items(): typecheck(e, f"conditions.{c}", (c,))

def evaluate(e, data):
    k = e[0]
    if k == "&&": return all(evaluate(x, data) for x in e[1])
    if k == "||": return any(evaluate(x, data) for x in e[1])
    if k == "not": return not evaluate(e[1], data)
    if k == "cond": return evaluate(ast[e[1]], data)
    if k == "lit": return e[2]
    if k == "path": return get(data, e[1])
    _, op, a, b = e
    x, y = evaluate(a, data), evaluate(b, data)
    if op == "==": return x == y
    if op == "!=": return x != y
    if x is None or y is None: return False
    return {"<": x < y, "<=": x <= y, ">": x > y, ">=": x >= y}[op]

def guards(e) -> set[str]:
    """条件为真时，保证不为 null 的路径：顶层 && 链里的 `p != null`，以及引用的具名条件的同类保证。"""
    k = e[0]
    if k == "&&": return set().union(*(guards(x) for x in e[1]))
    if k == "cond": return guards(ast[e[1]]) if e[1] in ast else set()
    if k == "cmp" and e[1] == "!=" and {e[2][0], e[3][0]} == {"path", "lit"}:
        p, l = (e[2], e[3]) if e[2][0] == "path" else (e[3], e[2])
        if l[1] == "null": return {p[1]}
    return set()

# ---------- 节点 ----------
FORMATS = {"money": {"money"}, "date": {"date"}, "date:short": {"date"}, "count": {"array"}}
NUMBER_FMT = re.compile(r"number:(\d)(?:-(\d))?$")   # number:2 固定两位；number:0-1 最少 0 位、最多 1 位
PLACEHOLDER = re.compile(r"\{([\w.]+)(?:\|([\w:-]+))?\}")
TEXT_PROPS = {"text", "title", "subtitle", "label", "detail"}
USE = re.compile(r"(!?)([A-Za-z_][\w.]*)")
SKIP = {"$comment", "$note", "$why", "assumed", "id", "kind", "ref", "only", "each"}

def use_site(u, scope, where) -> set[str]:
    m = USE.fullmatch(u)
    if not m: err(where, f"使用处只能写 !?条件名 或 Bool 路径，得到 {u!r}"); return set()
    neg, n = m.groups()
    if n in conds: return set() if neg else guards(ast.get(n, ("lit", "bool", True)))
    t, _ = path_type(n, scope)
    if t is None: err(where, f"未知条件或路径 {n}")
    elif t.base != "bool" or t.arr or t.null: err(where, f"{n} 是 {t}，请改成具名条件")
    return set()

def template(s, scope, guarded, where, key):
    if re.search(r"\{[^}|]*[?:&=<>!][^}]*\}", s): err(where, f"模板里不能有运算 {s!r}"); return
    for ph in re.findall(r"\{[^{}]*\}", s):          # 兜底：不合语法的占位符直接报错，不静默跳过
        if not PLACEHOLDER.fullmatch(ph): err(where, f"占位符写法不对 {ph}")
    for path, fmt in PLACEHOLDER.findall(s):
        if path == "index": continue
        t, nullable = path_type(path, scope)
        if t is None: err(where, f"路径不存在 {path}"); continue
        unguarded = [p for p in nullable if p not in guarded]
        if unguarded: err(where, f"{path} 可能为空（{', '.join(unguarded)}），需要外层条件写明 != null")
        if s == "{" + path + "}" and key not in TEXT_PROPS: continue      # 原样传值
        kind = "array" if t.arr else t.base
        if not t.arr and (isinstance(types.get(t.base), (list, dict)) or kind in ("bool", "image")):
            err(where, f"{path} 是 {t}，不能直接显示成文字，用 cases 选文案"); continue
        if fmt and (m := NUMBER_FMT.match(fmt)):
            if kind not in ("number", "int"): err(where, f"{path} 是 {t}，不能用 |{fmt}")
            elif m.group(2) and int(m.group(2)) < int(m.group(1)): err(where, f"|{fmt}：最多位数小于最少位数")
        elif fmt:
            if fmt not in FORMATS: err(where, f"未知格式 {fmt}")
            elif kind not in FORMATS[fmt]: err(where, f"{path} 是 {t}，不能用 |{fmt}")
        elif kind in ("money", "date", "number", "array"):
            need = "number:N 或 number:N-M" if kind == "number" else "/".join(f for f, k in FORMATS.items() if kind in k)
            err(where, f"{path} 是 {t}，需要写格式（{need}）")

components = screen.get("components", {})
def walk(n, scope, guarded, where, key=""):
    if isinstance(n, list):
        for x in n: walk(x, scope, guarded, where, key)
        return
    if not isinstance(n, dict): return
    where = n.get("id", where)
    if "ref" in n:
        if n["ref"] not in components: err(where, f"ref 不存在 {n['ref']}")
        else: walk(components[n["ref"]], scope, guarded, where)
        return
    if "if" in n: guarded = guarded | use_site(n["if"], scope, f"{where}.if")
    if "each" in n:
        t, nullable = path_type(n["each"], scope)
        if t is None or not t.arr: err(where, f"each 只能遍历数组，{n['each']} 是 {t}")
        else:
            if [p for p in nullable if p not in guarded]: err(where, f"{n['each']} 可能为空")
            scope = {**scope, "item": t.elem(), "index": T("int")}
    for k, v in n.items():
        if k in SKIP or k == "if": continue
        if k == "disabled" and isinstance(v, str): use_site(v, scope, f"{where}.disabled")
        elif k == "cases":
            if not v or "else" not in v[-1]: err(where, "cases 最后一个分支必须是 else")
            for b in v:
                g = guarded | (use_site(b["if"], scope, f"{where}.cases") if "if" in b else set())
                val = b.get("then", b.get("else"))
                if isinstance(val, str): template(val, scope, g, where, key)
                else: walk(val, scope, g, where, key)
        elif isinstance(v, str): template(v, scope, guarded, f"{where}.{k}", k)
        else: walk(v, scope, guarded, where, k)

walk(screen["root"], ROOT, set(), "root")
for phase, tree in {k: v for k, v in screen.get("phases", {}).items() if not k.startswith("$")}.items():
    walk(tree, {}, set(), f"phases.{phase}")          # 阶段不读 data：作用域为空

# ---------- 条件覆盖 ----------
table = {c: {f: bool(evaluate(e, data)) for f, data in fx.items()} for c, e in ast.items()} if not errs else {}
for c, row in table.items():
    if len(set(row.values())) < 2:
        err(f"conditions.{c}", f"在所有 fixture 里都是 {list(row.values())[0]}，有状态渲染不出来")

if table:
    w = max(map(len, fx))
    print("条件真值表（✓ = 真）")
    print(" " * 14 + " ".join(f.ljust(w) for f in fx))
    for c, row in table.items():
        print(c.ljust(14) + " ".join(("✓" if row[f] else "·").ljust(w) for f in fx) + "  " + conds[c].get("label", ""))
    print(f"阶段：{', '.join(k for k in screen.get('phases', {}) if not k.startswith('$'))}（不读数据，总能渲染）")
print("\n错误:" if errs else "\n错误: 无")
for e in errs: print(" -", e)
sys.exit(1 if errs else 0)
