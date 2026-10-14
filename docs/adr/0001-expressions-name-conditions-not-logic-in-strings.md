# Name every condition once; keep logic out of strings

The first draft (`drafts/airbnb-listing`) put logic inside strings —
`"{booking.total ? {booking.total} 共 {booking.nights} 晚 : 添加日期查看价格}"` —
and tested values for truthiness. One decision, "the guest has chosen dates",
was written four ways (`booking.checkIn`, `!booking.checkIn`, `booking.total`,
`booking.nights > 0`) that are not equivalent, which already produced a bug: in
the "dates unavailable" state the sticky bar asked the guest to add dates they
had already added. Truthiness also means something different in TypeScript,
Swift and Kotlin, and three agents would translate it three ways.

We decided:

- **Strings only interpolate** (`{path|format}`); no operators inside strings.
- **Every decision is a named condition**, declared once in a `conditions`
  table in a small, strictly typed grammar with no truthiness and no arithmetic.
  Use sites name a condition or a Bool path.
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

- A person can be shown the screen per named state ("Dates chosen: no").
- The checker can require every condition to be true in one fixture and false
  in another, so no state goes unrendered.
- Each condition maps to one named property in the real code, so web, iOS and
  Android decide the same thing the same way.
- More keystrokes for one-off conditions: anything beyond a Bool path needs a
  name.
