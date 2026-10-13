//! `datum __complete <cword> -- <words…>` — the one thing a shell's
//! completion script asks (da-bu09u).
//!
//! The scripts `datum completions <shell>` prints only gather the words and
//! call this; every answer comes from here, so completion cannot drift from
//! the CLI. The grammar is `datum-parse`'s own command table — its flags,
//! subcommands, and the placeholders in its usage lines (`<FIELD>`,
//! `<ID_OR_SLUG>`, `[VALUE | KEY=VALUE]...`) that say what each positional
//! is. The words before the cursor go through the same pre-pass the parser's
//! input does (aliases, `coffee tail` → `tail coffee`), so an alias
//! completes as what it expands to.
//!
//! The db is opened READ-ONLY with no migration ([`Db::open_read_only`]) and
//! never created: completion is a reader, and a TAB must not reshape the
//! file. With no db, only the grammar answers. Nothing is printed but the
//! candidates, one value per line with no description (da-8bkh5: they made
//! the list long, and a field's latest state confused it), and every
//! failure is an empty answer — the caller always exits 0.
//!
//! A file position (da-2oz7k) — a usage line's `PATH`, `FILE` or `DIR` —
//! is answered by the shell itself: `__complete` prints [`FILES`] or
//! [`DIRS`], which each script hands to its own file completion, so `~`,
//! escaping and descending into directories are the shell's as for any
//! other command. Which positions those are stays here.

use std::collections::BTreeSet;
use std::path::PathBuf;

use datum_core::db::{is_meta_field, Db, NOT_FLAG_OR_META_SQL};
use datum_core::shape::is_ignored_state;
use datum_core::track::NOT_A_DECLARATION_SQL;
use datum_parse::alias::{self as palias, expand_argv};
use datum_parse::help::{find_command, CommandSpec, FlagSpec, COMMANDS, GLOBAL_FLAGS};

use crate::args::DbAliasResolver;
use crate::config;

/// How many recently modified entries an entry position offers.
const RECENT_ENTRIES: usize = 50;

/// The line that asks the shell for its file completion, and for its
/// directories alone. No candidate holds a tab, so neither is ever one.
pub const FILES: &str = "\tfile";
pub const DIRS: &str = "\tdir";

type Candidates = Vec<String>;

/// Where `__complete` stands in `argv`, when only global options come
/// before it: an alias's own (`alias dm='datum --db v3'`, da-ejoes).
pub fn position(argv: &[String]) -> Option<usize> {
    let mut i = 1;
    loop {
        let word = argv.get(i)?;
        if word == "__complete" {
            return Some(i);
        }
        i += 1 + usize::from(global_takes_value(word)?);
    }
}

/// Whether the global option `word` takes the next word as its value, or
/// `None` when it is not a global option.
fn global_takes_value(word: &str) -> Option<bool> {
    let globals: Vec<&FlagSpec> = GLOBAL_FLAGS.iter().collect();
    lookup_flag(&globals, word).map(|f| f.takes_value && !f.value_optional && !word.contains('='))
}

/// Run `__complete` on the words after it: `[--raw] <cword> -- <words…>`,
/// and print the candidates. `--raw` leaves values unquoted, for a shell
/// that quotes what it inserts itself (fish). `globals`, the options before
/// `__complete`, count as if typed after the command name.
pub fn run(globals: &[String], args: &[String]) {
    let raw = args.first().is_some_and(|a| a == "--raw");
    let args = &args[raw as usize..];
    let Some(cword) = args.first().and_then(|c| c.parse::<usize>().ok()) else {
        return;
    };
    let mut words = match args.get(1).map(String::as_str) {
        Some("--") => args[2..].to_vec(),
        _ => args[1..].to_vec(),
    };
    if !words.is_empty() {
        words.splice(1..1, globals.iter().cloned());
    }
    let mut out = String::new();
    for value in complete(cword + globals.len(), &words) {
        // One candidate per line: a value holding a control character (a
        // field may hold a newline) cannot cross that, nor be typed at a
        // prompt, so it is not offered.
        let sentinel = value == FILES || value == DIRS;
        if !sentinel && value.chars().any(char::is_control) {
            continue;
        }
        let value = if raw || sentinel {
            value
        } else {
            shell_quote(&value)
        };
        out.push_str(&value);
        out.push('\n');
    }
    use std::io::Write;
    let _ = std::io::stdout().write_all(out.as_bytes());
}

