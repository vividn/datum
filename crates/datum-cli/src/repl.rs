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
use datum_parse::cli::{ReadlineArgs, ScriptArgs};
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
/// says what it wrote, or that it failed, and the loop goes on: on a
/// terminal the entry takes the line's place ([`Sitting`]), from a pipe a
/// line on stderr says it ([`say`]). Ctrl-D or Ctrl-C ends it, the
/// half-typed line unsaved.
pub fn readline(args: &ReadlineArgs, db_hint: Option<&str>) -> Result<()> {
    let (db, path) = open(db_hint)?;
    read_lines(args, &db, Some(&path))
}

/// [`readline`]'s loop on `db`, the file at `path` (a copy with none).
fn read_lines(args: &ReadlineArgs, db: &Db, path: Option<&str>) -> Result<()> {
    use std::io::IsTerminal;
    let base = &args.base;
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
    let argv = |line: &str| {
        let mut argv = vec!["datum".to_string()];
        argv.extend_from_slice(base);
        // A `--` of the base's own already ends its flags.
        if !base.iter().any(|w| w == "--") {
            argv.push("--".into());
        }
        argv.push(line.to_string());
        argv
    };
    let mut rl = rustyline::Editor::<Dim, rustyline::history::DefaultHistory>::new()?;
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let mut sitting = tty.then(|| {
        // The base's `-R` key, read from the command it parses to.
        let remainder = match datum_commands::args::parse_invocation_with_db(argv(""), db) {
            Ok(Invocation::Single(cli)) => cli.command.log_args().and_then(|a| a.remainder.clone()),
            _ => None,
        };
        Sitting {
            compact: args.compact,
            remainder,
            shown: Vec::new(),
        }
    });
    // Tab ends the edit for a redraw, keeping where the cursor was.
    let tab = std::sync::Arc::new(std::sync::Mutex::new(None));
    if tty {
        rl.set_helper(Some(Dim));
        rl.bind_sequence(
            rustyline::KeyEvent(rustyline::KeyCode::Tab, rustyline::Modifiers::NONE),
            rustyline::EventHandler::Conditional(Box::new(Toggle(tab.clone()))),
        );
    }
    let mut prompt = String::new();
    let mut held: Option<(String, usize)> = None;
    loop {
        // The clock is the time the line's prompt was drawn, once a line.
        if tty && held.is_none() {
            let now = datum_parse::DatumTime::now().local_time();
            prompt = format!("{} | ", now.format("%H:%M:%S"));
        }
        let initial = held.as_ref().map_or(("", ""), |(l, at)| l.split_at(*at));
        let line = match rl.readline_with_initial(prompt.as_str(), initial) {
            Ok(line) => line,
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        held = None;
        if let Some(at) = tab.lock().unwrap().take() {
            if let Some(sitting) = sitting.as_mut() {
                sitting.toggle(&prompt, &line);
            }
            held = Some((line, at));
            continue;
        }
        let (result, said, printed) =
            datum_commands::render::json_echo::tapped(|| run_argv(argv(&line), &named, db, path));
        datum_commands::args::reset_duplicate_noop();
        match sitting.as_mut() {
            Some(sitting) if !printed => sitting.enter(&prompt, line, result, said),
            // Under what the base printed itself nothing can be redrawn.
            Some(sitting) => {
                sitting.shown.clear();
                say(result, &said);
            }
            None => say(result, &said),
        }
    }
}

/// Say on stderr what a `readline` line did (da-jt3k8), a line of its own:
/// what it wrote, `ACTION time field`, or FAILED and why.
fn say(result: Result<()>, said: &[datum_commands::render::json_echo::Said]) {
    match result {
        Err(e) => eprintln!("FAILED {e:#}"),
        Ok(()) if said.is_empty() => {}
        Ok(()) => {
            let said: Vec<&str> = said.iter().map(|s| s.said.as_str()).collect();
            eprintln!("{}", said.join(", "));
        }
    }
}

/// `readline` on a terminal (da-1lnm4): each line typed, on Enter, gives
/// its place to the entry it wrote, in the full view (what a command prints)
/// or the compact one ([`compact`]), and Tab turns the view over, the lines
/// on screen with it. A line that failed, or wrote nothing, stays as typed.
struct Sitting {
    compact: bool,
    /// The base's `-R` key, the compact view's one unlabeled value.
    remainder: Option<String>,
    shown: Vec<Shown>,
}

/// A line of the sitting, as it is on screen.
struct Shown {
    prompt: String,
    line: String,
    said: Vec<datum_commands::render::json_echo::Said>,
    error: Option<String>,
    /// The rows it fills now.
    rows: usize,
}

impl Shown {
    /// Its lines in a view: the line as typed, its error under it, where it
    /// failed or wrote nothing.
    fn lines(&self, compact: bool, remainder: Option<&str>) -> Vec<String> {
        use datum_commands::render::colors::dim;
        if self.error.is_some() || self.said.is_empty() {
            let typed = format!("{}{}", dim(&self.prompt), self.line);
            let error = self
                .error
                .iter()
                .flat_map(|e| e.lines().map(str::to_string));
            return std::iter::once(typed).chain(error).collect();
        }
        match compact {
            true => self
                .said
                .iter()
                .map(|s| compact_line(&stamp(&s.entry), &pairs(&s.entry), remainder))
                .collect(),
            false => self.said.iter().flat_map(|s| s.rows.clone()).collect(),
        }
    }
}

impl Sitting {
    /// The line typed at `prompt` ran: give its rows to what it wrote, or
    /// leave it, its error under it.
    fn enter(
        &mut self,
        prompt: &str,
        line: String,
        result: Result<()>,
        said: Vec<datum_commands::render::json_echo::Said>,
    ) {
        let (cols, _) = screen();
        let typed = rows(&format!("{prompt}{line}"), cols, true);
        let mut shown = Shown {
            prompt: prompt.to_string(),
            line,
            said,
            error: result.err().map(|e| format!("FAILED {e:#}")),
            rows: typed,
        };
        if let Some(error) = &shown.error {
            eprintln!("{error}");
            shown.rows += error.lines().map(|l| rows(l, cols, false)).sum::<usize>();
        } else if !shown.said.is_empty() {
            up(typed);
            shown.rows = self.draw(&shown);
        }
        self.shown.push(shown);
    }

    /// Tab, with `line` typed at `prompt`: the other view, and the lines
    /// wholly on screen drawn again in it. Which those are is counted by the
    /// rows each filled when drawn: a terminal resized since may have
    /// reflowed them, which nothing here can read.
    fn toggle(&mut self, prompt: &str, line: &str) {
        let (cols, height) = screen();
        let typed = rows(&format!("{prompt}{line}"), cols, true);
        let room = height.saturating_sub(1 + typed);
        let (mut from, mut above) = (self.shown.len(), 0);
        while from > 0 && above + self.shown[from - 1].rows <= room {
            from -= 1;
            above += self.shown[from].rows;
        }
        self.compact = !self.compact;
        up(above + typed);
        for i in from..self.shown.len() {
            self.shown[i].rows = self.draw(&self.shown[i]);
        }
    }

    /// Print `shown` in the view, returning the rows it fills.
    fn draw(&self, shown: &Shown) -> usize {
        use std::io::Write;
        let (cols, _) = screen();
        let lines = shown.lines(self.compact, self.remainder.as_deref());
        let mut out = std::io::stdout().lock();
        for line in &lines {
            let _ = writeln!(out, "{line}");
        }
        let _ = out.flush();
        // An item may hold several lines (a format string's newline).
        let each = lines.iter().flat_map(|l| l.split('\n'));
        each.map(|l| rows(l, cols, false)).sum()
    }
}

/// The terminal's width and height now: a sitting is live, and a resize
/// between its lines counts.
fn screen() -> (usize, usize) {
    use datum_commands::render::term_layout::{detect_cols, detect_rows};
    (detect_cols(), detect_rows())
}

/// Move the cursor up `n` rows to their start, and clear from there down.
fn up(n: usize) {
    use std::io::Write;
    if n > 0 {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "\x1b[{n}A\r\x1b[J");
        let _ = out.flush();
    }
}

