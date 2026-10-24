Component: props it reads (all optional unless named)

- `Screen`: the root; slots. gap, padding
- `Scroll`, `Stack`, `List`, `FieldGroup`: a column of children. gap, padding, radius, border, elevation
- `Card`: a column, gap 16 unless gap is given. same frame props
- `Row`: a row, items centred across. same frame props
- `NavBar`: a row 56 high, padding 0 16; leading, children, trailing
- `StickyBar`: a row with a line on top, ends pushed apart
- `Grid`: columns (default 2), gap 16
- `Columns`: columns (default 1), gap 24, children aligned at the top
- `Carousel`: a row of 280-wide cards cut at the edge
- `Section`: title, subtitle (font.heading / muted), then children; gap, padding
- `Text`: text, font (token, default font.body), color (an ink or colour token), maxLines, align (start|center|end), underline
- `Link`: text, font (default font.subhead), trailingIcon (a bool: adds ›)
- `Button`: text, variant ("primary" filled | "secondary" outlined), size ("small"), disabled (a bool: "!canAdd")
- `IconButton`: icon: a 40 circle
- `Glyph`: icon, size (default 24), font (a smaller icon), background (an ink)
- `Badge`: text: a small pill
- `Rating`: value: "★ 4.9"
- `Avatar`: size (default 40): a gray circle
- `Divider`: a 1-high line
- `ListItem`: title, detail, leadingIcon, trailingIcon, strikethrough
- `Metric`: value (font.price), label
- `EmptyState`: icon, title, detail; then children / action
- `ImageCarousel`: each over images (or items), aspectRatio ("4:3"): one photo, "1 / N"
- `ImageGrid`: each over images, radius: one large photo and four small
- `Map`: aspectRatio (default "16:9"): a gray area with a marker
- `DateRangePicker`: bind [from, to], months (1..3): the dates over month grids
- `DateField`: label, bind (a date): "add date" until set
- `TextField`: bind (a string), placeholder, lines (default 1): a box to type in, lines high: the value, or the placeholder muted while it is ""
- `Stepper`: label, bind (an int): − value +
- `PriceBreakdown`: lines (list of names), total (default "合计")
- `Skeleton`: shape: a list of "image 4:3" and "line 70%"
- `Wireframe`: screen (named), state, aspectRatio: another description's state drawn inside, scaled to the box. screen is the "screen" a description under the project (the directory holding domain.json) declares, as a navigate names it; state one of its fixtures or phases (its first fixture when not given). Drawn at the first of its widths, as tall as that state for the box's width unless aspectRatio says; its labels are left out, and nothing in it is a node of this screen. check names the node when no description declares the screen, it has no such state, or the state cannot be drawn (it has findings), and refuses a description that draws itself, at any depth (A draws B, B draws A). Draws the gallery's screens in a design of a tool that shows them:

  ```json
  { "id": "preview", "component": "Wireframe",
    "screen": "OrderList", "state": "empty" }
  ```

Each is a look component ([components](help:components)) whose params are the props above. Props a component does not read still reach `handoff` (and so the builder): write what the platform needs to know, such as "minLength", "sticky".

The built-in components use the fonts font.body, font.subhead, font.caption, font.heading and font.price: define them in tokens.json.