/// The candidates for `words[cword]`, where `words[0]` is the command name
/// the shell saw. Filtered to those starting with the word being typed.
pub fn complete(cword: usize, words: &[String]) -> Candidates {
    // A word the shell handed over still quoted (bash) is read as the
    // shell will read it.
    let words: Vec<String> = words.iter().map(|w| unquote(w)).collect();
    let cur = words.get(cword).map(String::as_str).unwrap_or("");
    // `words[0]` is the name the shell saw (`datum`, or what an alias became),
    // so the words before the cursor start at the first real one: global
    // flags, then the command.
    let before = words.get(1..cword.min(words.len())).unwrap_or(&[]);

    // The global flags in front of the command: `--db X` names the db.
    let mut db_name: Option<String> = None;
    let mut i = 0;
    while let Some(tok) = before.get(i) {
        if let Some(v) = tok.strip_prefix("--db=") {
            db_name = Some(v.to_string());
        } else if global_takes_value(tok) == Some(true) {
            match before.get(i + 1) {
                Some(v) if tok == "--db" => db_name = Some(v.clone()),
                Some(_) => {}
                None => return filtered(value_of(tok), cur),
            }
            i += 1;
        } else if !tok.starts_with('-') {
            break;
        }
        i += 1;
    }
    let db = open_db(db_name.as_deref());
    let db = db.as_ref();

    let rest: Vec<String> = before[i..].to_vec();
    if rest.is_empty() {
        let found = if cur.starts_with('-') {
            flags(GLOBAL_FLAGS)
        } else {
            first_words(db, cur)
        };
        return filtered(found, cur);
    }
    // A lone field is the field-first form (`coffee tail`), and a log line
    // of its own (`mood happy`): offer both.
    let lone_field = rest.len() == 1 && is_field(&rest[0], db);
    let mut found = if lone_field {
        field_first_commands()
    } else {
        Vec::new()
    };
    found.extend(after_command(rest, db, cur));
    filtered(found, cur)
}

/// Every field has an alias of its own name (`mood` → `switch mood`), and
/// `mood tail` is still the field-first form: a field is known by its spec.
fn is_field(word: &str, db: Option<&Db>) -> bool {
    find_command(word).is_none() && db.is_none_or(|db| db.load_spec(word).ok().flatten().is_some())
}

