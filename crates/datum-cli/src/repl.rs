//! Interactive REPL — invoked when `datum` is run with no subcommand on a
//! terminal — and `datum script`, a script of the same lines (da-ygfeh).
//!
//! Each line is shell-split and dispatched through the same alias →
//! parser → command path as a fresh `datum` invocation, but against a
//! single long-lived `Db` handle. Errors don't kill the REPL; a script
//! stops at its first, unless `--allow-partial`.

use std::io::BufRead;

use anyhow::{Context, Result};
use datum_commands::args::{Cli, Command, Invocation};
use datum_commands::config;
use datum_core::db::Db;
use datum_parse::cli::ScriptArgs;
use datum_parse::{shell_split, ParseError};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

const PROMPT: &str = "datum> ";

/// Run the REPL against `db_hint` (the global `--db` value from argv, if
/// any). `None` resolves the default DB from config, matching plain
/// `datum` with no flags.
pub fn run(db_hint: Option<&str>) -> Result<()> {
    let (db, path) = open(db_hint)?;

    let history_path = config::config_dir().ok().map(|d| d.join("repl_history"));

    let mut rl = DefaultEditor::new()?;
    if let Some(p) = &history_path {
        let _ = rl.load_history(p);
    }

    println!("{}", crate::VERSION_BANNER);
    println!("Type `help` for commands, `exit` or Ctrl-D to quit.");

    loop {
        match rl.readline(PROMPT) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed, "exit" | "quit") {
                    break;
                }
                let _ = rl.add_history_entry(trimmed);
                if let Err(e) = run_line(trimmed, &db, Some(&path)) {
                    eprintln!("error: {e:#}");
                }
                // A dedup is the line's, not the session's: left set, the
                // REPL exited 11 once any line had deduped (da-3f9nl).
                datum_commands::args::reset_duplicate_noop();
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }

    if let Some(p) = &history_path {
        let _ = rl.save_history(p);
    }
    Ok(())
}

/// `datum readline <command> [args…]` (da-xloff): each line typed runs
/// `<command> [args…] -- <line>` against one database, the line one word
/// exactly as typed, an empty one included, stamped at its Enter. Each line
/// says what it wrote, or that it failed, and the loop goes on ([`indicate`]).
/// Ctrl-D or Ctrl-C ends it, the half-typed line unsaved.
pub fn readline(base: &[String], db_hint: Option<&str>) -> Result<()> {
    let (db, path) = open(db_hint)?;
    read_lines(base, &db, Some(&path))
}

/// [`readline`]'s loop on `db`, the file at `path` (a copy with none).
fn read_lines(base: &[String], db: &Db, path: Option<&str>) -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    // Quiet: `DATUM_SHOW` keeps each line's `raw_input` as typed, and a
    // `--show` of the base's own, or the caller's `DATUM_SHOW`, wins. Put
    // back as it ends, for a kit script's next line.
    struct Quiet(bool);
    impl Drop for Quiet {
        fn drop(&mut self) {
            if self.0 {
                std::env::remove_var("DATUM_SHOW");
            }
        }
    }
    let _quiet = Quiet(std::env::var_os("DATUM_SHOW").is_none());
    if _quiet.0 {
        std::env::set_var("DATUM_SHOW", "none");
    }
    // A parse error's hint is about the command, which the base is: the
    // line after `--` is never read as one.
    let named = base.join(" ");
    loop {
        let line = match rl.readline("") {
            Ok(line) => line,
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut argv = vec!["datum".to_string()];
        argv.extend_from_slice(base);
        // A `--` of the base's own already ends its flags.
        if !base.iter().any(|w| w == "--") {
            argv.push("--".into());
        }
        argv.push(line.clone());
        let (result, said, printed) =
            datum_commands::render::json_echo::tapped(|| run_argv(argv, &named, db, path));
        datum_commands::args::reset_duplicate_noop();
        let said = match result {
            Err(e) => format!("FAILED {e:#}"),
            Ok(()) if said.is_empty() => continue,
            Ok(()) => said.join(", "),
        };
        indicate(&line, &said, printed);
    }
}

