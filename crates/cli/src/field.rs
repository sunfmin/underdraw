//! A text field of the gallery's sidebar, where the person writes a note, a
//! ✗ on an assumption, or the line that reopens a note: what is typed, the
//! caret in it, and what an input method is composing at the caret (pinyin
//! becoming 你好), shown but not yet typed. The window turns keys into
//! edits; the field knows nothing of keys or of drawing.

use std::ops::Range;

/// What the person typed, where the caret is, and what is being composed.
#[derive(Debug, Default)]
pub struct Field {
    text: String,
    /// Where the caret is: a byte of `text`, always between two characters.
    caret: usize,
    /// Whether ⌘A selected everything: what is typed next replaces it.
    all: bool,
    /// What an input method is composing at the caret, not yet typed.
    composing: String,
}

/// One edit a key makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// ←: the caret one character back.
    Left,
    /// →: the caret one character on.
    Right,
    /// Backspace: the character before the caret gone.
    Back,
    /// Delete: the character after the caret gone.
    Delete,
    /// ⌘A: everything selected.
    All,
}

/// The field as it is drawn: the text with what is being composed put in
/// at the caret, where the caret is in that, and which of it is composed.
#[derive(Debug, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub caret: usize,
    pub composing: Range<usize>,
    /// Whether everything is selected.
    pub all: bool,
}

impl Field {
    /// What was typed: what saving it saves. What is being composed is not.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether nothing but spaces was typed.
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Whether an input method is composing: its keys are its own then.
    pub fn is_composing(&self) -> bool {
        !self.composing.is_empty()
    }

    /// `typed` put in at the caret, in place of everything when it is
    /// selected, the caret after it: a key's text, a paste, or what an input
    /// method committed, which ends what it composed.
    pub fn type_in(&mut self, typed: &str) {
        self.composing.clear();
        self.clear_selected();
        self.text.insert_str(self.caret, typed);
        self.caret += typed.len();
    }

    /// What an input method is composing now, in place of what it composed
    /// before; empty once it has stopped.
    pub fn compose(&mut self, composing: &str) {
        if !composing.is_empty() {
            self.clear_selected();
        }
        self.composing = composing.to_string();
    }

    /// `edit` made. Whether it changed anything.
    pub fn edit(&mut self, edit: Edit) -> bool {
        if self.all && edit != Edit::All {
            self.all = false;
            match edit {
                Edit::Left => self.caret = 0,
                Edit::Right => self.caret = self.text.len(),
                _ => self.clear_selected_now(),
            }
            return true;
        }
        let before = self.text[..self.caret].chars().next_back().map_or(0, char::len_utf8);
        let after = self.text[self.caret..].chars().next().map_or(0, char::len_utf8);
        match edit {
            Edit::Left if before > 0 => self.caret -= before,
            Edit::Right if after > 0 => self.caret += after,
            Edit::Back if before > 0 => {
                self.caret -= before;
                self.text.replace_range(self.caret..self.caret + before, "");
            }
            Edit::Delete if after > 0 => {
                self.text.replace_range(self.caret..self.caret + after, "");
            }
            Edit::All if !self.text.is_empty() && !self.all => self.all = true,
            _ => return false,
        }
        true
    }

    /// The field as it is drawn.
    pub fn shown(&self) -> Shown {
        let mut text = self.text.clone();
        text.insert_str(self.caret, &self.composing);
        let composing = self.caret..self.caret + self.composing.len();
        Shown { text, caret: composing.end, composing, all: self.all }
    }

    /// Everything selected gone, before what replaces it.
    fn clear_selected(&mut self) {
        if self.all {
            self.all = false;
            self.clear_selected_now();
        }
    }

    fn clear_selected_now(&mut self) {
        self.text.clear();
        self.caret = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_moves_by_characters_and_edits_where_it_is() {
        let mut field = Field::default();
        field.type_in("价格太红");
        assert!(field.edit(Edit::Left) && field.edit(Edit::Left));
        field.type_in("很");
        assert_eq!(field.text(), "价格很太红");
        assert!(field.edit(Edit::Delete), "Delete takes the character after the caret");
        assert_eq!(field.text(), "价格很红");
        assert!(field.edit(Edit::Back));
        assert_eq!(field.text(), "价格红");
        assert!(field.edit(Edit::Left) && field.edit(Edit::Left) && !field.edit(Edit::Left));
        assert!(!field.edit(Edit::Back), "nothing before the start");
        while field.edit(Edit::Right) {}
        assert!(!field.edit(Edit::Delete), "nothing after the end");
        assert_eq!(field.shown().caret, "价格红".len());
    }

    #[test]
    fn an_input_method_shows_what_it_composes_and_types_what_it_commits() {
        let mut field = Field::default();
        field.type_in("ab");
        field.edit(Edit::Left);
        field.compose("ni");
        field.compose("nihao");
        assert!(field.is_composing());
        let shown = field.shown();
        assert_eq!(shown.text, "anihaob");
        assert_eq!(&shown.text[shown.composing.clone()], "nihao", "drawn underlined");
        assert_eq!(field.text(), "ab", "composing is not typed");
        field.type_in("你好");
        assert!(!field.is_composing());
        assert_eq!(field.text(), "a你好b");
        assert_eq!(field.shown().caret, "a你好".len());
        field.compose("x");
        field.compose("");
        assert_eq!(field.text(), "a你好b", "composing dropped types nothing");
    }

    #[test]
    fn select_all_is_replaced_by_what_is_typed_or_cleared() {
        let mut field = Field::default();
        field.type_in("old words");
        assert!(field.edit(Edit::All) && field.shown().all);
        field.type_in("new");
        assert_eq!(field.text(), "new");
        field.edit(Edit::All);
        assert!(field.edit(Edit::Back));
        assert!(field.is_blank());
        field.type_in("abc");
        field.edit(Edit::All);
        field.edit(Edit::Left);
        field.type_in(">");
        assert_eq!(field.text(), ">abc", "← drops the selection to its start");
    }
}