/// The candidates once there is a command (or a field) before the cursor.
fn after_command(mut rest: Vec<String>, db: Option<&Db>, cur: &str) -> Candidates {
    // The pre-pass the parser's input takes: aliases, the field-first swap.
    let mut argv = vec![String::from("datum")];
    argv.append(&mut rest);
    let expanded = match db {
        Some(db) => expand_argv(argv.clone(), &DbAliasResolver::new(db)).map(|(a, _)| a),
        None => expand_argv(argv.clone(), &palias::NoResolver).map(|(a, _)| a),
    };
    let mut rest = expanded.unwrap_or(argv).split_off(1);
    let multi = rest.iter().rposition(|w| w == palias::MULTI_COMMA);
    if let Some(at) = multi {
        rest.drain(..=at);
    }
    if rest.is_empty() {
        return first_words(db, cur);
    }
    let name = rest.remove(0);
    let Some((spec, _)) = find_command(&name) else {
        return Vec::new();
    };

    let sub = spec
        .subcommands
        .iter()
        .find(|s| rest.first().is_some_and(|w| w == s.name));
    if sub.is_some() {
        rest.remove(0);
    }
    let usage = sub.map_or(spec.usage, |s| s.usage);
    let all_flags: Vec<&FlagSpec> = spec
        .flags
        .iter()
        .chain(sub.map_or(&[][..], |s| s.flags))
        .chain(GLOBAL_FLAGS)
        .collect();

    // A flag's value: only `--db` and `--color` have anything to offer.
    if let Some(prev) = rest.last() {
        if let Some(flag) = lookup_flag(&all_flags, prev) {
            if flag.takes_value && !flag.value_optional && !prev.contains('=') {
                return value_of(&format!("--{}", flag.long));
            }
        }
        // A flag only a usage line names: `--to DIR`, `--admin-key <FILE>`.
        if let Some(v) = usage_value(usage, prev).filter(|_| !prev.contains('=')) {
            return file_slot(v).and_then(files).unwrap_or_default();
        }
    }
    if cur.starts_with('-') {
        return flags(all_flags.iter().copied());
    }
    if sub.is_none()
        && !spec.subcommands.is_empty()
        && positionals(&rest, &all_flags, usage).is_empty()
    {
        return spec
            .subcommands
            .iter()
            .map(|s| s.name.to_string())
            .collect();
    }

    let given = positionals(&rest, &all_flags, usage);
    // A group of its own (`sync chain`): the word after it in its usage.
    if let Some(s) = sub.filter(|_| given.is_empty()) {
        let mut words: Vec<String> = s
            .usage
            .iter()
            .filter_map(|l| group_tokens(l).into_iter().nth(3))
            .filter(|w| w.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
            .collect();
        words.dedup();
        if !words.is_empty() {
            return words;
        }
    }
    // In a group, the lines of the word after it (`sync admin add`).
    let lines: Vec<&str> = usage
        .iter()
        .copied()
        .filter(|l| sub.is_some() && group_tokens(l).get(3) == given.first().copied())
        .collect();
    let (slots, given) = if lines.is_empty() {
        (slots(usage, 2 + sub.is_some() as usize), &given[..])
    } else {
        (slots(&lines, 4), &given[1..])
    };
    let slot = |n: usize| slots.get(n).or(slots.last().filter(|s| s.1)).map(|s| s.0);
    // Key/value words do not take a positional of their own in the usage
    // line's count: they fill the variadic tail.
    let n = given.iter().filter(|w| !w.contains('=')).count();
    if let Some(found) = slot(n).and_then(files) {
        return found;
    }
    let Some(db) = db else {
        return Vec::new();
    };
    match slot(n) {
        Some(Slot::Field) => fields(db),
        Some(Slot::Entry) if spec.name == "get" && cur.contains("::") => projections(db, cur),
        Some(Slot::Entry) => entries(db, cur),
        Some(Slot::Keys) => {
            let field = field_on_line(&slots, given);
            // `switch`'s state is the word after the field.
            let state = (spec.name == "switch")
                .then(|| given.iter().filter(|w| !w.contains('=')).nth(1))
                .flatten()
                .map(|s| s.to_string());
            let mut found = Vec::new();
            if spec.name == "switch" && state.is_none() {
                found.extend(field.as_deref().map(|f| states(db, f)).unwrap_or_default());
            }
            let on_line: BTreeSet<&str> = given
                .iter()
                .filter_map(|w| w.split_once('='))
                .map(|(k, _)| k)
                .collect();
            let keys = field
                .as_deref()
                .map(|f| keys(db, f, state.as_deref()))
                .unwrap_or_default();
            found.extend(
                keys.into_iter()
                    .filter(|k| !on_line.contains(k.trim_end_matches('='))),
            );
            found
        }
        _ => Vec::new(),
    }
}

/// What a positional in a usage line is.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Slot {
    Field,
    Entry,
    Keys,
    /// A file, and `-` too when it is one of the alternatives (stdin).
    File {
        dash: bool,
    },
    Dir,
    Other,
}

/// Each usage line's positionals after `datum NAME [SUB]`, merged across
/// lines (the first line that says what a position is, says it), with
/// whether the last one repeats.
fn slots(usage: &[&str], skip: usize) -> Vec<(Slot, bool)> {
    let mut merged: Vec<(Slot, bool)> = Vec::new();
    for line in usage {
        let toks = group_tokens(line);
        let mut toks = toks.iter().skip(skip).peekable();
        let mut n = 0;
        while let Some(tok) = toks.next() {
            if tok.starts_with('-') || tok.starts_with("[-") {
                // A bare flag's `<VALUE>` is the flag's, not a positional.
                if tok.starts_with('-') && toks.peek().is_some_and(|t| t.starts_with('<')) {
                    toks.next();
                }
                continue;
            }
            let upper = tok.chars().any(|c| c.is_ascii_uppercase());
            if !(tok.starts_with('<') || tok.starts_with('[') || upper && !tok.starts_with('(')) {
                break;
            }
            let kind = if tok.contains("FIELD") {
                Slot::Field
            } else if tok.contains("KEY=VALUE") {
                Slot::Keys
            } else if ["ID_OR_SLUG", "SLUG", "HID"]
                .iter()
                .any(|k| tok.contains(k))
            {
                Slot::Entry
            } else {
                file_slot(tok).unwrap_or(Slot::Other)
            };
            let repeats = tok.ends_with("...");
            match merged.get_mut(n) {
                Some(s) if s.0 == Slot::Other => *s = (kind, repeats),
                Some(_) => {}
                None => merged.push((kind, repeats)),
            }
            n += 1;
        }
    }
    merged
}

