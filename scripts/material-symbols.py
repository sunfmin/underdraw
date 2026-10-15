"""Makes the icon fonts underdraw draws with from Material Symbols' variable font.

    uv run --with fonttools,brotli scripts/material-symbols.py <dir>

<dir> holds Google's release (github.com/google/material-design-icons,
variablefont/): outlined.ttf, outlined.codepoints and LICENSE. Writes:

- crates/format/material-symbols.codepoints: every icon name and its
  character, which `check` reads names from and `glyph` draws by.
- crates/cli/fonts/material-symbols.ttf: the outlined icons (FILL 0), at
  their own characters (U+E000..U+F8FF).
- crates/cli/fonts/material-symbols-fill.ttf: the solid icons (FILL 1), each
  moved to U+F0000 + its character, so text falls back to it for a `.fill`
  icon and to the outlined font otherwise, with no style telling them apart.

Both at weight 400, grade 0, optical size 24; only the icons, without the
ligatures that spell an icon from its name.
"""

import shutil
import sys
from pathlib import Path

from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
from fontTools.varLib.instancer import instantiateVariableFont

FILL_PLANE = 0xF0000
ROOT = Path(__file__).resolve().parent.parent


def instance(source: Path, fill: int, family: str, shift: int) -> TTFont:
    font = instantiateVariableFont(TTFont(source), {"FILL": fill, "wght": 400, "GRAD": 0, "opsz": 24})
    icons = sorted({c for c in font.getBestCmap() if 0xE000 <= c <= 0xF8FF})
    options = Options()
    options.layout_features = []
    options.name_IDs = ["*"]
    options.notdef_outline = True
    subsetter = Subsetter(options)
    subsetter.populate(unicodes=icons)
    subsetter.subset(font)
    if shift:
        moved = {c + shift: g for c, g in font.getBestCmap().items()}
        table = CmapSubtable.newSubtable(12)
        table.platformID, table.platEncID, table.language, table.cmap = 3, 10, 0, moved
        font["cmap"].tables = [table]
    for record in font["name"].names:
        if record.nameID in (1, 4, 16):
            record.string = family
        if record.nameID == 6:
            record.string = family.replace(" ", "")
    return font


def main() -> None:
    source = Path(sys.argv[1])
    fonts = ROOT / "crates/cli/fonts"
    fonts.mkdir(exist_ok=True)
    instance(source / "outlined.ttf", 0, "Material Symbols", 0).save(fonts / "material-symbols.ttf")
    instance(source / "outlined.ttf", 1, "Material Symbols Fill", FILL_PLANE).save(fonts / "material-symbols-fill.ttf")
    shutil.copy(source / "LICENSE", fonts / "LICENSE-material-symbols")
    shutil.copy(source / "outlined.codepoints", ROOT / "crates/format/material-symbols.codepoints")


if __name__ == "__main__":
    main()
