//! Text, set with parley in the system's fonts: measured for layout, and
//! the same layout drawn into the picture, so what was measured is what is
//! drawn.

use std::borrow::Cow;

use parley::fontique::Blob;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Language, Layout, LayoutContext,
    LineHeight, StyleProperty,
};
use underdraw_layout::{Align, Font, Measure, Size};

/// The fonts a wireframe is set in: the system's interface font, then, for
/// Han characters, the fonts of the locale's writing on macOS, Windows and
/// Linux in turn. The system's own fallback is not enough: parley cannot
/// draw PingFang, and what it falls back to lacks Simplified forms such as 涩.
/// Last come the icon fonts, the only ones holding the characters icons are
/// drawn as (underdraw_format::icons).
fn families(locale: Option<&str>) -> &'static str {
    let tag = locale.unwrap_or("").to_ascii_lowercase();
    if tag.starts_with("ja") {
        "system-ui, Hiragino Sans, Yu Gothic, Noto Sans CJK JP, sans-serif, Material Symbols, Material Symbols Fill"
    } else if tag.starts_with("zh-hant") || tag.starts_with("zh-tw") || tag.starts_with("zh-hk") {
        "system-ui, Heiti TC, Microsoft JhengHei, Noto Sans CJK TC, sans-serif, Material Symbols, Material Symbols Fill"
    } else {
        "system-ui, Hiragino Sans GB, Heiti SC, Microsoft YaHei, Noto Sans CJK SC, sans-serif, Material Symbols, Material Symbols Fill"
    }
}

/// Material Symbols, outlined and solid, made by scripts/material-symbols.py.
const ICON_FONTS: [&[u8]; 2] =
    [include_bytes!("../fonts/material-symbols.ttf"), include_bytes!("../fonts/material-symbols-fill.ttf")];

pub struct Typesetter {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    /// The screen's language: which glyphs a character takes (Simplified
    /// or Japanese Han) and where lines may break.
    locale: Option<Language>,
    families: &'static str,
}

impl Typesetter {
    /// Text in `locale`, a BCP 47 tag such as `zh-CN`; none leaves the
    /// choice of glyphs to the system.
    pub fn new(locale: Option<&str>) -> Result<Typesetter, String> {
        let families = families(locale);
        let locale = match locale {
            Some(tag) => Some(Language::parse(tag).map_err(|e| {
                format!(
                    "locale {tag:?} is not a BCP 47 tag ({e:?}); write one such as \"zh-CN\" or \"en-US\""
                )
            })?),
            None => None,
        };
        let mut fonts = FontContext::new();
        for font in ICON_FONTS {
            fonts.collection.register_fonts(Blob::new(std::sync::Arc::new(font)), None);
        }
        Ok(Typesetter { fonts, layouts: LayoutContext::new(), locale, families })
    }

    /// `text` in `font`, broken into lines no wider than `max_width`.
    pub fn set(&mut self, text: &str, font: Font, max_width: Option<f32>, align: Align) -> Layout<()> {
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(Cow::Borrowed(self.families))));
        builder.push_default(StyleProperty::FontSize(font.size));
        builder.push_default(StyleProperty::Locale(self.locale));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(font.weight))));
        builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(font.line_height)));
        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);
        let alignment = match align {
            Align::Start => Alignment::Start,
            Align::Center => Alignment::Center,
            Align::End => Alignment::End,
        };
        layout.align(alignment, AlignmentOptions::default());
        layout
    }
}

impl Measure for Typesetter {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size {
        let layout = self.set(text, font, max_width, Align::Start);
        // Rounded up, so that set again in the box it was given, the text
        // breaks where it broke here.
        Size { width: layout.width().ceil(), height: layout.height().ceil() }
    }
}