/// A file position, by its placeholder — the contract `CommandSpec::usage`
/// states: a word ending in `DIR`, or in `FILE` / `PATH` (`KEYFILE`), with
/// `-` beside it when the dash is an alternative.
fn file_slot(tok: &str) -> Option<Slot> {
    let mut words = tok.split(|c: char| !c.is_ascii_uppercase());
    if words.clone().any(|w| w.ends_with("DIR")) {
        return Some(Slot::Dir);
    }
    let alts = tok.trim_matches(['<', '>', '[', ']', '(', ')']).split('|');
    let dash = alts.map(str::trim).any(|a| a == "-");
    words
        .any(|w| w.ends_with("FILE") || w.ends_with("PATH"))
        .then_some(Slot::File { dash })
}

/// The value a usage line gives `flag`, for a flag only the usage lines
/// name (`--to DIR`, `[--admin-key <FILE>]`); none where its group closes
/// on it (`--merge]`) or the next word is another flag or alternative.
fn usage_value<'a>(usage: &[&'a str], flag: &str) -> Option<&'a str> {
    let flag = flag.split('=').next().filter(|f| f.starts_with('-'))?;
    usage.iter().find_map(|line| {
        let toks: Vec<&str> = line.split_whitespace().collect();
        toks.windows(2)
            .find(|w| w[0].trim_start_matches(['[', '(']) == flag && !w[0].ends_with([']', ')']))
            .map(|w| w[1])
            .filter(|v| !v.starts_with(['-', '|', '[', '(']))
    })
}

/// A file position's answer: the shell's own completion, and `-`.
fn files(slot: Slot) -> Option<Candidates> {
    match slot {
        Slot::File { dash } => {
            let dash = dash.then(|| String::from("-"));
            Some(dash.into_iter().chain([FILES.to_string()]).collect())
        }
        Slot::Dir => Some(vec![DIRS.to_string()]),
        _ => None,
    }
}

/// Split a usage line on the spaces outside `<>`, `[]` and `()`.
fn group_tokens(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in line.chars() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ' ' if depth <= 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The positional words of `rest`: every word that is neither a flag nor
/// a flag's value.
fn positionals<'a>(rest: &'a [String], flags: &[&FlagSpec], usage: &[&str]) -> Vec<&'a String> {
    let mut out = Vec::new();
    let mut words = rest.iter();
    while let Some(w) = words.next() {
        if w.starts_with('-') && w.len() > 1 {
            let takes = lookup_flag(flags, w).map_or_else(
                || usage_value(usage, w).is_some(),
                |f| f.takes_value && !f.value_optional,
            );
            if takes && !w.contains('=') {
                words.next();
            }
            continue;
        }
        out.push(w);
    }
    out
}

fn lookup_flag<'a>(flags: &[&'a FlagSpec], word: &str) -> Option<&'a FlagSpec> {
    let word = word.split('=').next().unwrap_or(word);
    flags.iter().copied().find(|f| {
        (!f.long.is_empty() && word.strip_prefix("--") == Some(f.long))
            || f.short
                .is_some_and(|c| word.len() == 2 && word == format!("-{c}"))
    })
}

fn flags<'a>(flags: impl IntoIterator<Item = &'a FlagSpec>) -> Candidates {
    let mut out = Vec::new();
    for f in flags {
        if !f.long.is_empty() {
            out.push(format!("--{}", f.long));
        }
        if let Some(c) = f.short {
            out.push(format!("-{c}"));
        }
    }
    out
}

