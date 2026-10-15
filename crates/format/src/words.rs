//! Everything underdraw and its gallery say themselves, rather than repeat
//! from a description: the window's own words (its sidebar, the tree's
//! marks, the flow's and the overview's marks, its title), the assumptions
//! underdraw makes itself, and how a reference is said. One table per
//! language, each sentence in it named once ([`Words`]) and said by its
//! name ([`say!`](crate::say)); adding a language is adding its table to
//! [`LANGUAGES`].
//!
//! A sentence holds `{name}` placeholders, the same ones in every language,
//! which [`fill`] fills.

mod en;
mod ja;
mod zh;

/// Every language `--lang` takes, the first the default.
const LANGUAGES: [&Words; 3] = [&en::WORDS, &zh::WORDS, &ja::WORDS];

/// The language underdraw says its own words in, as the commands that
/// report them are asked (`--lang`).
#[derive(Clone, Copy)]
pub struct Lang(&'static Words);

#[allow(non_upper_case_globals)]
impl Lang {
    pub const En: Lang = Lang(&en::WORDS);
    pub const Zh: Lang = Lang(&zh::WORDS);
    pub const Ja: Lang = Lang(&ja::WORDS);

    /// Every language, in the order `--lang` lists them.
    pub fn all() -> impl Iterator<Item = Lang> {
        LANGUAGES.into_iter().map(Lang)
    }

    /// Its name, as `--lang` takes it.
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// The language `name` names, as `--lang` takes it.
    pub fn parse(name: &str) -> Option<Lang> {
        Lang::all().find(|lang| lang.name() == name)
    }

    /// The BCP 47 tag its words are set in, which picks the Han glyphs
    /// (Japanese or Simplified) and the fonts that hold them; none leaves
    /// the choice to the system.
    pub fn locale(self) -> Option<&'static str> {
        self.0.locale
    }

    /// Its sentences.
    pub fn words(self) -> &'static Words {
        self.0
    }
}

impl Default for Lang {
    fn default() -> Lang {
        Lang(LANGUAGES[0])
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Lang) -> bool {
        self.name() == other.name()
    }
}

impl Eq for Lang {}

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Declares [`Words`] with one field per sentence, and [`Words::each`]
/// over the same fields, so that the list of sentences is written once.
macro_rules! sentences {
    ($($sentence:ident,)*) => {
        /// One language's sentences, each by its name. Its English table
        /// (`words/en.rs`) says what each one says.
        pub struct Words {
            /// The language's name, as `--lang` takes it.
            pub name: &'static str,
            /// The BCP 47 tag its sentences are set in, if any: see [`Lang::locale`].
            pub locale: Option<&'static str>,
            $(pub $sentence: &'static str,)*
        }

        impl Words {
            /// Every sentence, by its name.
            pub fn each(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((stringify!($sentence), self.$sentence)),*]
            }
        }
    };
}

sentences! {
    // What underdraw assumes itself (`check`, `render`, and the gallery's marks).
    undrawn_one,
    undrawn_many,
    list_separator,
    unknown_component,
    // What a note is on, as the sidebar says it.
    on_node,
    on_state,
    on_transition,
    transition_outcome,
    on_navigate,
    // The rest of the gallery's own words.
    acted_to_state,
    acted_calls,
    acted_navigates,
    acted_goes_back,
    acted_goes_back_gone,
    acted_goes_back_nowhere,
    no_transition,
    propose_navigate,
    undone,
    acted_outcome,
    proposal_needs_node,
    proposal_saved,
    not_saved,
    gone_from_tree,
    no_such_state,
    journey_title,
    journey_label,
    journey_answered,
    journey_acts_on,
    journey_ends,
    previous_step,
    next_step,
    journey_mark,
    nothing_to_copy,
    no_clipboard,
    copied,
    could_not_copy,
    hint_click,
    hint_act,
    hint_keys,
    whole_state,
    talk_title,
    talk_none,
    talk_hint,
    talk_box,
    send,
    nothing_picked,
    guess_on,
    agent_reply,
    nothing_waits,
    waiting_on_agent,
    waits_on_you,
    or_else,
    or_else_not_said,
    answers_it,
    keep_it,
    wrong_it,
    judge_hint,
    assumed_title,
    nothing_assumed,
    judged_right_saved,
    confirming_in,
    undo,
    confirming_title,
    ask_right,
    not_sent_empty,
    enter_sends,
    done_rejected,
    note_sent,
    calls_nothing,
    calls,
    id,
    at_width,
    node_box,
    not_in_state,
    note_answered,
    faults_not_played_one,
    faults_not_played_many,
    faults_not_drawn_one,
    faults_not_drawn_many,
    notes_one,
    notes_many,
    faults_one,
    faults_many,
    faulty_one,
    faulty_many,
    status_waiting,
    status_answered,
    window_title,
    details,
    node_label,
    node_no_label,
    answer_right,
    answer_wrong,
}