/// The rows `text` fills on a terminal `cols` wide, its escapes no width,
/// a wide character that does not fit a row's end going to the next, as
/// rustyline lays a line out. `edited`: rustyline drew it, and a line that
/// fills its last row exactly is followed by the empty row it wraps to;
/// printed and ended by a newline, it is not.
fn rows(text: &str, cols: usize, edited: bool) -> usize {
    let cols = cols.max(1);
    let (mut row, mut col) = (0, 0);
    let mut buf = [0; 4];
    for c in datum_commands::render::strip_ansi(text).chars() {
        let width = datum_commands::render::visible_width(c.encode_utf8(&mut buf));
        col += width;
        if col > cols {
            row += 1;
            col = width;
        }
    }
    if edited && col == cols {
        row += 1;
    }
    row + 1
}

/// `HH:MM:SS` of the entry, in its own zone; a timeless one's of when it
/// was made, in this host's.
fn stamp(entry: &datum_core::entry::Entry) -> String {
    match &entry.at {
        Some(at) => at.local_time().format("%H:%M:%S").to_string(),
        None => {
            let here = datum_parse::DatumTime::now().local_time();
            let made = entry.create_time.with_timezone(here.offset());
            made.format("%H:%M:%S").to_string()
        }
    }
}

/// The entry's data, each key and its value as text, in the data's order.
fn pairs(entry: &datum_core::entry::Entry) -> Vec<(String, String)> {
    let data = entry.data.as_ref().and_then(|d| d.as_object());
    let pair = |(key, value): (&String, _)| {
        let value = datum_core::entry::shown_data_value(key, value);
        let text = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_string);
        (key.clone(), text)
    };
    data.into_iter().flatten().map(pair).collect()
}