/// The field on a line: the word in the usage line's FIELD position.
fn field_on_line(slots: &[(Slot, bool)], given: &[&String]) -> Option<String> {
    let at = slots.iter().position(|s| s.0 == Slot::Field)?;
    given
        .iter()
        .filter(|w| !w.contains('='))
        .nth(at)
        .map(|w| w.to_string())
}

/// What a flag's value may be: the known databases for `--db`.
fn value_of(flag: &str) -> Candidates {
    if flag == "--color" {
        return ["auto", "always", "never"].map(String::from).to_vec();
    }
    if flag != "--db" {
        return Vec::new();
    }
    let mut names: BTreeSet<String> = config::load_registry()
        .map(|r| r.db.into_keys().collect())
        .unwrap_or_default();
    if let Some(dir) = config::data_dir_path() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".db") {
                names.insert(stem.to_string());
            }
        }
    }
    names.into_iter().collect()
}

fn open_db(name: Option<&str>) -> Option<Db> {
    let path: PathBuf = config::locate_db(name)?;
    if !path.is_file() {
        return None;
    }
    Db::open_read_only(path.to_str()?).ok()
}

/// The first word: with nothing typed, the commands' own names alone
/// (da-8bkh5); once a prefix is typed, their aliases too, and the db's
/// fields, aliases and kit commands.
fn first_words(db: Option<&Db>, cur: &str) -> Candidates {
    let mut out: Candidates = COMMANDS.iter().map(|c| c.name.to_string()).collect();
    if cur.is_empty() {
        return out;
    }
    out.extend(
        COMMANDS
            .iter()
            .flat_map(|c| c.aliases)
            .map(|a| a.to_string()),
    );
    let Some(db) = db else {
        return out;
    };
    out.extend(fields(db));
    out.extend(
        db.list_user_aliases()
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.name),
    );
    out.extend(
        db.list_kit_commands()
            .unwrap_or_default()
            .into_iter()
            .map(|k| k.name),
    );
    out
}

/// The commands a field may be followed by (`coffee tail`).
fn field_first_commands() -> Candidates {
    COMMANDS
        .iter()
        .filter(|c| takes_field_first(c))
        .map(|c| c.name.to_string())
        .collect()
}

fn takes_field_first(c: &CommandSpec) -> bool {
    c.subcommands.is_empty()
        && slots(c.usage, 2)
            .first()
            .is_some_and(|s| s.0 == Slot::Field)
}

/// Every field, logged or declared.
fn fields(db: &Db) -> Candidates {
    let mut names: BTreeSet<String> = db
        .distinct_fields()
        .unwrap_or_default()
        .into_iter()
        .collect();
    if let Ok(r) = db.load_registry() {
        names.extend(r.fields.into_keys().filter(|f| !is_meta_field(f)));
    }
    names.into_iter().collect()
}

/// A field's states: its spec's, then the ones logged.
fn states(db: &Db, field: &str) -> Candidates {
    let spec = db.load_spec(field).ok().flatten();
    let mut out: Candidates = spec
        .into_iter()
        .flat_map(|s| s.allowed_states.into_iter().flatten())
        .collect();
    let logged: Vec<String> = db
        .conn()
        .prepare(&format!(
            "SELECT DISTINCT state FROM entries WHERE field = ?1 AND deleted_at IS NULL \
             AND superseded_by IS NULL AND state IS NOT NULL AND {NOT_A_DECLARATION_SQL} \
             AND {NOT_FLAG_OR_META_SQL}"
        ))
        .and_then(|mut stmt| stmt.query_map([field], |r| r.get(0))?.collect())
        .unwrap_or_default();
    for s in logged.iter().filter_map(|s| serde_json::from_str(s).ok()) {
        if !is_ignored_state(&s) {
            out.push(label(&s));
        }
    }
    out
}

/// A field's declared keys as `key=`.
fn keys(db: &Db, field: &str, state: Option<&str>) -> Candidates {
    let Some(spec) = db.load_spec(field).ok().flatten() else {
        return Vec::new();
    };
    spec.declared_keys(state)
        .into_iter()
        .map(|k| format!("{}=", k.name))
        .collect()
}