/// Say what a `readline` line did (da-jt3k8): on a terminal after the text
/// typed, on its line, when nothing was printed below it; otherwise on a
/// line of its own, on stderr, the base's stdout left its own.
fn indicate(line: &str, said: &str, printed: bool) {
    use std::io::{IsTerminal, Write};
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if !tty || printed {
        eprintln!("{said}");
        return;
    }
    // The cursor is at the start of the row under the line's last: up one,
    // past what of the line that row holds, and back down after. A line
    // filling its last row exactly is followed by an empty row rustyline
    // wraps to, so column 0 there overwrites nothing.
    let cols = datum_commands::render::term_layout::detect_cols();
    let col = datum_commands::render::visible_width(line) % cols;
    let right = if col > 0 {
        format!("\x1b[{col}C")
    } else {
        String::new()
    };
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b[A\r{right}  {said}\r\n");
    let _ = out.flush();
}

/// A kit command's script (da-et0lm): each of its lines, filled, run in
/// order as `datum <line>` on `db`, the file at `path`; a `readline` line
/// reads its lines until Ctrl-D. The first line refused stops it, naming
/// that line.
pub fn kit_script(
    name: &str,
    lines: Vec<(usize, String, Vec<String>)>,
    db: &Db,
    path: &str,
) -> Result<()> {
    for (n, line, words) in lines {
        let mut argv = vec!["datum".to_string()];
        argv.extend(words);
        // A `readline` line reads its lines here; a REPL's or a `script`'s
        // still refuses one.
        let result = match datum_commands::args::parse_invocation_with_db(argv.clone(), db) {
            Ok(Invocation::Single(Cli {
                command: Command::Readline(base),
                ..
            })) => read_lines(&base, db, Some(path)),
            _ => run_argv(argv, &line, db, Some(path)),
        };
        datum_commands::args::reset_duplicate_noop();
        result.with_context(|| {
            format!("kit command '{name}' stopped at line {n}, «{line}»: the lines after it did not run")
        })?;
    }
    Ok(())
}

/// The database `db_hint` names, opened to write, and its path.
fn open(db_hint: Option<&str>) -> Result<(Db, String)> {
    let resolved = config::resolve_db(db_hint)?;
    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }
    let path = resolved.path.to_string_lossy().into_owned();
    let db = Db::open(&path)?;
    datum_commands::announce_migration(&db);
    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }
    Ok((db, path))
}