/// `sentence` with each `{name}` in it replaced by the value given for
/// `name`. Every placeholder must be given, and nothing else: a sentence is
/// the same in every language, so a call that misses one misses it in all.
pub fn fill(sentence: &str, values: &[(&str, String)]) -> String {
    debug_assert!(
        {
            let mut wanted = placeholders(sentence);
            let mut given: Vec<&str> = values.iter().map(|(name, _)| *name).collect();
            wanted.sort_unstable();
            wanted.dedup();
            given.sort_unstable();
            wanted == given
        },
        "{sentence:?} is not given exactly its placeholders"
    );
    let mut said = String::new();
    let mut rest = sentence;
    while let Some((before, after)) = rest.split_once('{') {
        said.push_str(before);
        let filled = after
            .split_once('}')
            .and_then(|(name, tail)| Some((values.iter().find(|(given, _)| *given == name)?, tail)));
        match filled {
            Some(((_, value), tail)) => {
                said.push_str(value);
                rest = tail;
            }
            None => {
                said.push('{');
                rest = after;
            }
        }
    }
    said.push_str(rest);
    said
}

/// The names of the placeholders in `sentence`, in order: each `{name}`
/// whose name is letters, digits and `_`.
pub fn placeholders(sentence: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = sentence;
    while let Some((_, after)) = rest.split_once('{') {
        rest = after;
        if let Some((name, tail)) = after.split_once('}')
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            found.push(name);
            rest = tail;
        }
    }
    found
}

/// The sentence `$sentence` in `$lang`, its placeholders filled: each by
/// the variable of its name, or by the value written after `=`.
///
/// `say!(lang, acted_navigates, on, screen = name)`
#[macro_export]
macro_rules! say {
    (@value $name:ident) => {
        $name
    };
    (@value $name:ident = $value:expr) => {
        $value
    };
    ($lang:expr, $sentence:ident $(, $name:ident $(= $value:expr)?)* $(,)?) => {
        $crate::words::fill(
            $lang.words().$sentence,
            &[$((stringify!($name), $crate::say!(@value $name $(= $value)?).to_string())),*],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_says_every_sentence_with_the_same_placeholders() {
        let first = Lang::default().words();
        for lang in Lang::all() {
            for ((name, said), (_, model)) in lang.words().each().into_iter().zip(first.each()) {
                assert!(!said.is_empty(), "{} does not say {name}", lang.name());
                let mut placeholders = super::placeholders(said);
                let mut wanted = super::placeholders(model);
                placeholders.sort_unstable();
                placeholders.dedup();
                wanted.sort_unstable();
                wanted.dedup();
                assert_eq!(
                    placeholders,
                    wanted,
                    "{name} in {} holds other placeholders than in {}",
                    lang.name(),
                    Lang::default().name()
                );
            }
        }
    }

    #[test]
    fn every_language_is_parsed_by_its_name() {
        let names: Vec<&str> = Lang::all().map(Lang::name).collect();
        assert_eq!(names, ["en", "zh", "ja"]);
        for lang in Lang::all() {
            assert_eq!(Lang::parse(lang.name()), Some(lang));
        }
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn fills_each_placeholder_by_its_name() {
        let said =
            fill("{a} and {b}, then {a}; {not one} {", &[("a", "1".to_string()), ("b", "2".to_string())]);
        assert_eq!(said, "1 and 2, then 1; {not one} {");
        assert_eq!(placeholders("{a} {b c} {} {d_1}"), ["a", "d_1"]);
    }
}
