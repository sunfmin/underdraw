//! The text of a note being written: parley's `PlainEditor`, holding what
//! has been typed, its caret and selection, and what an input method is
//! composing (its preedit), set as the sidebar sets a card's text, so that
//! the caret and selection it lays out lie on the text the sidebar draws.
//! It knows the keys that move, select and delete in text; what Enter and
//! Esc do with a note is the window's.

use parley::{Affinity, BoundingBox, Cursor, PlainEditor, PlainEditorDriver, Selection};
use underdraw_layout::Rect;
use winit::keyboard::{Key, NamedKey};

use crate::chrome::CARD_TEXT;
use crate::sidebar::{self, TEXT_WIDTH};
use crate::text::Typesetter;

/// How wide the caret is drawn.
const CARET: f32 = 1.5;

pub struct Editor {
    editor: PlainEditor<()>,
}

impl Editor {
    /// An empty editor, its caret at the start.
    pub fn new(typesetter: &mut Typesetter) -> Editor {
        let mut editor = Editor { editor: typesetter.editor(CARD_TEXT, TEXT_WIDTH as f32) };
        editor.drive(typesetter, |_| {});
        editor
    }

    /// What has been written: never what an input method is still
    /// composing.
    pub fn text(&self) -> String {
        self.editor.text().to_string()
    }

    /// The text selected, when some is.
    pub fn selected(&self) -> Option<&str> {
        self.editor.selected_text()
    }

    /// `text` typed or pasted at the caret, in place of what is selected.
    pub fn insert(&mut self, typesetter: &mut Typesetter, text: &str) {
        self.drive(typesetter, |driver| driver.insert_or_replace_selection(text));
    }

    /// A key pressed, Shift held or not, with the `text` it types: the
    /// arrows, Home and End move the caret, or, with Shift, the selection's
    /// end; Backspace and Delete delete; any other key types its text.
    /// Whether it did any of these.
    pub fn key(&mut self, typesetter: &mut Typesetter, key: &Key, text: Option<&str>, shift: bool) -> bool {
        let Key::Named(named) = key else {
            return match text {
                Some(text) if !text.chars().any(char::is_control) => {
                    self.insert(typesetter, text);
                    true
                }
                _ => false,
            };
        };
        let does: fn(&mut PlainEditorDriver<()>) = match (named, shift) {
            (NamedKey::ArrowLeft, false) => |d| d.move_left(),
            (NamedKey::ArrowLeft, true) => |d| d.select_left(),
            (NamedKey::ArrowRight, false) => |d| d.move_right(),
            (NamedKey::ArrowRight, true) => |d| d.select_right(),
            (NamedKey::ArrowUp, false) => |d| d.move_up(),
            (NamedKey::ArrowUp, true) => |d| d.select_up(),
            (NamedKey::ArrowDown, false) => |d| d.move_down(),
            (NamedKey::ArrowDown, true) => |d| d.select_down(),
            (NamedKey::Home, false) => |d| d.move_to_line_start(),
            (NamedKey::Home, true) => |d| d.select_to_line_start(),
            (NamedKey::End, false) => |d| d.move_to_line_end(),
            (NamedKey::End, true) => |d| d.select_to_line_end(),
            (NamedKey::Backspace, _) => |d| d.backdelete(),
            (NamedKey::Delete, _) => |d| d.delete(),
            (NamedKey::Space, _) => |d| d.insert_or_replace_selection(" "),
            _ => return false,
        };
        self.drive(typesetter, does);
        true
    }

    /// Everything written selected.
    pub fn select_all(&mut self, typesetter: &mut Typesetter) {
        self.drive(typesetter, |driver| driver.select_all());
    }

    /// The text selected taken out, and given back; none when none is.
    pub fn cut(&mut self, typesetter: &mut Typesetter) -> Option<String> {
        let cut = self.selected()?.to_string();
        self.drive(typesetter, |driver| driver.delete_selection());
        Some(cut)
    }

    /// What an input method is composing, shown at the caret, in place of
    /// what it composed before: `cursor` is where in it its own caret is,
    /// in bytes. Empty, it composes nothing any more.
    pub fn compose(&mut self, typesetter: &mut Typesetter, text: &str, cursor: Option<(usize, usize)>) {
        self.drive(typesetter, |driver| {
            if text.is_empty() {
                driver.clear_compose();
            } else {
                driver.set_compose(text, cursor);
            }
        });
    }

    /// What an input method composed written, in place of its preedit.
    pub fn commit(&mut self, typesetter: &mut Typesetter, text: &str) {
        self.drive(typesetter, |driver| {
            driver.clear_compose();
            driver.insert_or_replace_selection(text);
        });
    }

    /// The caret put at `at`, a point from the text's top-left; or, when
    /// `extend`, the selection drawn from where it was to there.
    pub fn point(&mut self, typesetter: &mut Typesetter, at: (f64, f64), extend: bool) {
        let (x, y) = (at.0 as f32, at.1 as f32);
        self.drive(typesetter, |driver| {
            if extend {
                driver.extend_selection_to_point(x, y);
            } else {
                driver.move_to_point(x, y);
            }
        });
    }

    /// The text as the sidebar draws it: what an input method is composing
    /// included, and underlined.
    pub fn field(&self) -> sidebar::Field {
        let Some(layout) = self.editor.try_layout() else {
            unreachable!("the editor's layout is set again after every edit");
        };
        let preedit = match self.editor.raw_compose() {
            Some(range) => {
                let start = Cursor::from_byte_index(layout, range.start, Affinity::Downstream);
                let end = Cursor::from_byte_index(layout, range.end, Affinity::Upstream);
                Selection::new(start, end).geometry(layout).into_iter().map(|(r, _)| rect(r)).collect()
            }
            None => Vec::new(),
        };
        sidebar::Field {
            text: self.editor.raw_text().to_string(),
            selection: self.editor.selection_geometry().into_iter().map(|(r, _)| rect(r)).collect(),
            caret: self.editor.cursor_geometry(CARET).map(rect),
            preedit,
            height: f64::from(layout.height()),
        }
    }

    /// Where an input method's candidates should keep clear of, from the
    /// text's top-left: the preedit, or the caret.
    pub fn ime_area(&self) -> Rect {
        rect(self.editor.ime_cursor_area())
    }

    /// `does` done to the editor, and its layout set again for what it
    /// came to, so that it can be drawn without the typesetter.
    fn drive(&mut self, typesetter: &mut Typesetter, does: impl FnOnce(&mut PlainEditorDriver<()>)) {
        let mut driver = typesetter.drive(&mut self.editor);
        does(&mut driver);
        driver.refresh_layout();
    }
}

fn rect(r: BoundingBox) -> Rect {
    Rect { x: r.x0 as f32, y: r.y0 as f32, width: (r.x1 - r.x0) as f32, height: (r.y1 - r.y0) as f32 }
}
