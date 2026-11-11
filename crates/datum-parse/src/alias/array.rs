//! An array of fields on a write command (da-p2ro6): `moment a,b -t 9 x=1` is
//! `moment a -t 9 x=1 , moment b -t 9 x=1`, the one batch a `,` line already
//! is, so it writes at one instant, all or nothing, and undoes as one.
//!
//! This runs on a side once the regions have made it flat, so a side an alias
//! wrote or filled takes an array as a typed one does, and nothing past here
//! knows an array was typed but `stop`, which `in_array` on
//! [`ExpandedSides`](super::ExpandedSides) tells that a field not running is a
//! failure (`LogArgs::in_array`).

use super::expand::{command_named, max_placeholder, Resolver};
use super::regions::{is_flag, is_value_flag};
use crate::error::{ParseError, Result};
use crate::spec_keys::{parse_key_decl, spec_key_words, takes_spec_keys, typed_state};

/// The commands whose field may be an array. `track` and `ignore` split
/// their own; `add` takes a card by an id, and `setup` one spec at a time.
const ARRAY_COMMANDS: &[&str] = &["occur", "start", "stop", "end", "switch", "flag"];

/// Each side with an array of fields as one side a field, and whether it came
/// from one, in order.
pub(super) fn expand_field_arrays(
    sides: Vec<Vec<String>>,
    resolver: &dyn Resolver,
) -> Result<(Vec<Vec<String>>, Vec<bool>)> {
    let mut out = Vec::with_capacity(sides.len());
    let mut in_array = Vec::with_capacity(sides.len());
    for side in sides {
        let at = ARRAY_COMMANDS
            .contains(&command_named(side.first().map_or("", String::as_str)))
            .then(|| field_slot(&side))
            .flatten()
            .filter(|&at| side[at].contains(','));
        let Some(at) = at else {
            out.push(side);
            in_array.push(false);
            continue;
        };
        let names = names_of(&side[at], &side[0], resolver)?;
        refuse_what_no_array_means(&side, at, &names, resolver)?;
        for (i, name) in names.into_iter().enumerate() {
            let mut one = side.clone();
            one[at] = name;
            if i > 0 {
                // The line's alias is the array's, recorded once.
                one = without_alias_flags(one);
            }
            out.push(one);
            in_array.push(true);
        }
    }
    Ok((out, in_array))
}

