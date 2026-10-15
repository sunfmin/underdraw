//! A text field of the gallery's sidebar, where the person writes a note, a
//! ✗ on an assumption, or the line that reopens a note: what is typed, the
//! caret and what is selected in it, and what an input method is composing
//! at the caret (pinyin becoming 你好), shown but not yet typed. It is
//! parley's `PlainEditor`, its lines broken as the sidebar breaks the field's
//! text, so that the caret goes up and down, and a click lands, on the text
//! drawn. The window turns keys into edits; the field knows nothing of keys
//! or of drawing.

use std::ops::Range;

use parley::{PlainEditor, PlainEditorDriver};

use crate::chrome::CARD_TEXT;
use crate::sidebar::FIELD_WIDTH;
use crate::text::Typesetter;

/// What the person typed, where the caret is, what is selected, and what
/// is being composed.
pub struct Field {
    editor: PlainEditor<()>,
    /// Sets its text as the sidebar sets it.
    typesetter: Typesetter,
}

/// One edit a key makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// An arrow, Home or End: the caret moved, what was selected let go.
    Move(Motion),
    /// The same with Shift: the end of what is selected moved.
    Select(Motion),
    /// Backspace: what is selected, or the character before the caret, gone.
    Back,
    /// Delete: what is selected, or the character after the caret, gone.
    Delete,
    /// ⌘A: everything selected.
    All,
}

/// Where a key moves the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// ←, →: one character back, or on.
    Left,
    Right,
    /// ↑, ↓: a line up, or down, as the field breaks its lines.
    Up,
    Down,
    /// Home, End: the start, or the end, of the line.
    Home,
    End,
}

/// The field as it is drawn: the text with what is being composed put in
/// at the caret, and, as bytes of that, where the caret is, which of it is
/// composed, and which is selected.
#[derive(Debug, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub caret: usize,
    pub composing: Range<usize>,
    pub selected: Range<usize>,
}

impl Field {
    /// An empty field, its text set by `typesetter`'s fonts.
    pub fn new(typesetter: &Typesetter) -> Field {
        let typesetter = typesetter.like();
        let editor = typesetter.editor(CARD_TEXT, FIELD_WIDTH as f32);
        let mut field = Field { editor, typesetter };
        field.drive(|_| {});
        field
    }

    /// What was typed: what saving it saves. What is being composed is not.
    pub fn text(&self) -> String {
        self.editor.text().to_string()
    }

