Direction comes from the component: Stack, List, Scroll, Screen, Card, FieldGroup are columns; Row, NavBar, StickyBar, Carousel are rows; Grid and Columns are grids. These containers take from the node:

- `"gap"`: a length: 12 or a token "space.3"
- `"padding"`: a length, [vertical, horizontal] or [top, right, bottom, left]
- `"radius"`: a length or "radius.md"; also clips the children
- `"border"`: a colour token: "color.border"
- `"elevation"`: an elevation token: outlined in the wireframe
- `"columns"`: Grid and Columns only: a count, or tracks ["58%", "1fr"]
- `"dividers"`: Grid and Columns only: true draws a line between the cells, in the line ink, for panes split without a border round each

Each cell of a Grid or Columns is as tall as its row, so its border and fill reach the row's foot.

Section takes "gap" and "padding" too. Width, grow, alignment and sizes are never written on a node: they belong to a component's look ([looks](help:looks)). When a built-in component does not lay out as needed, define one.