/// `datum script`, and `datum < FILE` with its defaults (da-ygfeh): each line
/// as the REPL runs it, stopping at the first that fails. `--atomic` runs
/// them on a snapshot and lands what they signed as one queued change, so
/// one transaction and one action on the undo stack; `--dry-run` on a copy.
pub fn script(args: &ScriptArgs, db_hint: Option<&str>) -> Result<()> {
    let _held = Held {
        tz: args.tz.as_ref().map(|_| std::env::var_os("TZ")),
        clock: datum_parse::time::clock::current(),
    };
    if let Some(tz) = &args.tz {
        std::env::set_var("TZ", datum_parse::time::iana_zone(tz)?);
    }
    if let Some(now) = &args.now {
        let time = datum_parse::input::TimeArgs {
            time: Some(now.clone()),
            ..Default::default()
        };
        let at = time.build(&datum_parse::DatumTime::now())?.time;
        let at = at.context("--now names no time")?;
        let clock = datum_parse::time::clock::MockClock::new(at);
        datum_parse::time::clock::install(std::sync::Arc::new(clock));
    }
    let input: Box<dyn BufRead> = match args.file.as_deref() {
        // Typed on a terminal, a bare `script` would wait on lines no one sends.
        None if std::io::IsTerminal::is_terminal(&std::io::stdin()) => {
            anyhow::bail!("script takes a file: datum script <FILE>, or `-` to type one")
        }
        None | Some("-") => Box::new(std::io::stdin().lock()),
        Some(file) => Box::new(std::io::BufReader::new(
            std::fs::File::open(file).with_context(|| format!("cannot read {file}"))?,
        )),
    };
    let db_hint = args.db.as_deref().or(db_hint);
    let (db, path) = match crate::missing(db_hint) {
        Some(_) if args.dry_run => (Db::open_in_memory()?, String::new()),
        _ => open(db_hint)?,
    };
    // An atomic script says QUEUED when a log would, checked before its
    // lines run, as each prints before the script lands (da-zwvok).
    let mut queues = false;
    let (db, spool) = match (args.dry_run, args.atomic) {
        (true, _) if path.is_empty() => (db, None),
        (true, _) => (db.copy_in_memory()?, None),
        (false, true) => {
            queues = crate::ready_to_write(&db, &path, true).is_err();
            drop(db);
            let (snapshot, spool) = Db::spool_open(&path)?;
            (snapshot, Some(spool))
        }
        (false, false) => (db, None),
    };
    let _queued = queues.then(datum_commands::render::Queued::hold);
    // Its lines echo entries the snapshot alone holds until it lands.
    #[cfg(unix)]
    let interrupt = spool
        .as_ref()
        .map(|_| datum_commands::interrupt::Said::install());
    // A copy's lines run on it alone: a lock or a question never reaches
    // the file.
    let on_file = (!args.dry_run && !args.atomic).then_some(path.as_str());
    let _dry = args.dry_run.then(datum_commands::render::Queued::dry_run);

    let (mut failed, mut count) = (Vec::new(), 0);
    // A closed stdout stops no line of an atomic script: each goes on with
    // its receipts unread, and every line lands (da-rdgr1).
    let mut broke = false;
    for (n, line) in input.lines().enumerate() {
        let line = line?;
        #[cfg(unix)]
        datum_commands::interrupt::LINE.store(n + 1, std::sync::atomic::Ordering::Relaxed);
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }
        count += 1;
        let result = if args.atomic {
            lands_whole(line, &db)
                .and_then(|()| crate::broke_stdout(|| run_line(line, &db, on_file)))
                .map(|b| broke |= b)
        } else {
            run_line(line, &db, on_file)
        };
        datum_commands::args::reset_duplicate_noop();
        if let Err(e) = result {
            eprintln!("error: line {}: {e:#}", n + 1);
            failed.push((n + 1).to_string());
            if !args.allow_partial {
                break;
            }
        }
    }

    let nothing = if args.dry_run || args.atomic {
        "nothing was written"
    } else {
        "the lines after it did not run"
    };
    match failed.as_slice() {
        [] => {}
        [line] if !args.allow_partial => anyhow::bail!("stopped at line {line}: {nothing}"),
        lines => anyhow::bail!(
            "{} of {count} lines failed: {}",
            lines.len(),
            lines.join(", ")
        ),
    }
    if args.dry_run {
        eprintln!("[dry-run] nothing written");
    }
    if let Some(spool) = spool {
        // From here a Ctrl-C ends it as it would any command.
        #[cfg(unix)]
        drop(interrupt);
        spool.append(&db)?;
        drop(db);
        let db = Db::open(&path)?;
        if let Some(e) = db.drain_spool()?.stuck {
            anyhow::bail!("the script is queued in {path}.spool, and did not land: {e}");
        }
        if db.spool_waiting() {
            eprintln!("note: another holds the write lock: the script is queued, and lands at the next command");
        }
    }
    if broke {
        std::process::exit(crate::EXIT_BROKEN_PIPE);
    }
    Ok(())
}

