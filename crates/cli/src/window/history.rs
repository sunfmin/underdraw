//! What the window has shown, stepped back and forward through as a
//! browser does: every change of what is shown is a step, and showing
//! something new after going back drops what was forward of it. Kept only
//! while the window is open.

/// The places shown, in the order they were, and which is shown now.
#[derive(Clone, Debug)]
pub(super) struct History<T> {
    places: Vec<T>,
    /// Where in `places` the place shown is; nothing before the first.
    at: usize,
}

impl<T: Clone + PartialEq> History<T> {
    /// A window that has shown nothing yet.
    pub fn new() -> History<T> {
        History { places: Vec::new(), at: 0 }
    }

    /// `place` now shown: a step, unless it is the place shown already,
    /// which it is once [`History::back`] or [`History::forward`] went to
    /// it. Every place forward of the one shown is dropped.
    pub fn arrive(&mut self, place: T) {
        if self.places.get(self.at) == Some(&place) {
            return;
        }
        if !self.places.is_empty() {
            self.places.truncate(self.at + 1);
            self.at += 1;
        }
        self.places.push(place);
    }

    /// The place shown before the one shown now, gone back to; none at the
    /// first.
    pub fn back(&mut self) -> Option<T> {
        self.at = self.at.checked_sub(1)?;
        Some(self.places[self.at].clone())
    }

    /// The place gone back from, gone forward to again; none at the last.
    pub fn forward(&mut self) -> Option<T> {
        if !self.can_forward() {
            return None;
        }
        self.at += 1;
        Some(self.places[self.at].clone())
    }

    /// Whether there is a place to go back to.
    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    /// Whether there is a place to go forward to.
    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.places.len()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn back_and_forward_step_through_what_was_shown_and_something_new_drops_what_was_forward() {
        let mut history = History::new();
        assert!(!history.can_back() && !history.can_forward(), "nothing shown, nowhere to go");
        for place in ["a", "b", "c"] {
            history.arrive(place);
        }
        history.arrive("c");
        assert!(!history.can_forward(), "the place shown again is no step");
        assert_eq!(history.back(), Some("b"));
        history.arrive("b");
        assert_eq!(history.back(), Some("a"), "arriving where back went is no step");
        assert_eq!(history.back(), None, "nothing before the first");
        assert!(!history.can_back() && history.can_forward());
        assert_eq!(history.forward(), Some("b"));
        history.arrive("d");
        assert!(!history.can_forward(), "showing something new drops c");
        assert_eq!(history.forward(), None);
        assert_eq!(history.back(), Some("b"));
        assert_eq!(history.back(), Some("a"));
    }

    /// One thing done to a history.
    #[derive(Clone, Debug)]
    enum Done {
        Arrive(u8),
        Back,
        Forward,
    }

    proptest! {
        /// A history behaves as a stack of places behind and a stack ahead.
        #[test]
        fn a_history_is_a_place_with_a_stack_behind_and_a_stack_ahead(
            done in prop::collection::vec(prop_oneof![
                (0u8..4).prop_map(Done::Arrive),
                Just(Done::Back),
                Just(Done::Forward),
            ], 0..60)
        ) {
            let mut history = History::new();
            let (mut behind, mut shown, mut ahead): (Vec<u8>, Option<u8>, Vec<u8>) = (vec![], None, vec![]);
            for done in done {
                match done {
                    Done::Arrive(place) => {
                        history.arrive(place);
                        if shown != Some(place) {
                            behind.extend(shown);
                            shown = Some(place);
                            ahead.clear();
                        }
                    }
                    Done::Back => {
                        let went = behind.pop();
                        prop_assert_eq!(history.back(), went);
                        if let Some(went) = went {
                            ahead.extend(shown.replace(went));
                        }
                    }
                    Done::Forward => {
                        let went = ahead.pop();
                        prop_assert_eq!(history.forward(), went);
                        if let Some(went) = went {
                            behind.extend(shown.replace(went));
                        }
                    }
                }
                prop_assert_eq!(history.can_back(), !behind.is_empty());
                prop_assert_eq!(history.can_forward(), !ahead.is_empty());
            }
        }
    }
}
