# Platform files

`platforms/<platform>.json`, beside `screen.json`: how one platform builds
what the description names. The description says *what*; this file says
*how, here*. `underdraw handoff <dir> --platform <platform>` reads it, and
the platforms it accepts are the files in this folder.

```json
{
  "platform": "ios",
  "language": "swift",
  "stack": "SwiftUI (iOS 17+)",
  "testId": ".accessibilityIdentifier(\"{id}\")",
  "elementTree": "XCUITest: app.descendants(matching: .any), frames by identifier",
  "tokenNaming": { "pattern": "Theme.{group}.{name}", "example": "space.4 → Theme.space._4" },
  "breakpoints": "horizontalSizeClass: .compact → compact, .regular → regular",
  "components":   { "Button": "Button; variant primary → .borderedProminent", "…": "…" },
  "actions": { "navigate": "NavigationLink(value:) / path.append", "…": "…" }
}
```

| Key | Says | Handoff uses it for |
|---|---|---|
| `platform` | the platform's name, as the file is named | |
| `language` | `typescript`, `swift` or `kotlin` | the generated view model and conditions |
| `stack` | the UI stack, in words | passed on in `rules` |
| `testId` | how a test id is written; `{id}` is the node's id | every node's `test_id` |
| `elementTree` | how a test reads the running UI's elements | passed on in `rules` |
| `tokenNaming.pattern` | a token's name in code: `{group}`, `{name}`, `{kebab-name}` | `tokens` |
| `breakpoints` | how the platform tells `compact` from `regular` | passed on in `rules` |
| `components` | per component, what builds it | every node's `implement` |
| `actions` | per action, what does it | every event's `implement` |

A component or action the description uses with no entry here, and a missing
`language`, `testId` or `tokenNaming.pattern`, is a finding: handoff still
answers, with `"ok": false` and exit 1.

## Per-node overrides

A node may say how one platform differs, under `platform`:

```json
{ "id": "nav", "component": "NavBar",
  "platform": { "web": { "hidden": true, "$why": "the site header already has these" } } }
```

`"hidden": true` leaves the node, and everything in it, out of that
platform's `nodes`; it is listed under `hidden` with its `$why`. Any other key
replaces the node's prop of that name on that platform.

## Token names

`{group}` is the token's first segment, `{name}` the rest as written,
`{kebab-name}` the rest in kebab case: `font.subhead` with
`var(--{group}-{kebab-name})` is `var(--font-subhead)`. In Swift and Kotlin a
name that starts with a digit gets `_` in front (`Theme.space._4`).