/// What `--tz` and `--now` changed, put back as a script ends, for a
/// caller in the same process.
struct Held {
    tz: Option<Option<std::ffi::OsString>>,
    clock: Option<std::sync::Arc<dyn datum_parse::time::clock::Clock>>,
}

impl Drop for Held {
    fn drop(&mut self) {
        match self.tz.take() {
            Some(Some(tz)) => std::env::set_var("TZ", tz),
            Some(None) => std::env::remove_var("TZ"),
            None => {}
        }
        match self.clock.take() {
            Some(clock) => datum_parse::time::clock::install(clock),
            None => datum_parse::time::clock::uninstall(),
        };
    }
}

/// Refuse a line of an `--atomic` script whose write one queued change
/// cannot carry: the file's, the network's or a device's, a kit's DDL, or a
/// `-L` claim, which holds only against the database as it lands.
fn lands_whole(line: &str, db: &Db) -> Result<()> {
    let mut argv = vec!["datum".to_string()];
    argv.extend(shell_split(line));
    let Ok(invocation) = datum_commands::args::parse_invocation_with_db(argv, db) else {
        return Ok(());
    };
    let whole = crate::sides(&invocation)
        .iter()
        .all(|c| crate::spools(&c.command) || crate::only_reads(&c.command));
    if !whole {
        anyhow::bail!(
            "an --atomic script lands as one write of entries, which a db, sync, config or kit \
             write, or a -L claim, is not: run this line apart"
        );
    }
    Ok(())
}

/// Run one line as `datum <line>` would run against the file at `path`: a
/// write the lock refuses is queued, printing QUEUED, and lands later
/// (da-3j6g7). With no `path`, `db` is a copy, and the line runs on it.
fn run_line(line: &str, db: &Db, path: Option<&str>) -> Result<()> {
    let mut argv = vec!["datum".to_string()];
    argv.extend(shell_split(line));
    run_argv(argv, line, db, path)
}

/// [`run_line`] of a line already split into `argv`.
fn run_argv(argv: Vec<String>, line: &str, db: &Db, path: Option<&str>) -> Result<()> {
    let parse = |argv| datum_commands::args::parse_invocation_with_db(argv, db);
    // `run`'s dry-run seam (da-7r1v4) on the REPL's open database, which is
    // always there: keep the two alike.
    if (0..argv.len()).any(crate::dry_run_flag(&argv))
        && !parse(argv.clone()).is_ok_and(|i| {
            crate::sides(&i)
                .iter()
                .all(|c| crate::dry_runs_itself(&c.command))
        })
    {
        if let Ok(invocation) = parse(crate::without_dry_run(&argv)) {
            return crate::dry_run(&argv, invocation, Some(db));
        }
    }
    match parse(argv.clone()) {
        Ok(invocation) => match path {
            None => {
                let color = crate::sides(&invocation).first().and_then(|c| c.color);
                let _color = color.map(crate::output::colors::ColorOverride::set);
                datum_commands::cmd::dispatch_invocation(invocation, db)
            }
            Some(path) => match crate::run_on(db, path, invocation) {
                Err(e) if e.is::<crate::Locked>() => crate::queue(path, parse(argv)?),
                r => r,
            },
        },
        Err(e) => {
            // The REPL deliberately omits the kit-level-2 fallback that
            // `lib.rs::run` adds for unknown subcommands — an unknown
            // command at the prompt just prints the error and the user
            // retries.
            if let Some(pe) = e.downcast_ref::<ParseError>() {
                if let Some(result) = crate::render_parse_special(pe, Some(db)) {
                    return result;
                }
                let rest = line
                    .trim_start()
                    .strip_prefix("datum")
                    .unwrap_or(line)
                    .trim();
                if let Some(hint) = crate::leading_datum(pe, rest) {
                    anyhow::bail!(hint);
                }
                if let Some(hint) = crate::archived_state(pe, db) {
                    anyhow::bail!(hint);
                }
            }
            Err(e)
        }
    }
}
