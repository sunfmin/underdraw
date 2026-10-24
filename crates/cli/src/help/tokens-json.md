```json
{
  "color":      { "primary": "#3B5BDB", "text": "#222222", "textSecondary": "#6A6A6A",
                  "border": "#DDDDDD", "surface": "#FFFFFF" },
  "space":      { "1": 4, "2": 8, "3": 12, "4": 16, "6": 24 },
  "radius":     { "sm": 8, "md": 12, "pill": 999 },
  "font":       { "body":    { "size": 16, "weight": 400, "lineHeight": 22 },
                  "caption": { "size": 14, "weight": 400, "lineHeight": 18 }, … },
  "elevation":  { "card": { "y": 2, "blur": 8, "color": "#0000001A" } },
  "breakpoint": { "compact": [0, 743], "regular": [744, null] },
  "widths":     [390, 1440]
}
```

"widths" are the widths in points the project's descriptions are drawn and checked at, each a whole number from {const:WIDTH_LEAST} to {const:WIDTH_MOST}: check lays every state out at each, the gallery draws a state at each side by side, in the order named, and `render` draws at the first unless given --width. With no "widths", 390 (a phone) and 1120 (a desktop).

A token is named by its dotted path: "space.4", "radius.md", "font.body", "color.textSecondary", "elevation.card". A length is a number or a token naming one. Groups and names are the project's; a platform file names them in code ("tokenNaming"). The wireframe is gray: color.textSecondary draws muted, color.primary strong, color.border as a line, other colours as text. Colour itself is out of a design's reach, so the discussion stays on structure: no token or ink draws a hue. Where a meaning rides on a colour (blue for a journey, orange for a guess), let the wireframe show it in structure as well (an icon, a word, a strong fill or a dashed line), and say the colour in "$comment" on the node or in the component's "doc"; a platform file maps the token to the real colour.
