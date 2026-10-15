//! Reading design tokens (tokens.json): a property such as `"space.4"` or
//! `"font.body"` names a token; a bare number is a length as written.

use serde_json::Value;

use crate::{Font, Ink};

pub(crate) struct Tokens<'a>(pub(crate) &'a Value);

/// What text is set in when nothing names a font.
pub(crate) const BODY: Font = Font { size: 16.0, weight: 400, line_height: 22.0 };

impl Tokens<'_> {
    /// The token `name` names, such as `space.4` → `tokens.space["4"]`.
    fn get(&self, name: &str) -> Option<&Value> {
        let mut at = Some(self.0);
        for segment in name.split('.') {
            at = at.and_then(|value| value.get(segment));
        }
        at
    }

    /// A length: a number as written, or the number a token names; `or`
    /// when there is neither.
    pub(crate) fn length(&self, value: Option<&Value>, or: f32) -> f32 {
        match value.and_then(|value| self.find_length(value)) {
            Some(n) => n,
            None => or,
        }
    }

    /// A length: a number as written, or the number a token names; none
    /// when `value` is neither.
    pub(crate) fn find_length(&self, value: &Value) -> Option<f32> {
        let found = match value {
            Value::String(name) => self.get(name).and_then(Value::as_f64),
            value => value.as_f64(),
        };
        found.map(|n| n as f32)
    }

    /// The font `value` names (`font.subhead`), or the body font at the
    /// size `value` gives (`24`, as tall as its line: an icon in its square);
    /// `or`, a font's token name, when it names none.
    pub(crate) fn font(&self, value: Option<&Value>, or: &str) -> Font {
        if let Some(size) = value.and_then(Value::as_f64) {
            let size = size as f32;
            return Font { size, weight: BODY.weight, line_height: size };
        }
        let name = value.and_then(Value::as_str).unwrap_or(or);
        let Some(font) = self.get(name) else {
            return BODY;
        };
        let number = |key: &str, or: f32| font.get(key).and_then(Value::as_f64).map_or(or, |n| n as f32);
        let weight = font.get("weight").and_then(Value::as_u64).map_or(400, |w| w.min(1000) as u16);
        Font { size: number("size", BODY.size), weight, line_height: number("lineHeight", BODY.line_height) }
    }

    /// The ink a colour token is drawn in. A wireframe has no colour: the
    /// brand colour reads as the strongest ink, a secondary one as muted, and
    /// any other as the text ink, or as the paper when tokens.json makes it
    /// light (a surface's white), so text in it shows on a dark fill.
    pub(crate) fn ink(&self, value: Option<&Value>) -> Ink {
        match value.and_then(Value::as_str) {
            Some("color.textSecondary") => Ink::Muted,
            Some("color.primary" | "color.primaryPressed") => Ink::Strong,
            Some("color.border") => Ink::Line,
            Some(name) if self.get(name).and_then(Value::as_str).is_some_and(light) => Ink::Paper,
            _ => Ink::Text,
        }
    }
}

/// Whether `hex` ("#FFFFFF", "#F7F7F7CC") is a colour near white: its
/// lightness, the mean of its strongest and weakest channel, is at least
/// 90%.
fn light(hex: &str) -> bool {
    let Some(rgb) = hex.strip_prefix('#').and_then(|digits| digits.get(0..6)) else {
        return false;
    };
    let Ok(rgb) = u32::from_str_radix(rgb, 16) else {
        return false;
    };
    let channels = [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF];
    let strongest = channels.iter().copied().max().unwrap_or(0);
    let weakest = channels.iter().copied().min().unwrap_or(0);
    strongest.saturating_add(weakest) >= 459
}