/// Where a side's field stands: its first bare word, wherever a flag or a
/// `key=value` sits.
fn field_slot(side: &[String]) -> Option<usize> {
    let mut i = 1;
    while let Some(tok) = side.get(i) {
        if tok == "--" {
            return None;
        } else if is_flag(tok) {
            i += usize::from(is_value_flag(tok));
        } else if crate::input::split_key_value(tok).is_none() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The names of `array`, each its alias expanded alone, each once.
fn names_of(array: &str, command: &str, resolver: &dyn Resolver) -> Result<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for name in array.split(',') {
        if name.trim().is_empty() {
            return Err(ParseError::InvalidArg(format!(
                "`{array}` has an empty name: an array of fields takes no spaces, as in `a,b`"
            )));
        }
        let name = field_named(name, command, resolver)?;
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// The field `name` stands for in an array of `command`'s: itself, or what
/// its alias names, when that alias names only a field. One that says more is
/// a whole command, and nothing joins it to this one.
fn field_named(name: &str, command: &str, resolver: &dyn Resolver) -> Result<String> {
    let mut name = name.to_string();
    let mut visited: Vec<String> = Vec::new();
    loop {
        if visited.contains(&name) {
            visited.push(name);
            return Err(ParseError::AliasCycle(visited));
        }
        let Some(alias) = resolver.lookup(&name) else {
            return Ok(name);
        };
        visited.push(name.clone());
        match alias.tokens.as_slice() {
            // A field's own word (`coffee` → `occur coffee -k dose`) is that field.
            [_, field, ..] if *field == name => return Ok(name),
            [word] if max_placeholder(&alias.tokens) == 0 => name = word.clone(),
            [verb, field] if same_command(verb, command) => return Ok(field.clone()),
            _ => {
                return Err(ParseError::InvalidArg(format!(
                    "alias '{name}' expands to a whole command (`{}`), which cannot join an \
                     array of fields: run it with a `,` between the commands",
                    alias.expands_to
                )))
            }
        }
    }
}

fn same_command(a: &str, b: &str) -> bool {
    let word = |w| match command_named(w) {
        "end" => "stop",
        w => w,
    };
    word(a) == word(b)
}

/// Refuse what an array cannot mean: `-u`, which takes back one field's line
/// on its own, and a word that goes to a different place on each field.
fn refuse_what_no_array_means(
    side: &[String],
    at: usize,
    names: &[String],
    resolver: &dyn Resolver,
) -> Result<()> {
    if side.iter().any(|t| t == "-u" || t == "--undo") {
        return Err(ParseError::InvalidArg(
            "-u takes back one field's line, not an array's: use `datum undo`".into(),
        ));
    }
    let verb = command_named(&side[0]);
    let mut one: Vec<&str> = side[1..].iter().map(String::as_str).collect();
    one[at - 1] = &names[0];
    let Ok((_, tail)) = crate::input::take_show(&one) else {
        return Ok(());
    };
    // A line that does not parse says why when each side does.
    let Ok(line) = crate::parse::log::parse_log_args(&tail) else {
        return Ok(());
    };
    if !takes_spec_keys(verb, &line) {
        return Ok(());
    }
    let state = (verb == "switch").then(|| typed_state(&line)).flatten();
    let words = line
        .rest
        .iter()
        .filter(|t| crate::input::split_key_value(t).is_none());
    let takes_state = verb == "switch" && !line.rest.iter().any(|t| t.starts_with("state="));
    let words: Vec<&String> = match line.string_state && takes_state {
        // `-I` joins them all into the state.
        true => Vec::new(),
        false => words.skip(usize::from(takes_state)).collect(),
    };
    let unfilled = words
        .len()
        .saturating_sub(line.keys.len() + line.slug_keys.len());
    if unfilled == 0 {
        return Ok(());
    }
    let takes = |field: &str| {
        let keys = spec_key_words(
            &line,
            &resolver.spec_keys(field, state.as_deref()),
            &resolver.spec_remainder(field),
        );
        let remainder = keys.remainder.clone();
        let comment = keys.comment_remainder.then(|| "the comment".to_string());
        let slots = keys.slug_keys.iter().chain(&keys.keys);
        let slots = slots.map(|k| parse_key_decl(k).0.to_string());
        let mut takes: Vec<String> = slots.take(unfilled).collect();
        if let Some(rest) = remainder.or(comment) {
            takes.resize(unfilled, rest);
        }
        takes
    };
    let word = words[words.len() - unfilled];
    let first = takes(&names[0]);
    for name in names {
        let these = takes(name);
        if these != first {
            let says = |f: &str, k: &[String]| match k.first() {
                Some(k) => format!("{k} on {f}"),
                None => format!("no key on {f}"),
            };
            return Err(ParseError::InvalidArg(format!(
                "`{word}` would be {} and {}: name its key with key={word}, or log them apart",
                says(&names[0], &first),
                says(name, &these),
            )));
        }
        // Past a field's keys a start or a switch reads a duration, as it
        // does alone; a moment has nowhere for the word to go.
        if verb == "occur" && these.len() < unfilled {
            return Err(ParseError::InvalidArg(format!(
                "`{word}` has no key on {name}: name its key with key={word}, or log them apart"
            )));
        }
    }
    Ok(())
}

/// `side` without what records the line's alias: a name after the first has
/// nothing to record.
fn without_alias_flags(side: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(side.len());
    let mut words = side.into_iter();
    while let Some(tok) = words.next() {
        match tok.as_str() {
            "--alias" => drop(words.next()),
            "--alias-overwrite" => {}
            t if t.starts_with("--alias=") => {}
            _ => out.push(tok),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::expand::{shell_split, MapResolver, NoResolver, ResolvedAlias};
    use super::*;
    use std::collections::HashMap;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn expanded(side: &[&str]) -> Result<Vec<Vec<String>>> {
        expand_field_arrays(vec![v(side)], &NoResolver).map(|(sides, _)| sides)
    }

    #[test]
    fn the_rest_of_the_line_goes_with_each_name() {
        assert_eq!(
            expanded(&["occur", "-t", "9", "a,b", "x=1"]).unwrap(),
            vec![
                v(&["occur", "-t", "9", "a", "x=1"]),
                v(&["occur", "-t", "9", "b", "x=1"]),
            ]
        );
    }

    #[test]
    fn a_state_with_a_comma_stays_one_state() {
        assert_eq!(
            expanded(&["switch", "a,b", "x,y"]).unwrap(),
            vec![v(&["switch", "a", "x,y"]), v(&["switch", "b", "x,y"])]
        );
        assert_eq!(
            expanded(&["switch", "a", "x,y"]).unwrap(),
            vec![v(&["switch", "a", "x,y"])]
        );
    }

    #[test]
    fn a_name_twice_is_one_name() {
        assert_eq!(
            expanded(&["occur", "a,a"]).unwrap(),
            vec![v(&["occur", "a"])]
        );
    }

    #[test]
    fn an_empty_name_is_refused_saying_how_to_write_it() {
        for array in ["a,", ",a", "a,,b", "a, ,b"] {
            let err = expanded(&["occur", array, "b"]).unwrap_err().to_string();
            assert!(err.contains("empty name"), "{array}: {err}");
        }
    }

    #[test]
    fn a_command_with_no_array_keeps_its_comma() {
        for side in [&["note", "a,b"][..], &["add", "a,b"], &["setup", "a,b"]] {
            assert_eq!(expanded(side).unwrap(), vec![v(side)]);
        }
    }

    #[test]
    fn a_later_side_does_not_record_the_lines_alias_again() {
        let sides = expanded(&["occur", "a,b", "--alias", "ab", "--alias-overwrite"]).unwrap();
        assert_eq!(
            sides[0],
            v(&["occur", "a", "--alias", "ab", "--alias-overwrite"])
        );
        assert_eq!(sides[1], v(&["occur", "b"]));
    }

    fn resolver() -> HashMap<String, ResolvedAlias> {
        let alias = |to: &str| ResolvedAlias {
            expands_to: to.into(),
            tokens: shell_split(to),
        };
        HashMap::from([
            ("coffee".to_string(), alias("occur coffee")),
            ("caf".to_string(), alias("coffee")),
            ("c".to_string(), alias("occur coffee")),
            ("dose".to_string(), alias("occur dose -k amount")),
            ("outside".to_string(), alias("switch environment outside")),
        ])
    }

    #[test]
    fn each_name_expands_its_alias_alone() {
        let a = resolver();
        for (array, want) in [
            ("coffee,tea", ["coffee", "tea"]),
            ("caf,tea", ["coffee", "tea"]),
            ("c,tea", ["coffee", "tea"]),
            ("dose,tea", ["dose", "tea"]),
        ] {
            let (sides, _) =
                expand_field_arrays(vec![v(&["occur", array])], &MapResolver(&a)).unwrap();
            assert_eq!(sides, vec![v(&["occur", want[0]]), v(&["occur", want[1]])]);
        }
    }

    #[test]
    fn a_cycle_of_bare_words_says_its_path() {
        let alias = |to: &str| ResolvedAlias {
            expands_to: to.into(),
            tokens: shell_split(to),
        };
        let a = HashMap::from([("x".to_string(), alias("y")), ("y".to_string(), alias("x"))]);
        let err = expand_field_arrays(vec![v(&["occur", "x,t"])], &MapResolver(&a)).unwrap_err();
        assert_eq!(err, ParseError::AliasCycle(v(&["x", "y", "x"])));
    }

    #[test]
    fn an_alias_that_is_a_whole_command_is_refused_saying_so() {
        let a = resolver();
        for (side, name) in [
            (["occur", "outside,tea"], "outside"),
            // `c` is a moment of coffee, which `start` cannot make of it.
            (["start", "c,tea"], "c"),
        ] {
            let err = expand_field_arrays(vec![v(&side)], &MapResolver(&a))
                .unwrap_err()
                .to_string();
            assert!(err.contains(name) && err.contains("whole command"), "{err}");
        }
    }
}
