//! Stored text drawn so a terminal shows it rather than obeys it (da-0z8cb).
//!
//! A value reaches every reader's terminal, and it may come from another
//! member of a shared database or a file handed over for import, which no
//! write-time check sees. So at render, a control character (Unicode Cc: C0,
//! DEL, C1) other than a newline or a tab, and a bidi control, is drawn as
//! its escape, `\u{1b}`: an ESC there could clear or retitle the screen or
//! write the clipboard, and a bidi control lets one value display as another.
//! The value itself stays as stored, and JSON output carries it as JSON does.

use std::borrow::Cow;

/// A bidi control: the marks and the embeddings, overrides and isolates.
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn hidden(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t') || is_bidi_control(c)
}

/// `s` with each control and bidi character but `\n` and `\t` drawn as its
/// escape, borrowed when there is none.
pub fn visible(s: &str) -> Cow<'_, str> {
    if !s.chars().any(hidden) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match hidden(c) {
            true => out.extend(c.escape_unicode()),
            false => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_controls_and_bidi_and_keeps_newline_and_tab() {
        let s = "a\u{1b}]0;x\u{7}b\u{202E}c\u{7f}\u{85}\td\ne";
        assert_eq!(
            visible(s),
            "a\\u{1b}]0;x\\u{7}b\\u{202e}c\\u{7f}\\u{85}\td\ne"
        );
        assert!(matches!(visible("plain ünïcode\t"), Cow::Borrowed(_)));
    }
}
