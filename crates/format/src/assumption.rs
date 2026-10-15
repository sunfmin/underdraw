//! What an assumption must say for the person to answer it: what was
//! guessed, and what else it could be. The agent writes its own `assumed`;
//! the ones underdraw makes itself (a screen not drawn yet, a component it does
//! not know) are written here, as whole sentences in the language asked for.

/// The language underdraw words its own assumptions in, as the commands
/// that report them are asked (`--lang`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Zh,
}

impl Lang {
    /// Every language, by the name `--lang` takes.
    pub const NAMES: [&'static str; 2] = ["en", "zh"];

    /// Its name, as `--lang` takes it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    /// The language `name` names, as `--lang` takes it.
    pub fn parse(name: &str) -> Option<Lang> {
        match name {
            "en" => Some(Lang::En),
            "zh" => Some(Lang::Zh),
            _ => None,
        }
    }
}

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
    match lang {
        Lang::En => {
            let (names, it, is) = match screens {
                [one] => (one.clone(), "it", "is a screen"),
                _ => (screens.join(", "), "them", "are screens"),
            };
            format!(
                "This navigates to {names}, which no description under the root declares, so it is guessed that {names} {is} not drawn yet; it could instead be a misspelt name of a screen that is drawn, or the navigate may not be wanted at all: draw {it}, or correct the name."
            )
        }
        Lang::Zh => {
            let names = screens.join("、");
            format!(
                "这里跳转到 {names}，但根目录下没有任何描述声明它，所以猜测 {names} 是还没画的屏幕；也可能是已画屏幕的名字写错了，或者根本不需要这个跳转：把它画出来，或者改正名字。"
            )
        }
    }
}

/// The assumption on a node of `component`, which none of the screen's own components and none of
/// underdraw's own components define.
pub fn unknown_component(component: &str, lang: Lang) -> String {
    match lang {
        Lang::En => format!(
            "{component} is not a component underdraw knows, so it is drawn as a box with its name, guessed to be a component of this project whose look is not written yet; it could instead be a misspelt name of a component underdraw has: define it in the screen's \"components\", with its params and look, or correct the name."
        ),
        Lang::Zh => format!(
            "{component} 不是 underdraw 认识的组件，所以画成一个写着名字的框，猜测它是这个项目里还没写外观的组件；也可能是 underdraw 已有组件的名字写错了：在 screen.json 的 \"components\" 里写出它的 params 和外观，或者改正名字。"
        ),
    }
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
        for lang in Lang::NAMES.map(|name| Lang::parse(name).unwrap()) {
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
