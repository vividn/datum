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
//! candidates, one `value<TAB>description` per line, and every failure is
//! an empty answer — the caller always exits 0.

use std::collections::BTreeSet;
use std::path::PathBuf;

use datum_core::db::{is_meta_field, Db};
use datum_core::shape::{is_ignored_state, TRACKED_STATE_VALUE};
use datum_core::track::NOT_A_DECLARATION_SQL;
use datum_parse::alias::{self as palias, expand_argv};
use datum_parse::help::{find_command, CommandSpec, FlagSpec, COMMANDS, GLOBAL_FLAGS};
use datum_parse::spec_keys::KeyKind;

use crate::args::DbAliasResolver;
use crate::config;

/// How many recently modified entries an entry position offers.
const RECENT_ENTRIES: usize = 50;

type Candidates = Vec<(String, String)>;

/// Run `__complete` on the words after it: `[--raw] <cword> -- <words…>`,
/// and print the candidates. `--raw` leaves values unquoted, for a shell
/// that quotes what it inserts itself (fish).
pub fn run(args: &[String]) {
    let raw = args.first().is_some_and(|a| a == "--raw");
    let args = &args[raw as usize..];
    let Some(cword) = args.first().and_then(|c| c.parse::<usize>().ok()) else {
        return;
    };
    let words = match args.get(1).map(String::as_str) {
        Some("--") => &args[2..],
        _ => &args[1..],
    };
    let mut out = String::new();
    for (value, desc) in complete(cword, words) {
        // One candidate per line, the value ended by a tab: a value holding
        // a control character (a field may hold a tab) cannot cross that,
        // nor be typed at a prompt, so it is not offered.
        if value.chars().any(char::is_control) {
            continue;
        }
        let value = if raw { value } else { shell_quote(&value) };
        out.push_str(&value);
        if !desc.is_empty() {
            out.push('\t');
            out.extend(desc.chars().map(|c| if c.is_control() { ' ' } else { c }));
        }
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
        } else if tok == "--db" || tok == "--show" {
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
            first_words(db)
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
        return first_words(db);
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
    let all_flags: Vec<&FlagSpec> = spec
        .flags
        .iter()
        .chain(sub.map_or(&[][..], |s| s.flags))
        .chain(GLOBAL_FLAGS)
        .collect();

    // A flag's value: only `--db` has anything to offer.
    if let Some(prev) = rest.last() {
        if let Some(flag) = lookup_flag(&all_flags, prev) {
            if flag.takes_value && !flag.value_optional && !prev.contains('=') {
                return value_of(&format!("--{}", flag.long));
            }
        }
    }
    if cur.starts_with('-') {
        return flags(all_flags.iter().copied());
    }
    if sub.is_none() && !spec.subcommands.is_empty() && positionals(&rest, &all_flags).is_empty() {
        let subs = spec
            .subcommands
            .iter()
            .map(|s| (s.name.to_string(), String::new()));
        return subs.collect();
    }

    let given = positionals(&rest, &all_flags);
    let usage = sub.map_or(spec.usage, |s| s.usage);
    let slots = slots(usage, 2 + sub.is_some() as usize);
    let slot = |n: usize| slots.get(n).or(slots.last().filter(|s| s.1)).map(|s| s.0);
    // Key/value words do not take a positional of their own in the usage
    // line's count: they fill the variadic tail.
    let n = given.iter().filter(|w| !w.contains('=')).count();
    let Some(db) = db else {
        return Vec::new();
    };
    match slot(n) {
        Some(Slot::Field) => fields(db),
        Some(Slot::Entry) if spec.name == "get" && cur.contains("::") => projections(db, cur),
        Some(Slot::Entry) => entries(db, cur),
        Some(Slot::Keys) => {
            let field = field_on_line(&slots, &given);
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
                    .filter(|(k, _)| !on_line.contains(k.trim_end_matches('='))),
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
                Slot::Other
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
fn positionals<'a>(rest: &'a [String], flags: &[&FlagSpec]) -> Vec<&'a String> {
    let mut out = Vec::new();
    let mut words = rest.iter();
    while let Some(w) = words.next() {
        if w.starts_with('-') && w.len() > 1 {
            if lookup_flag(flags, w).is_some_and(|f| f.takes_value && !f.value_optional)
                && !w.contains('=')
            {
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
            out.push((format!("--{}", f.long), f.summary.to_string()));
        }
        if let Some(c) = f.short {
            out.push((format!("-{c}"), f.summary.to_string()));
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
    names
        .into_iter()
        .map(|n| (n, String::from("database")))
        .collect()
}

fn open_db(name: Option<&str>) -> Option<Db> {
    let path: PathBuf = config::locate_db(name)?;
    if !path.is_file() {
        return None;
    }
    Db::open_read_only(path.to_str()?).ok()
}

/// The first word: the commands, then the db's aliases, kit commands and
/// fields.
fn first_words(db: Option<&Db>) -> Candidates {
    let mut out = Vec::new();
    for c in COMMANDS {
        for name in std::iter::once(&c.name).chain(c.aliases) {
            out.push((name.to_string(), c.summary.to_string()));
        }
    }
    let Some(db) = db else {
        return out;
    };
    out.extend(fields(db));
    for a in db.list_user_aliases().unwrap_or_default() {
        let desc = a.description.unwrap_or(a.expands_to);
        out.push((a.name, desc));
    }
    for k in db.list_kit_commands().unwrap_or_default() {
        let desc = k.help.or(k.expands_to).unwrap_or_default();
        out.push((k.name, desc));
    }
    out
}

/// The commands a field may be followed by (`coffee tail`).
fn field_first_commands() -> Candidates {
    COMMANDS
        .iter()
        .filter(|c| takes_field_first(c))
        .map(|c| (c.name.to_string(), c.summary.to_string()))
        .collect()
}

fn takes_field_first(c: &CommandSpec) -> bool {
    c.subcommands.is_empty()
        && slots(c.usage, 2)
            .first()
            .is_some_and(|s| s.0 == Slot::Field)
}

/// Every field — logged or declared — described by its verb and its
/// latest state.
fn fields(db: &Db) -> Candidates {
    let mut names: BTreeSet<String> = db
        .distinct_fields()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let registry = db.load_registry().ok();
    if let Some(r) = &registry {
        names.extend(r.fields.keys().filter(|f| !is_meta_field(f)).cloned());
    }
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let latest = db
        .batch_state_at(&refs, chrono::Utc::now())
        .unwrap_or_default();
    names
        .into_iter()
        .map(|f| {
            let verb = registry
                .as_ref()
                .and_then(|r| r.fields.get(&f))
                .map(|s| s.verb.to_string());
            let state = latest
                .get(&f)
                .filter(|s| **s != TRACKED_STATE_VALUE && !is_ignored_state(s))
                .map(label);
            let desc = [verb, state]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            (f, desc)
        })
        .collect()
}

/// A field's states: its spec's, then the ones logged.
fn states(db: &Db, field: &str) -> Candidates {
    let spec = db.load_spec(field).ok().flatten();
    let mut out: Candidates = Vec::new();
    for s in spec.iter().flat_map(|s| s.allowed_states.iter().flatten()) {
        let desc = spec
            .as_ref()
            .and_then(|sp| sp.state_descriptions.get(s).cloned())
            .unwrap_or_else(|| String::from("state"));
        out.push((s.clone(), desc));
    }
    let seen: BTreeSet<String> = out.iter().map(|(s, _)| s.clone()).collect();
    let logged: Vec<String> = db
        .conn()
        .prepare(&format!(
            "SELECT DISTINCT state FROM entries WHERE field = ?1 AND deleted_at IS NULL \
             AND superseded_by IS NULL AND state IS NOT NULL AND {NOT_A_DECLARATION_SQL}"
        ))
        .and_then(|mut stmt| stmt.query_map([field], |r| r.get(0))?.collect())
        .unwrap_or_default();
    for s in logged.iter().filter_map(|s| serde_json::from_str(s).ok()) {
        if is_ignored_state(&s) {
            continue;
        }
        let s = label(&s);
        if !seen.contains(&s) {
            out.push((s, String::from("logged")));
        }
    }
    out
}

/// A field's declared keys as `key=`, described by their kind.
fn keys(db: &Db, field: &str, state: Option<&str>) -> Candidates {
    let Some(spec) = db.load_spec(field).ok().flatten() else {
        return Vec::new();
    };
    spec.declared_keys(state)
        .into_iter()
        .map(|k| {
            let kind = match k.kind {
                KeyKind::Required => String::from("required"),
                KeyKind::Optional => String::from("optional"),
                KeyKind::Default(v) => format!("default {v}"),
                KeyKind::Assign(v) => format!("assign {v}"),
            };
            (format!("{}=", k.name), kind)
        })
        .collect()
}

/// An entry position: `_LAST`, the most recently modified entries' hids
/// (narrowed to the typed prefix), and — once an `_` is typed — the quick
/// ids a listing prints.
fn entries(db: &Db, cur: &str) -> Candidates {
    let mut out = vec![(
        String::from("_LAST"),
        String::from("the last entry touched"),
    )];
    if cur.starts_with('_') {
        let ordinals = datum_core::ids::resolve::quick_ordinals(db).unwrap_or_default();
        let mut by_n: Vec<_> = ordinals.into_iter().map(|(h, n)| (n, h)).collect();
        by_n.sort_by_key(|(n, _)| *n);
        let hids: Vec<String> = by_n.iter().map(|(_, h)| h.to_string()).collect();
        let described = describe(db, &hids);
        for ((n, _), (_, desc)) in by_n.iter().zip(described) {
            out.push((format!("_{n}"), desc));
        }
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
    out.extend(describe(db, &hids).into_iter().take(RECENT_ENTRIES));
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
    let at = |name: &str, desc: &str| (format!("{id}::{name}"), desc.to_string());
    let mut out = Vec::new();
    if let Ok(serde_json::Value::Object(row)) = serde_json::to_value(&entry) {
        out.extend(
            row.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, _)| at(k, "column")),
        );
    }
    if let Some(state) = &entry.state {
        if state_refs(spec.as_ref(), state).len() == 1 {
            out.push(at("state::", "reference"));
        }
    }
    for (k, v) in entry.data.iter().filter_map(|d| d.as_object()).flatten() {
        out.push(at(k, "data"));
        if data_refs(spec.as_ref(), k, v).len() == 1 {
            out.push(at(&format!("{k}::"), "reference"));
        }
    }
    out
}

/// `(hid, "<field> <state> <time>")` for each hid, in order, meta fields'
/// entries left out.
fn describe(db: &Db, hids: &[String]) -> Candidates {
    let mut stmt = match db
        .conn()
        .prepare("SELECT field, state, occur_time FROM entries WHERE hid = ?1")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    hids.iter()
        .filter_map(|h| {
            let (field, state, time): (String, Option<String>, Option<String>) = stmt
                .query_row([h], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .ok()?;
            if is_meta_field(&field) {
                return None;
            }
            let state = state
                .and_then(|s| serde_json::from_str(&s).ok())
                .map(|v| label(&v));
            let time = time.map(|t| t.chars().take(16).collect::<String>());
            let desc = [Some(field), state, time]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            Some((h.clone(), desc.join(" ")))
        })
        .collect()
}

fn label(v: &serde_json::Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

fn filtered(found: Candidates, cur: &str) -> Candidates {
    let mut seen = BTreeSet::new();
    found
        .into_iter()
        .filter(|(v, _)| v.starts_with(cur) && seen.insert(v.clone()))
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