/// An entry position: `_LAST`, the most recently modified entries' hids
/// (narrowed to the typed prefix), and — once an `_` is typed — the quick
/// ids a listing prints.
fn entries(db: &Db, cur: &str) -> Candidates {
    let mut out = vec![String::from("_LAST")];
    if cur.starts_with('_') {
        let ordinals = datum_core::ids::resolve::quick_ordinals(db).unwrap_or_default();
        let mut by_n: Vec<_> = ordinals.into_iter().map(|(h, n)| (n, h)).collect();
        by_n.sort_by_key(|(n, _)| *n);
        let by_n = by_n.into_iter().filter(|(_, h)| visible(db, h.as_ref()));
        out.extend(by_n.map(|(n, _)| format!("_{n}")));
        return out;
    }
    let like = format!("{}%", cur.replace(['%', '_'], ""));
    let hids: Vec<String> = db
        .conn()
        .prepare(&format!(
            "SELECT hid FROM entries WHERE deleted_at IS NULL AND superseded_by IS NULL \
             AND hid LIKE ?1 ORDER BY modify_time_ms DESC LIMIT {}",
            RECENT_ENTRIES * 2
        ))
        .and_then(|mut stmt| stmt.query_map([like], |r| r.get(0))?.collect())
        .unwrap_or_default();
    let hids = hids.into_iter().filter(|h| visible(db, h));
    out.extend(hids.take(RECENT_ENTRIES));
    out
}

/// `get <id>::` (da-u7mgc): the entry's columns and data keys, and
/// `<name>::` where that value is a reference to follow. One hop: past a
/// second `::` it offers nothing.
fn projections(db: &Db, cur: &str) -> Candidates {
    use datum_core::doc_ref::{data_refs, state_refs};
    let Some((id, name)) = cur.split_once("::") else {
        return Vec::new();
    };
    let Ok(entry) = datum_core::ids::resolve_quickid_single(db, id) else {
        return Vec::new();
    };
    if name.contains("::") {
        return Vec::new();
    }
    let spec = db.load_spec(&entry.field).ok().flatten();
    let at = |name: &str| format!("{id}::{name}");
    let mut out = Vec::new();
    if let Ok(serde_json::Value::Object(row)) = serde_json::to_value(&entry) {
        out.extend(row.iter().filter(|(_, v)| !v.is_null()).map(|(k, _)| at(k)));
    }
    if let Some(state) = &entry.state {
        if state_refs(spec.as_ref(), state).len() == 1 {
            out.push(at("state::"));
        }
    }
    for (k, v) in entry.data.iter().filter_map(|d| d.as_object()).flatten() {
        out.push(at(k));
        if data_refs(spec.as_ref(), k, v).len() == 1 {
            out.push(at(&format!("{k}::")));
        }
    }
    out
}

/// Whether an entry is offered: meta fields' entries are left out.
fn visible(db: &Db, hid: &str) -> bool {
    db.conn()
        .prepare_cached("SELECT field FROM entries WHERE hid = ?1")
        .and_then(|mut stmt| stmt.query_row([hid], |r| r.get::<_, String>(0)))
        .is_ok_and(|f| !is_meta_field(&f))
}

fn label(v: &serde_json::Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

fn filtered(found: Candidates, cur: &str) -> Candidates {
    let mut seen = BTreeSet::new();
    // A sentinel is the shell's to filter, by the word being typed.
    let sentinel = |v: &String| v == FILES || v == DIRS;
    found
        .into_iter()
        .filter(|v| (v.starts_with(cur) || sentinel(v)) && seen.insert(v.clone()))
        .collect()
}

fn unquote(word: &str) -> String {
    if !word.contains(['\'', '"', '\\']) {
        return word.to_string();
    }
    // An unterminated quote is a word still being typed: it reads as closed.
    palias::shell_split(word).join(" ")
}

/// A value as the shell must see it to read it back as one word: quoted
/// when it holds anything a shell splits or expands on (a field may hold
/// spaces, #283).
fn shell_quote(v: &str) -> String {
    let plain = |c: char| c.is_alphanumeric() || "-_=.:,/@+%^".contains(c);
    if v.chars().all(plain) {
        return v.to_string();
    }
    format!("'{}'", v.replace('\'', r"'\''"))
}