    /// Whether nothing but spaces was typed.
    pub fn is_blank(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// Whether an input method is composing: its keys are its own then.
    pub fn is_composing(&self) -> bool {
        self.editor.is_composing()
    }

    /// What is selected, when something is.
    pub fn selected(&self) -> Option<&str> {
        self.editor.selected_text()
    }

    /// `typed` put in at the caret, in place of what is selected, the caret
    /// after it: a key's text, a paste, or what an input method committed,
    /// which ends what it composed.
    pub fn type_in(&mut self, typed: &str) {
        self.drive(|driver| {
            driver.clear_compose();
            driver.insert_or_replace_selection(typed);
        });
    }

    /// What an input method is composing now, in place of what it composed
    /// before, at the caret, in place of what is selected; empty once it has
    /// stopped. `cursor` is where its own caret is in it, in bytes.
    pub fn compose(&mut self, composing: &str, cursor: Option<(usize, usize)>) {
        self.drive(|driver| {
            if composing.is_empty() {
                driver.clear_compose();
            } else {
                driver.set_compose(composing, cursor);
            }
        });
    }

    /// `edit` made. Whether it changed anything.
    pub fn edit(&mut self, edit: Edit) -> bool {
        let before = self.editor.generation();
        let does: fn(&mut PlainEditorDriver<()>) = match edit {
            Edit::Move(Motion::Left) => |d| d.move_left(),
            Edit::Move(Motion::Right) => |d| d.move_right(),
            Edit::Move(Motion::Up) => |d| d.move_up(),
            Edit::Move(Motion::Down) => |d| d.move_down(),
            Edit::Move(Motion::Home) => |d| d.move_to_line_start(),
            Edit::Move(Motion::End) => |d| d.move_to_line_end(),
            Edit::Select(Motion::Left) => |d| d.select_left(),
            Edit::Select(Motion::Right) => |d| d.select_right(),
            Edit::Select(Motion::Up) => |d| d.select_up(),
            Edit::Select(Motion::Down) => |d| d.select_down(),
            Edit::Select(Motion::Home) => |d| d.select_to_line_start(),
            Edit::Select(Motion::End) => |d| d.select_to_line_end(),
            Edit::Back => |d| d.backdelete(),
            Edit::Delete => |d| d.delete(),
            Edit::All => |d| d.select_all(),
        };
        self.drive(does);
        self.editor.generation() != before
    }

    /// What is selected taken out, and given back; none when nothing is.
    pub fn cut(&mut self) -> Option<String> {
        let cut = self.selected()?.to_string();
        self.drive(|driver| driver.delete_selection());
        Some(cut)
    }

    /// The caret put at `at`, a point from the top-left of the field's
    /// text; or, when `extend`, what is selected drawn from where it starts
    /// to there.
    pub fn point(&mut self, at: (f64, f64), extend: bool) {
        let (x, y) = (at.0 as f32, at.1 as f32);
        self.drive(|driver| {
            if extend {
                driver.extend_selection_to_point(x, y);
            } else {
                driver.move_to_point(x, y);
            }
        });
    }

    /// The field as it is drawn.
    pub fn shown(&self) -> Shown {
        let selection = self.editor.raw_selection();
        let caret = selection.focus().index();
        let composing = self.editor.raw_compose().clone().unwrap_or(caret..caret);
        // An input method that hides its caret is composing at the end of
        // what it composes.
        let caret = if self.editor.is_cursor_visible() { caret } else { composing.end };
        Shown {
            text: self.editor.raw_text().to_string(),
            caret,
            composing,
            selected: if self.is_composing() { caret..caret } else { selection.text_range() },
        }
    }

    /// `does` done to the editor, and its lines broken again for what it
    /// came to.
    fn drive(&mut self, does: impl FnOnce(&mut PlainEditorDriver<()>)) {
        let mut driver = self.typesetter.drive(&mut self.editor);
        does(&mut driver);
        driver.refresh_layout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Edit = Edit::Move(Motion::Left);
    const RIGHT: Edit = Edit::Move(Motion::Right);

    fn field() -> Field {
        Field::new(&Typesetter::new(None).unwrap())
    }

    #[test]
    fn the_caret_moves_by_characters_and_edits_where_it_is() {
        let mut field = field();
        field.type_in("价格太红");
        assert!(field.edit(LEFT) && field.edit(LEFT));
        field.type_in("很");
        assert_eq!(field.text(), "价格很太红");
        assert!(field.edit(Edit::Delete), "Delete takes the character after the caret");
        assert_eq!(field.text(), "价格很红");
        assert!(field.edit(Edit::Back));
        assert_eq!(field.text(), "价格红");
        assert!(field.edit(LEFT) && field.edit(LEFT) && !field.edit(LEFT));
        assert!(!field.edit(Edit::Back), "nothing before the start");
        while field.edit(RIGHT) {}
        assert!(!field.edit(Edit::Delete), "nothing after the end");
        assert_eq!(field.shown().caret, "价格红".len());
    }

    #[test]
    fn home_end_and_the_lines_of_the_field() {
        let mut field = field();
        field.type_in("first\nsecond");
        assert!(field.edit(Edit::Move(Motion::Home)));
        field.type_in(">");
        assert!(field.edit(Edit::Move(Motion::Up)), "up to the line before");
        assert!(field.edit(Edit::Move(Motion::Home)));
        field.type_in("^");
        assert!(field.edit(Edit::Move(Motion::End)));
        field.type_in("!");
        assert!(field.edit(Edit::Move(Motion::Down)));
        assert!(field.edit(Edit::Move(Motion::End)));
        field.type_in("$");
        assert_eq!(field.text(), "^first!\n>second$");
        let mut long = self::field();
        long.type_in(&"word ".repeat(40));
        assert!(long.edit(Edit::Move(Motion::Home)), "the start of the last line the field breaks");
        assert!(long.shown().caret > 0, "not the start of the text");
        assert!(long.edit(Edit::Move(Motion::Up)), "a line the field breaks is a line to go up");
    }

    #[test]
    fn what_is_selected_is_replaced_by_what_is_typed_and_cut() {
        let mut field = field();
        field.type_in("too red");
        for _ in 0..3 {
            assert!(field.edit(Edit::Select(Motion::Left)));
        }
        assert_eq!(field.selected(), Some("red"));
        assert_eq!(field.shown().selected, 4..7, "drawn shaded");
        field.type_in("blue");
        assert_eq!(field.text(), "too blue");
        assert!(field.selected().is_none());
        assert!(field.edit(Edit::Select(Motion::Home)));
        assert_eq!(field.cut().as_deref(), Some("too blue"));
        assert!(field.is_blank());
        assert!(field.cut().is_none(), "nothing to cut");
    }

    #[test]
    fn a_point_puts_the_caret_and_a_drag_selects() {
        let mut field = field();
        field.type_in("one two three");
        field.point((0.0, 5.0), false);
        assert_eq!(field.shown().caret, 0, "the left of the text is its start");
        field.point((1000.0, 5.0), true);
        assert_eq!(field.selected(), Some("one two three"));
        field.point((1000.0, 5.0), false);
        assert!(field.selected().is_none());
        assert_eq!(field.shown().caret, "one two three".len());
    }

    #[test]
    fn an_input_method_shows_what_it_composes_and_types_what_it_commits() {
        let mut field = field();
        field.type_in("ab");
        field.edit(LEFT);
        field.compose("ni", Some((2, 2)));
        field.compose("nihao", Some((5, 5)));
        assert!(field.is_composing());
        let shown = field.shown();
        assert_eq!(shown.text, "anihaob");
        assert_eq!(&shown.text[shown.composing.clone()], "nihao", "drawn underlined");
        assert_eq!(shown.caret, "anihao".len(), "the caret where the input method puts it");
        assert_eq!(field.text(), "ab", "composing is not typed");
        field.type_in("你好");
        assert!(!field.is_composing());
        assert_eq!(field.text(), "a你好b");
        assert_eq!(field.shown().caret, "a你好".len());
        field.compose("x", None);
        field.compose("", None);
        assert_eq!(field.text(), "a你好b", "composing dropped types nothing");
    }

    #[test]
    fn select_all_is_replaced_by_what_is_typed_or_cleared() {
        let mut field = field();
        field.type_in("old words");
        assert!(field.edit(Edit::All) && field.shown().selected == (0.."old words".len()));
        assert!(!field.edit(Edit::All), "all of it is selected already");
        field.type_in("new");
        assert_eq!(field.text(), "new");
        field.edit(Edit::All);
        assert!(field.edit(Edit::Back));
        assert!(field.is_blank());
        assert!(!field.edit(Edit::All), "nothing to select");
        field.type_in("abc");
        field.edit(Edit::All);
        field.edit(LEFT);
        field.type_in(">");
        assert_eq!(field.text(), ">abc", "← drops the selection to its start");
    }
}
