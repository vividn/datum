//! A field TEMPLATE (da-38obu): a field name holding `%key%` placeholders,
//! as `add 'issue__%hid:8%'` writes one and `setup` declares a spec on one.
//! The spec covers every field the template writes, and every field whose
//! name parses against it ([`matches`]).
//!
//! `\%` is a literal percent, as in `datum_core`'s interpolation, and so is
//! a `%` left unpaired.

use crate::entry_id::HID_LEN;

/// The characters `%uid%` keeps of a hid: `datum_core::entry::SHORT_HID_LEN`.
const UID_LEN: usize = 5;

/// `name`'s literal runs and placeholder keys, alternating, a literal first
/// and last: `issue__%hid:8%` is `["issue__", "hid:8", ""]`. One part when
/// it holds no placeholder.
pub fn parts(name: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'%') => {
                chars.next();
                parts.last_mut().expect("never empty").push('%');
            }
            '%' => parts.push(String::new()),
            c => parts.last_mut().expect("never empty").push(c),
        }
    }
    // An unpaired `%` opens no placeholder: what follows it is literal.
    if parts.len() % 2 == 0 {
        let tail = parts.pop().expect("two or more");
        let last = parts.last_mut().expect("one or more");
        last.push('%');
        last.push_str(&tail);
    }
    parts
}

/// Does `name` hold a placeholder?
pub fn is_template(name: &str) -> bool {
    parts(name).len() > 1
}

/// `name`'s text outside its placeholders. Between two templates matching
/// one field, the one with the longer literal text wins.
pub fn literal(name: &str) -> String {
    parts(name).into_iter().step_by(2).collect()
}

/// `name` with each placeholder one `x`: what the field-name rule judges of
/// a template, whose placeholders are not its text (`%hid:8%` holds a `:`).
pub fn skeleton(name: &str) -> String {
    let parts = parts(name);
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        out.push_str(if i % 2 == 0 { part } else { "x" });
    }
    out
}

/// Does `field` parse against `template`? `%hid:N%` takes exactly N
/// characters of the hid alphabet (`[0-9a-z]`), `%hid%` all twelve, `%uid%`
/// five; any other key one character or more, and `%?key%` any number. So
/// `issue__notes` is not an `issue__%hid:8%`. A `%hid:N%` reads as
/// interpolation writes it: N past twelve is twelve, and N under five
/// writes nothing and matches nothing.
pub fn matches(template: &str, field: &str) -> bool {
    let parts = parts(template);
    parts.len() > 1 && match_from(&parts, field)
}

fn match_from(parts: &[String], rest: &str) -> bool {
    let Some(rest) = rest.strip_prefix(parts[0].as_str()) else {
        return false;
    };
    let [_, key, tail @ ..] = parts else {
        return rest.is_empty();
    };
    let hid_len = match key.as_str() {
        "hid" => Some(HID_LEN),
        "uid" => Some(UID_LEN),
        k => match k.strip_prefix("hid:").map(str::parse::<usize>) {
            Some(Ok(n)) if n >= UID_LEN => Some(n.min(HID_LEN)),
            Some(_) => return false,
            None => None,
        },
    };
    if let Some(n) = hid_len {
        let taken = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
            .take(n)
            .count();
        return taken == n && match_from(tail, &rest[n..]);
    }
    let least = usize::from(!key.starts_with('?'));
    rest.char_indices()
        .map(|(i, _)| i)
        .chain([rest.len()])
        .skip(least)
        .any(|i| match_from(tail, &rest[i..]))
}

/// Could `a` and `b` both match one field with neither winning? Their
/// literal texts are as long, their first literals one a prefix of the other
/// and their last one a suffix — what any name matching both must start and
/// end with. `setup` refuses the second of two such templates.
pub fn ties(a: &str, b: &str) -> bool {
    let (a, b) = (parts(a), parts(b));
    let len = |p: &[String]| {
        p.iter()
            .step_by(2)
            .map(|l| l.chars().count())
            .sum::<usize>()
    };
    let either = |x: &str, y: &str, f: fn(&str, &str) -> bool| f(x, y) || f(y, x);
    let last = |p: &[String]| p.last().expect("never empty").clone();
    len(&a) == len(&b)
        && either(&a[0], &b[0], |x, y| x.starts_with(y))
        && either(&last(&a), &last(&b), |x, y| x.ends_with(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_splits_into_literals_and_keys() {
        assert_eq!(parts("issue__%hid:8%"), ["issue__", "hid:8", ""]);
        assert_eq!(parts("coffee"), ["coffee"]);
        assert_eq!(parts("50%"), ["50%"], "an unpaired percent is literal");
        assert_eq!(parts(r"a\%b%c%"), ["a%b", "c", ""]);
        assert!(is_template("issue__%hid:8%"));
        assert!(!is_template("50%"));
        assert!(!is_template(r"50\%off\%"), "escaped percents are literal");
        assert!(is_template("50%off%"), "a paired one is a placeholder");
        assert_eq!(literal("issue__%hid:8%"), "issue__");
        assert_eq!(skeleton("issue__%hid:8%"), "issue__x");
    }

    #[test]
    fn a_hid_placeholder_takes_exactly_its_length_of_the_alphabet() {
        let t = "issue__%hid:8%";
        assert!(matches(t, "issue__2qvk0dk4"));
        assert!(!matches(t, "issue__notes"), "five characters, not eight");
        assert!(!matches(t, "issue__2qvk0dk4x"), "nine");
        assert!(!matches(t, "issue__2QVK0DK4"), "not the hid alphabet");
        assert!(!matches(t, "task__2qvk0dk4"));
        assert!(matches("x%hid%", "x0123456789ab"));
        assert!(matches("x%uid%y", "x01234y"));
        assert!(!matches("issue__%hid:0%", "issue__"), "no hid is written");
        assert!(matches("x%hid:20%", "x0123456789ab"), "a hid is twelve");
    }

    #[test]
    fn any_other_key_takes_one_character_or_more() {
        assert!(matches("word__%srcLang%", "word__pt"));
        assert!(!matches("word__%srcLang%", "word__"));
        assert!(matches("word__%?srcLang%", "word__"));
        assert!(matches("%a%-%b%", "x-y-z"), "backtracks to a later literal");
        assert!(!matches("coffee", "coffee"), "a plain name is no template");
    }

    #[test]
    fn two_templates_tie_when_one_name_could_match_both_alike() {
        assert!(ties("issue__%hid:8%", "issue__%issue_id%"));
        assert!(!ties("issue__%hid:8%", "story__%hid:8%"), "no name is both");
        assert!(
            !ties("issue__%hid:8%", "issue__x%hid:8%"),
            "the longer wins"
        );
    }
}
