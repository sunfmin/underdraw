//! Icon names: Material Symbols' (fonts.google.com/icons), and `.fill` after
//! one for its solid form (`favorite.fill`). Why:
//! [ADR 0008](../../../docs/adr/0008-icons-are-material-symbols.md).

/// Every Material Symbols name and its character, one `name hex` a line, as
/// Google releases it (scripts/material-symbols.py copies it here).
const CODEPOINTS: &str = include_str!("../material-symbols.codepoints");

/// Where the solid font holds each icon: past the outlined one's character
/// by this much, in a plane of its own.
const FILL_PLANE: u32 = 0xF0000;

/// What a name with `.fill` after it asks for.
const FILL: &str = ".fill";

/// The character `icon` is drawn as in underdraw's icon fonts; none when it
/// is not an icon's name.
pub fn glyph(icon: &str) -> Option<char> {
    let (name, shift) = match icon.strip_suffix(FILL) {
        Some(name) => (name, FILL_PLANE),
        None => (icon, 0),
    };
    let code = codepoint(name)?;
    char::from_u32(code.checked_add(shift)?)
}

/// Whether `icon` names a Material Symbols icon, or its solid form.
pub fn known(icon: &str) -> bool {
    glyph(icon).is_some()
}

/// What a description is told when `icon` is not one.
pub(crate) fn unknown(icon: &str) -> String {
    format!(
        "{icon} is not a Material Symbols icon: use its name from fonts.google.com/icons (arrow_back, favorite), with .fill for the solid form"
    )
}

fn codepoint(name: &str) -> Option<u32> {
    for line in CODEPOINTS.lines() {
        if let Some((listed, hex)) = line.split_once(' ')
            && listed == name
        {
            return u32::from_str_radix(hex.trim(), 16).ok();
        }
    }
    None
}

/// Whether a node's or look's `key` holds an icon's name.
pub(crate) fn is_icon_key(key: &str) -> bool {
    key == "icon" || key.ends_with("Icon")
}
