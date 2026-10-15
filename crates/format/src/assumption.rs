//! What an assumption must say for the person to answer it: what was
//! guessed, and what else it could be. The agent writes its own `assumed`;
//! the ones underdraw makes itself (a screen not drawn yet, a component it does
//! not know) are said here, in the language asked for ([`crate::words`]).

use crate::words::Lang;

/// The fewest words an `assumed` says what was guessed and what else it
/// could be in, where words are spaced.
const WORDS: usize = 5;
/// The fewest characters it says that in, in a script written unspaced
/// (Chinese, Japanese).
const UNSPACED: usize = 10;

/// What `check` says of an `assumed` too short to say what was guessed and
/// what else it could be (empty, or a word or two); none when it is long
/// enough.
pub fn too_short(assumed: &str) -> Option<&'static str> {
    let spaced = assumed.split_whitespace().count() >= WORDS;
    let unspaced = assumed.chars().filter(|c| is_unspaced(*c)).count() >= UNSPACED;
    if spaced || unspaced {
        return None;
    }
    Some(
        "is too short for the person to answer: say in a sentence what was guessed and what else it could be, \"<the guess>; it could instead be <the other>\", in the language you speak with the person (`underdraw check --help`, \"assumed\")",
    )
}

/// A character of a script written without spaces between words.
fn is_unspaced(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

/// The assumption on a node that navigates to `screens`, none of which a
/// description under the root declares.
pub fn undrawn(screens: &[String], lang: Lang) -> String {
    let names = screens.join(lang.words().list_separator);
    match screens {
        [_] => crate::say!(lang, undrawn_one, names),
        _ => crate::say!(lang, undrawn_many, names),
    }
}

/// The assumption on a node of `component`, which none of the screen's own components and none of
/// underdraw's own components define.
pub fn unknown_component(component: &str, lang: Lang) -> String {
    crate::say!(lang, unknown_component, component)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_assumed_of_a_word_or_two_is_too_short() {
        for short in ["", "  ", "guess", "card fields", "待定", "可能是卡片"] {
            assert!(too_short(short).is_some(), "{short:?} is too short");
        }
    }

    #[test]
    fn an_assumed_that_says_the_guess_and_the_other_is_long_enough() {
        for long in [
            "Card fields are drawn here; the real build may use the payment provider's hosted fields",
            "这里画了卡号输入框，实际也可能用支付服务商托管的输入框",
        ] {
            assert_eq!(too_short(long), None, "{long:?} is long enough");
        }
    }

    #[test]
    fn what_underdraw_assumes_itself_is_long_enough_in_every_language() {
        for lang in Lang::all() {
            for said in [undrawn(&["OrderDetail".to_string()], lang), unknown_component("Gauge", lang)] {
                assert_eq!(too_short(&said), None, "{said}");
            }
        }
    }

    #[test]
    fn names_the_screen_or_component() {
        let two = ["OrderDetail".to_string(), "Export".to_string()];
        assert!(undrawn(&two, Lang::En).contains("OrderDetail, Export are screens not drawn yet"));
        assert!(undrawn(&two, Lang::Zh).contains("OrderDetail、Export"));
        assert!(unknown_component("Gauge", Lang::Zh).starts_with("Gauge 不是"));
    }
}
