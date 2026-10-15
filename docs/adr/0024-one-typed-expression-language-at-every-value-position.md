---
status: proposed
---

# One typed expression language at every value position

A description is read by five implementations: the TypeScript, Swift and
Kotlin builds, underdraw itself (`format` and `layout`), and the gallery's
own Rust UI. Each must read a value the same way. Today a value is read by
whichever of several small dialects its position happens to use, so the
same text means different things in different places, and a mistake in one
dialect passes check silently. We decide to have **one expression language,
specified once, typed at every position**. It keeps
[ADR 0001](0001-expressions-name-conditions-not-logic-in-strings.md)'s rules
for data, adds a narrow length arithmetic in looks, and is complete enough
for underdraw's own components to be written in it, so the gallery draws
them by interpreting their looks instead of re-implementing them in Rust.

## What there is now

Seven ways a value is read, each with its own parser or special case:

| Position | What it accepts |
|---|---|
| a condition's `when` | the full grammar: `&&`, `\|\|`, `!`, comparisons |
| a use site (`if`, `disabled`, …) | a second parser: names or Bool paths, `!`, `&&` only |
| a look's `if` | the full grammar again, over props instead of paths |
| a string | `{path\|filter}` interpolation only |
| a choice | `cases`, structural, chosen in several separate places (instancing, the format's and the layout's look, handoff) |
| a look's length | a number, a token or a percentage, no arithmetic |
| a prop by its key | `disabled` is read as a condition because of its name; any other bool prop is read as text |

What went wrong in practice, all passing check (#239, #282, and the gallery
toolbar redesign):

- A look's own bool param (`dim`) could not take `!bar.canBack`. Written
  anyway, it was read as a non-empty string and held everywhere.
- Spelled out as `cases` instead, its polarity came out reversed. A nested
  `cases` as an `if` was accepted and read as false.
- A tree's indent was a per-depth `cases` table plus a second hard-coded
  indent in another component, kept in step by eye (#288).

## The decision

- **One grammar and one AST**, parsed once and used by check, render,
  handoff and the gallery. A condition's `when`, a use site, a look's `if`,
  a `cases` branch and a template are positions in it, not dialects.
- **Every value position has a type.** Bool, text, a length, an ink, an
  icon, an enum. A bool position takes an expression (`"dim":
  "!bar.canBack"`), whatever its key is called. A text position only
  interpolates. A value of the wrong type is a finding at its position,
  never a literal by default.
- **ADR 0001 holds for data.** Decisions over data are named conditions.
  There is no truthiness, and no arithmetic over data or the view model:
  business values stay data the fixtures supply, because five
  implementations would round, divide and overflow five ways.
- **Lengths may be computed, in looks only.** Over whole points, tokens and
  bounded ints: `length × int`, `length + length`. No division, no
  floating point, and every operand bounded (a `depth` of `int 0..4`), so
  the result is proved in range rather than trusted. This is
  presentation, never a business value.
- **underdraw's own components are written in it.** A built-in component
  that needs more than a look can say (a tree's indent per level, its
  folding) is a reason to grow the language, not to hard-code the
  component in Rust. The gallery draws its UI by laying out these looks
  through `underdraw-layout`, so the Rust UI has one implementation: the
  reference one.
- **Conformance travels with handoff.** For every fixture, handoff states
  what each expression evaluates to, so a platform build can be tested
  against the reference, not only against pictures.

## Considered options

- **Keep the dialects, patch each hole.** Cheapest per fix, but each new
  position is another dialect, and the faults found so far were all at the
  seams between them.
- **A new surface syntax instead of JSON.** Easier to read, but agents
  write JSON reliably, every tool already reads it, and the problem is
  semantics, not syntax.
- **An existing language (CEL, JSONLogic).** As in ADR 0001: CEL brings
  general arithmetic and string functions that every platform would
  translate, and JSONLogic is an AST nobody wants to write by hand.
- **Semantic primitives only, no arithmetic.** Each primitive (a tree, an
  indent) would then be hand-written in the gallery's Rust as well as in
  underdraw. With a small typed length arithmetic, the primitive is a look
  written once. A native platform still sees its name and maps it to a
  native control, so the intent is kept.

## Consequences

- Every value position in FORMAT says its type, in one EXPRESSIONS section
  that replaces the rules now spread over CONDITIONS, TEMPLATES, CASES and
  LOOKS.
- Descriptions that relied on a literal being read as text in a bool
  position (or on `disabled` being special) get a finding. Each such
  finding names the expression to write instead.
- The gallery's hand-written views shrink, region by region, as their looks
  can be written in the language.
