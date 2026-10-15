//! Text, set with parley in the system's fonts: measured for layout, and
//! the same layout drawn into the picture, so what was measured is what is
//! drawn.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use std::sync::{Mutex, OnceLock};

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontContext, FontFamily, FontWeight, Language, Layout,
    LayoutContext, LineHeight, Selection, StyleProperty,
};
use underdraw_layout::{Align, Font, Measure, Rect, Size};

/// The fonts a wireframe is set in: the system's interface font, then, for
/// Han characters, the fonts of the locale's writing on macOS, Windows and
/// Linux in turn. The system's own fallback is not enough: parley cannot
/// draw PingFang, and what it falls back to lacks Simplified forms such as 涩.
fn families(locale: Option<&str>) -> &'static str {
    let tag = locale.unwrap_or("").to_ascii_lowercase();
    if tag.starts_with("ja") {
        "system-ui, Hiragino Sans, Yu Gothic, Noto Sans CJK JP, sans-serif"
    } else if tag.starts_with("zh-hant") || tag.starts_with("zh-tw") || tag.starts_with("zh-hk") {
        "system-ui, Heiti TC, Microsoft JhengHei, Noto Sans CJK TC, sans-serif"
    } else {
        "system-ui, Hiragino Sans GB, Heiti SC, Microsoft YaHei, Noto Sans CJK SC, sans-serif"
    }
}

/// The fonts an icon is set in (underdraw_format::icons): on their own, so
/// that its line takes their heights and the icon sits in the middle of it.
const ICON_FAMILIES: &str = "Material Symbols, Material Symbols Fill";

/// Whether `text` is icons only: every character one the icon fonts hold,
/// in the Private Use Areas, where no other font puts anything.
fn is_icons(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}'))
}

/// The system's fonts and the icon fonts, found once per process: finding
/// them takes longer than laying out a page, and every [`Typesetter`] takes
/// a clone, which shares what was found.
static FONTS: OnceLock<Mutex<FontContext>> = OnceLock::new();

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
    /// What [`Measure::text`] has answered, by its text, font and width:
    /// the overview and `check` lay out every state at two widths, and
    /// shape the same words again and again.
    measured: HashMap<(String, [u32; 3], Option<u32>), Size>,
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
        let fonts = FONTS
            .get_or_init(|| {
                let collection = Collection::new(CollectionOptions { shared: true, system_fonts: true });
                let mut fonts = FontContext { collection, source_cache: SourceCache::new_shared() };
                for font in ICON_FONTS {
                    fonts.collection.register_fonts(Blob::new(std::sync::Arc::new(font)), None);
                }
                Mutex::new(fonts)
            })
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Ok(Typesetter { fonts, layouts: LayoutContext::new(), locale, families, measured: HashMap::new() })
    }

    /// `text` in `font`, broken into lines no wider than `max_width`.
    pub fn set(&mut self, text: &str, font: Font, max_width: Option<f32>, align: Align) -> Layout<()> {
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, true);
        let families = if is_icons(text) { ICON_FAMILIES } else { self.families };
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(Cow::Borrowed(families))));
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

    /// Where the bytes `range` of `text` are, set in `font` from the left
    /// of a box `width` wide as a drawn text is: a box for each line they
    /// cross, from the box's top-left; for an empty range, the caret there,
    /// a box as tall as its line and no wider.
    pub fn spans(&mut self, text: &str, font: Font, width: f32, range: Range<usize>) -> Vec<Rect> {
        let layout = self.set(text, font, Some(width + HAIR), Align::Start);
        let caret = |at: usize| Cursor::from_byte_index(&layout, at, Affinity::Downstream);
        let boxes = if range.is_empty() {
            vec![caret(range.start).geometry(&layout, 0.0)]
        } else {
            let (from, to) = (caret(range.start), caret(range.end));
            Selection::new(from, to).geometry(&layout).into_iter().map(|(area, _)| area).collect()
        };
        boxes
            .into_iter()
            .map(|area| {
                let tall = (area.y1 - area.y0).max(f64::from(font.line_height));
                Rect {
                    x: area.x0 as f32,
                    y: area.y0 as f32,
                    width: (area.x1 - area.x0) as f32,
                    height: tall as f32,
                }
            })
            .collect()
    }
}

/// How much wider than measured a line is set when it is drawn, so that
/// rounding never breaks a line the measurement kept whole.
pub const HAIR: f32 = 1.0;

impl Measure for Typesetter {
    fn text(&mut self, text: &str, font: Font, max_width: Option<f32>) -> Size {
        let key = (
            text.to_string(),
            [font.size.to_bits(), u32::from(font.weight), font.line_height.to_bits()],
            max_width.map(f32::to_bits),
        );
        if let Some(size) = self.measured.get(&key) {
            return *size;
        }
        let layout = self.set(text, font, max_width, Align::Start);
        // Rounded up, so that set again in the box it was given, the text
        // breaks where it broke here.
        let size = Size { width: layout.width().ceil(), height: layout.height().ceil() };
        self.measured.insert(key, size);
        size
    }
}
