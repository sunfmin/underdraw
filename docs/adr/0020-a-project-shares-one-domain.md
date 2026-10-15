# A project shares one domain: types, conditions and operations

A project's screens and components share one **domain**, `domain.json` at
the project's root: its `types`, `conditions` and `operations`, each
declared once and used by name. This replaces the rule of
[ADR 0009](0009-operations-live-in-the-screen-and-are-called-by-name.md)
that an operation is one screen's own: once a component is shared
([ADR 0021](0021-kinds-become-components-one-screens-or-the-projects.md)),
an `OrderRow` reads an `Order` and calls `cancelOrder` on every screen it
is on, and each screen declaring them again is the drift that rule
expected to revisit. The CLI derived from the operations
([ADR 0011](0011-a-cli-is-derived-from-the-operations.md)) now has its
one source.

The decisions:

- **A screen may add its own** types, conditions and operations (a view
  state wrapper such as `DocRow { doc, menuOpen }`), but never one of a
  name the domain declares: `check` refuses the shadowing.
- **A shared condition is declared on a type** (`Order.cancellable`) and
  used through a value of it (`{item.cancellable}`), since the domain has
  no screen's `data` to start a path from. A condition over a screen's
  data stays that screen's. An operation's `requires` may name either.

## Considered options

- **Each component carries its types and operations**, and screens import
  them. `Order` would then be declared by every component that reads one.
- **Keep everything per screen**, and check that same-named declarations
  agree. It catches the drift after it is written, rather than leaving one
  place to write.
