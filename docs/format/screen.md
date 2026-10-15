# The screen

`screen.json` is one screen. Its top-level keys:

| Key | What it holds |
|---|---|
| `screen` | The screen's name, such as `ListingDetail`. |
| `title` | What a person calls it. |
| `locale` | The language its text is in, a BCP 47 tag such as `zh-CN`. Optional. |
| `tokens` | The design tokens file beside it, usually `tokens.json`. |
| `data`, `types` | The view model: see [data](data.md). |
| `fixtures` | Sample view models, one state each: see [data](data.md#fixtures). |
| `conditions` | The named yes/no tests: see [expressions](expressions.md). |
| `components` | The screen's own components, each a look with its `params` (see [components](kinds.md)), or `nodes` with typed `params`, used by name with `"with"` (ADR 0021). |
| `root` | The node tree shown once data is present. |
| `phases` | Trees shown before data arrives, such as `loading`: see [data](data.md#phases). |

## Locale

`locale` decides how text is drawn, not what it says: which glyphs a Han
character takes (Simplified, Traditional or Japanese forms) and where lines
may break. Without it, the system chooses, and a Simplified character such as
涩 may come out in a Japanese font that lacks it.

```json
"locale": "zh-CN"
```

## Breakpoints

A screen is laid out at one width, and `tokens.json` says which **breakpoint**
that width falls in:

```json
"breakpoint": { "compact": [0, 743], "regular": [744, null] }
```

Two things follow the breakpoint.

**`only`** keeps a node at one breakpoint and drops it at the other. The
listing draft shows a photo carousel and a sticky bar on a phone, and a photo
grid and a booking card beside the content on a desktop:

```json
{ "id": "photos-carousel", "component": "ImageCarousel", "only": "compact", … }
{ "id": "photos-grid",     "component": "ImageGrid",     "only": "regular", … }
```

**A responsive value** is an object keyed by both breakpoints, in place of any
property's value. The value for the breakpoint in use is taken:

```json
"columns": { "compact": ["1fr"], "regular": ["58%", "1fr"] }
```

Name both breakpoints: a value for only one would leave the other with
nothing, and `check` refuses it. A value the same at both is written once,
without the object.
