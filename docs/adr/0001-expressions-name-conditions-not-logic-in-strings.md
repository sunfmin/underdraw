# Name every condition once; keep logic out of strings

Every decision on a screen is a named condition, declared once in a small,
strictly typed grammar, and strings only interpolate. The first draft
(`drafts/airbnb-listing`) wrote "the guest has chosen dates" four ways inside
strings that are not equivalent (`booking.checkIn`, `!booking.checkIn`,
`booking.total`, `booking.nights > 0`), which already produced a bug, and its
truthiness would be translated differently by the TypeScript, Swift and Kotlin
agents.

The rules:

- **Strings only interpolate** (`{path|format}`); no operators inside strings.
- **Every decision is a named condition** in a `conditions` table, with no
  truthiness and no arithmetic. Use sites name conditions or Bool paths,
  each with an optional `!`, joined by `&&` (`walks && !picked`); `||` and
  comparisons are named.
- **Choosing between values is structural**: an ordered `cases` list that must
  end in `else`.
- **Business values are data**, not expressions: derived fields are declared in
  `data` and supplied by fixtures.

## Considered options

- **An existing expression language (CEL, JSONLogic).** CEL is well specified,
  but it brings arithmetic, macros and string functions — all of which every
  platform agent would then have to translate. JSONLogic is an AST in JSON:
  exact but tiresome for an agent to write and for a person to read.
- **A mini-language inside strings, done properly.** Fixes truthiness, but
  leaves decisions anonymous and duplicated.

## Consequences

- The checker can require every condition to be true in one fixture and false
  in another, so no state goes unrendered.
- More keystrokes for one-off conditions: anything beyond a Bool path needs a
  name.
- A use site's `&&` chain (#238) names no new decision: each term is still a
  named condition or a Bool path, so a one-off "this and not that" no longer
  needs a name of its own, while `||` and comparisons still do.
