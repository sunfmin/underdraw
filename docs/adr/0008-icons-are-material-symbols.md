# Icons are Material Symbols names

An icon in a description is named as Material Symbols names it
(`arrow_back`, `favorite`), with `.fill` after the name for the solid form,
and the wireframe draws that icon from Material Symbols' own font. One
description is built on three platforms, so its icon names must be ones
every platform may use. Material Symbols is open (Apache 2.0) and native
on Android, a web font on the web, and SVG on iOS. Before this, each draft
made up its own names, and nothing checked them.

## Considered options

- **SF Symbols names.** The most native set on iOS, but Apple's licence
  keeps the symbols on Apple platforms, so the web and Android could not
  draw them.
- **Lucide or Phosphor.** Open and tidy, but made for the web, with no
  native form on Android or iOS.
- **underdraw's own vocabulary, mapped per platform.** Each name would need
  a row for every platform, and someone to keep the rows in step. With
  Material's names, the only map left is optional: an iOS team that wants
  SF Symbols maps names in `platforms/ios.json`.
- **Keep stand-in characters (◇ ◆) in the wireframe.** No font to ship, but
  a person reading the wireframe could not tell a door from a calendar.

## Consequences

- `check` refuses a name Material Symbols does not have, wherever it is
  written: a node's `icon` or `…Icon`, a fixture value of type `icon`, or a
  look in kinds.json. The names come from Google's codepoints file
  (`crates/format/material-symbols.codepoints`), the only list of them.
- kinds.json has no `icons` table. A project that needs an icon Material
  Symbols lacks has no way to add one yet.
- underdraw's binary carries two fonts made from Google's release by
  `scripts/material-symbols.py`: outlined, and solid. The solid font keeps
  each icon at U+F0000 plus its outlined character, so text falls back to it
  for `.fill`, and nothing in the layout has to tell the two apart.
- Updating to a newer Material release is running the script again; names
  Google removes become faults in `check`.