/// The compact view of an entry (da-1lnm4): `HH:MM:SS | ` dim, then
/// `key: value` for each key, `|` between, the `remainder` key's value last
/// and unlabeled.
fn compact_line(stamp: &str, pairs: &[(String, String)], remainder: Option<&str>) -> String {
    let is_remainder = |key: &str| Some(key) == remainder;
    let labeled = pairs
        .iter()
        .filter(|(k, _)| !is_remainder(k))
        .map(|(k, v)| format!("{k}: {v}"));
    let last = pairs
        .iter()
        .filter(|(k, _)| is_remainder(k))
        .map(|(_, v)| v.clone());
    let parts: Vec<String> = labeled.chain(last).collect();
    let clock = datum_commands::render::colors::dim(&format!("{stamp} | "));
    format!("{clock}{}", parts.join(" | "))
}

/// `readline`'s line editor helper: the prompt, its clock, dim. Its width
/// is the plain prompt's, so a wrapped line's rows count true.
struct Dim;

impl rustyline::Helper for Dim {}
impl rustyline::completion::Completer for Dim {
    type Candidate = String;
}
impl rustyline::hint::Hinter for Dim {
    type Hint = String;
}
impl rustyline::validate::Validator for Dim {}
impl rustyline::highlight::Highlighter for Dim {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> std::borrow::Cow<'b, str> {
        std::borrow::Cow::Owned(datum_commands::render::colors::dim(prompt))
    }
}

/// Tab at `readline`'s prompt: neither a tab nor a completion, but the end
/// of the edit, with where the cursor was, for [`Sitting::toggle`].
struct Toggle(std::sync::Arc<std::sync::Mutex<Option<usize>>>);

impl rustyline::ConditionalEventHandler for Toggle {
    fn handle(
        &self,
        _: &rustyline::Event,
        _: rustyline::RepeatCount,
        _: bool,
        ctx: &rustyline::EventContext,
    ) -> Option<rustyline::Cmd> {
        // A byte offset into the line, as `str::split_at` takes it.
        *self.0.lock().unwrap() = Some(ctx.pos());
        Some(rustyline::Cmd::AcceptLine)
    }
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
                command: Command::Readline(args),
                ..
            })) => read_lines(&args, db, Some(path)),
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
        if let Some(e) = db.drain_spool().map_or_else(Some, |d| d.stuck) {
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

#[cfg(test)]
mod tests {
    use super::{compact_line, rows};

    fn pairs(kv: &[(&str, &str)]) -> Vec<(String, String)> {
        kv.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Compact with its escapes gone, whatever the test's color.
    fn compact(kv: &[(&str, &str)], remainder: Option<&str>) -> String {
        datum_commands::render::strip_ansi(&compact_line("12:10:06", &pairs(kv), remainder))
    }

    #[test]
    fn compact_puts_the_remainder_last_and_unlabeled() {
        let kv = [("line", "hello there"), ("mood", "calm")];
        assert_eq!(
            compact(&kv, Some("line")),
            "12:10:06 | mood: calm | hello there"
        );
    }

    #[test]
    fn compact_labels_several_keys_in_order() {
        let kv = [("a", "1"), ("b", "two"), ("line", "x"), ("c", "[1,2]")];
        assert_eq!(
            compact(&kv, Some("line")),
            "12:10:06 | a: 1 | b: two | c: [1,2] | x"
        );
    }

    #[test]
    fn compact_of_the_remainder_alone_is_the_journal_line() {
        assert_eq!(
            compact(&[("line", "hello there")], Some("line")),
            "12:10:06 | hello there"
        );
    }

    #[test]
    fn compact_of_an_empty_line_is_its_clock() {
        assert_eq!(compact(&[("line", "")], Some("line")), "12:10:06 | ");
    }

    #[test]
    fn compact_with_no_remainder_key_labels_every_key() {
        let kv = [("line", "hello"), ("mood", "calm")];
        assert_eq!(compact(&kv, None), "12:10:06 | line: hello | mood: calm");
    }

    #[test]
    fn rows_of_a_short_line_is_one() {
        assert_eq!(rows("12:10:06 | hello", 80, true), 1);
        assert_eq!(rows("", 80, true), 1);
        assert_eq!(rows("", 80, false), 1);
    }

    #[test]
    fn rows_of_an_exact_fill_counts_the_row_rustyline_wraps_to() {
        let full = "x".repeat(80);
        assert_eq!(rows(&full, 80, true), 2, "typed: the cursor wrapped below");
        assert_eq!(
            rows(&full, 80, false),
            1,
            "printed: the newline takes the wrap"
        );
    }

    #[test]
    fn rows_of_a_wrapped_line() {
        assert_eq!(rows(&"x".repeat(81), 80, true), 2);
        assert_eq!(rows(&"x".repeat(161), 80, false), 3);
    }

    #[test]
    fn rows_count_wide_characters_twice_and_escapes_not_at_all() {
        // Forty wide characters fill a row of 80; 39 and a half do not.
        assert_eq!(rows(&"界".repeat(40), 80, false), 1);
        assert_eq!(rows(&"界".repeat(41), 80, false), 2);
        // One column left at a row's end: the wide character goes on to the next.
        assert_eq!(rows(&format!("{}界", "x".repeat(79)), 80, false), 2);
        assert_eq!(rows("\x1b[2m12:10:06 | \x1b[0mhi", 14, false), 1);
    }
}
