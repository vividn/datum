//! `datum help <NAME>` for a name the database defines — an alias or a kit
//! command (da-3odno). The command table in `datum_parse::help` knows only
//! the built-in commands and topics, so without this an alias got the
//! general page, byte for byte what a typo gets, and datum.day's Help
//! button, which asks the CLI, explained no kit command at all.

use anyhow::Result;
use datum_core::db::Db;
use datum_parse::{help, ParseError};

/// The name a help request asks about when only the database can answer
/// it: `help NAME` or `NAME --help` naming no built-in command or topic.
/// A topic outranks an alias of its name, as after `help` no alias fires
/// (da-wd5ip): `help time` is the time page whatever `time` expands to.
pub fn asks_the_db(e: &ParseError) -> Option<&str> {
    match e {
        ParseError::HelpRequested(Some(name)) | ParseError::GrammarRequested(Some(name))
            if help::find_command(name).is_none() && help::find_topic(name).is_none() =>
        {
            Some(name)
        }
        _ => None,
    }
}

/// The page for `name` when the database defines it, in the resolver's
/// precedence: an alias, then a kit command.
pub fn page(name: &str, db: &Db) -> Result<Option<String>> {
    if let Some(a) = db.lookup_alias(name)? {
        let mut s = heading(name, a.description.as_deref(), "an alias");
        s.push_str(&format!("\nexpands to: datum {}\n", a.expands_to));
        // What may follow it is the page of what it expands to (da-wxan9).
        if let Some(next) = a.expands_to.split_whitespace().next() {
            if help::find_command(next).is_some() || db.lookup_alias(next)?.is_some() {
                s.push_str(&format!("options: datum help {next}\n"));
            }
        }
        if let Some(kit) = db.alias_kits()?.get(name) {
            s.push_str(&from_kit(kit));
        }
        return Ok(Some(s));
    }
    let Some(c) = db.get_kit_command(name)? else {
        return Ok(None);
    };
    let mut s = heading(name, c.help.as_deref(), "a kit command");
    if let Some(to) = &c.expands_to {
        s.push_str(&format!("\nexpands to: datum {to}\n"));
    } else {
        let mut usage = format!("datum {name}");
        for arg in &c.args {
            match arg.split_once('=') {
                None => usage.push_str(&format!(" <{arg}>")),
                Some((key, "")) => usage.push_str(&format!(" [{key}]")),
                Some(_) => usage.push_str(&format!(" [{arg}]")),
            }
        }
        for flag in c.switches.iter().chain(&c.flags) {
            usage.push_str(&format!(" [--{flag}]"));
        }
        s.push_str(&format!("\nusage: {usage} [--sort [-]COLUMN]\n"));
        if let Some(view) = &c.view_name {
            s.push_str(&format!("shows the view '{view}'\n"));
        }
    }
    if let Some(kit) = &c.kit_id {
        s.push_str(&from_kit(kit));
    }
    Ok(Some(s))
}

fn from_kit(kit: &str) -> String {
    format!("from the kit '{kit}': datum kit show {kit}\n")
}

fn heading(name: &str, about: Option<&str>, kind: &str) -> String {
    format!("datum {name} — {}\n", about.unwrap_or(kind))
}
