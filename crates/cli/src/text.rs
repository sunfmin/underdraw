//! Text, set with parley in the system's fonts: measured for layout, and
//! the same layout drawn into the picture, so what was measured is what is
//! drawn.

use std::borrow::Cow;
use std::collections::HashMap;

use std::sync::{Mutex, OnceLock};

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Language, Layout, LayoutContext,
    LineHeight, PlainEditor, PlainEditorDriver, StyleProperty,
};
use underdraw_layout::{Align, Font, Measure, Size};

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
        let styles = self.styles(font, is_icons(text));
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, true);
        for style in styles {
            builder.push_default(style);
        }
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

impl Typesetter {
    /// How text in `font` is set, in the icon fonts when it is `icons`:
    /// the same for what is measured and drawn as for what is edited.
    fn styles(&self, font: Font, icons: bool) -> [StyleProperty<'static, ()>; 5] {
        let families = if icons { ICON_FAMILIES } else { self.families };
        [
            StyleProperty::FontFamily(FontFamily::Source(Cow::Borrowed(families))),
            StyleProperty::FontSize(font.size),
            StyleProperty::Locale(self.locale),
            StyleProperty::FontWeight(FontWeight::new(f32::from(font.weight))),
            StyleProperty::LineHeight(LineHeight::Absolute(font.line_height)),
        ]
    }

    /// An editor of text in `font`, broken into lines no wider than
    /// `max_width`: set as [`Typesetter::set`] sets the same text, so that
    /// its caret and selection lie on the text drawn.
    pub fn editor(&self, font: Font, max_width: f32) -> PlainEditor<()> {
        let mut editor = PlainEditor::new(font.size);
        for style in self.styles(font, false) {
            editor.edit_styles().insert(style);
        }
        editor.set_width(Some(max_width));
        editor
    }

    /// `editor` driven with this typesetter's fonts.
    pub fn drive<'a>(&'a mut self, editor: &'a mut PlainEditor<()>) -> PlainEditorDriver<'a, ()> {
        editor.driver(&mut self.fonts, &mut self.layouts)
    }
}

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
