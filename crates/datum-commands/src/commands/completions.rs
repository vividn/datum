//! `datum completions` — the shell scripts that ask `datum __complete`
//! ([`super::complete`]), and putting them where each shell loads them by
//! itself (da-bu09u).
//!
//! Nothing here reads the process environment except
//! [`ShellEnv::from_process`]: the paths come in as a [`ShellEnv`], so a
//! test names its own home rather than setting one on the process (#303).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use datum_parse::cli::{CompletionsCommand, Shell};

/// The one-time tip an interactive run prints while no script is installed.
pub const TIP: &str = "tip: tab completion is available: datum completions install";

/// Proof a file is ours before `uninstall` removes it.
const MARK: &str = "datum __complete";

/// `shell`'s script, asking `exe` when it is given (an installed script
/// asks the binary that installed it, da-ejoes), else `datum`.
pub fn script(shell: Shell, exe: Option<&Path>) -> String {
    let template = match shell {
        Shell::Zsh => include_str!("../../completions/datum.zsh"),
        Shell::Bash => include_str!("../../completions/datum.bash"),
        Shell::Fish => include_str!("../../completions/datum.fish"),
    };
    // Single-quoted, which reads the same in all three shells.
    let exe = exe.map_or(String::new(), |p| p.display().to_string());
    template.replace("@DATUM_EXE@", &format!("'{}'", exe.replace('\'', r"'\''")))
}

/// Where a shell's autoloaded completions live, for this user.
#[derive(Debug, Clone)]
pub struct ShellEnv {
    /// `$SHELL`'s shell, when it is one datum has a script for.
    pub shell: Option<Shell>,
    pub home: PathBuf,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    /// zsh's `$fpath`, as far as a child process can see it. Empty from
    /// [`Self::from_process`]: asking costs a zsh, so only the zsh paths ask
    /// ([`Self::with_fpath`]).
    pub fpath: Vec<PathBuf>,
    /// The binary running, which an installed script asks (da-ejoes).
    /// `None` from [`Self::from_process`]: only install asks
    /// ([`Self::with_exe`]).
    pub exe: Option<PathBuf>,
}

impl ShellEnv {
    pub fn from_process() -> Option<ShellEnv> {
        let var = |k| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let shell = var("SHELL")
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .and_then(|n| Shell::from_name(&n));
        Some(ShellEnv {
            shell,
            home: var("HOME")?,
            xdg_data_home: var("XDG_DATA_HOME"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            fpath: Vec::new(),
            exe: None,
        })
    }

    /// zsh's `$fpath` is not exported, so ask a zsh for it: `FPATH` when
    /// the caller exported it, else the one a bare `zsh -f` starts with,
    /// which is where a package manager's site-functions live. Only zsh
    /// needs it, and only once per install or tip.
    fn with_fpath(mut self) -> ShellEnv {
        let listed = std::env::var("FPATH")
            .ok()
            .filter(|f| !f.is_empty())
            .or_else(|| {
                let out = std::process::Command::new("zsh")
                    .args(["-fc", "print -rl -- $fpath"])
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()?;
                Some(String::from_utf8_lossy(&out.stdout).replace('\n', ":"))
            });
        self.fpath = listed
            .unwrap_or_default()
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .collect();
        self
    }

    fn with_exe(mut self) -> ShellEnv {
        self.exe = std::env::current_exe().ok();
        self
    }

    fn data_home(&self) -> PathBuf {
        self.xdg_data_home
            .clone()
            .unwrap_or_else(|| self.home.join(".local/share"))
    }

    /// The zsh directory used when nothing on `$fpath` is writable.
    fn zsh_fallback(&self) -> PathBuf {
        self.data_home().join("zsh/site-functions")
    }

    /// The places a shell's script may be, in the order install tries them.
    fn candidates(&self, shell: Shell) -> Vec<PathBuf> {
        match shell {
            Shell::Fish => {
                let config = self
                    .xdg_config_home
                    .clone()
                    .unwrap_or_else(|| self.home.join(".config"));
                vec![config.join("fish/completions/datum.fish")]
            }
            Shell::Bash => vec![self.data_home().join("bash-completion/completions/datum")],
            Shell::Zsh => self
                .fpath
                .iter()
                .filter(|d| d.is_dir())
                .chain(std::iter::once(&self.zsh_fallback()))
                .map(|d| d.join("_datum"))
                .collect(),
        }
    }
}

pub fn run(cmd: &CompletionsCommand) -> Result<()> {
    let env = || ShellEnv::from_process().context("HOME is not set");
    match *cmd {
        CompletionsCommand::Script(shell) => print!("{}", script(shell, None)),
        CompletionsCommand::Install(shell) => {
            let env = env()?;
            let shell = pick(shell, &env)?;
            print!("{}", install(shell, &env.with_fpath().with_exe())?);
        }
        CompletionsCommand::Uninstall(shell) => {
            let env = env()?;
            let shell = pick(shell, &env)?;
            print!("{}", uninstall(shell, &env.with_fpath())?);
        }
    }
    Ok(())
}

fn pick(shell: Option<Shell>, env: &ShellEnv) -> Result<Shell> {
    shell
        .or(env.shell)
        .context("cannot tell the shell from $SHELL: name it with --shell zsh|bash|fish")
}

/// Write the script where `shell` loads it, and say where. Idempotent: a
/// second run overwrites the same file.
pub fn install(shell: Shell, env: &ShellEnv) -> Result<String> {
    let mut last_err = None;
    for path in env.candidates(shell) {
        let before = std::fs::read_to_string(&path).ok();
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, script(shell, env.exe.as_deref())));
        match written {
            Ok(()) => return Ok(installed_message(shell, env, &path, before)),
            Err(e) => last_err = Some((path, e)),
        }
    }
    let (path, e) = last_err.expect("every shell has a candidate");
    Err(e).with_context(|| format!("writing {}", path.display()))
}

/// How long [`completes`] waits for a shell's rc files.
const PROBE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Whether a new interactive `shell` completes `datum` with our `_datum`,
/// asked of one that reads the user's own rc files (da-ei1vj). An rc file
/// can print, prompt or hang, so its output goes nowhere and the wait is
/// bounded; `+m` turns job control off in zsh and bash alike, so the shell
/// never takes the terminal, which a killed one would never give back; yes
/// is exit 7, so an rc file that exits or execs another shell is no. Fish
/// loads its completions directory by itself.
fn completes(shell: Shell, env: &ShellEnv) -> bool {
    use std::process::{Command, Stdio};
    let test = match shell {
        Shell::Fish => return true,
        Shell::Zsh => "[[ $_comps[datum] == _datum ]] && exit 7",
        // bash-completion loads a script on the first TAB: load it now.
        Shell::Bash => "complete -p datum || _completion_loader datum; [[ $(complete -p datum) == *\" _datum \"* ]] && exit 7",
    };
    let mut cmd = Command::new(shell.name());
    cmd.args(["+m", "-ic", test])
        .env("HOME", &env.home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(d) = &env.xdg_data_home {
        cmd.env("XDG_DATA_HOME", d);
    }
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let deadline = std::time::Instant::now() + PROBE_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code() == Some(7),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn installed_message(shell: Shell, env: &ShellEnv, path: &Path, before: Option<String>) -> String {
    let wrote = format!("wrote {} completion to {}\n", shell.name(), path.display());
    if completes(shell, env) {
        let said = match before {
            None => wrote,
            Some(b) if b != script(shell, env.exe.as_deref()) => {
                format!("updated {}\n", path.display())
            }
            Some(_) => String::new(),
        };
        return format!("{said}tab completion: already set up ({})\n", shell.name());
    }
    let mut msg = wrote;
    let dir = path.parent().unwrap_or(path);
    if shell == Shell::Zsh && !env.fpath.iter().any(|d| d == dir) {
        msg.push_str(&format!(
            "add this line to ~/.zshrc, before compinit:\n  fpath=({} $fpath)\n\
             with oh-my-zsh, put it before `source $ZSH/oh-my-zsh.sh` (that line runs compinit)\n",
            dir.display()
        ));
    }
    if shell == Shell::Bash {
        msg.push_str("(the bash-completion package loads it on the first TAB)\n");
    }
    msg.push_str("start a new shell to use it\n");
    msg
}

/// Remove the script `install` wrote, and say which.
pub fn uninstall(shell: Shell, env: &ShellEnv) -> Result<String> {
    let Some(path) = installed(shell, env) else {
        return Ok(format!("no {} completion installed\n", shell.name()));
    };
    std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    Ok(format!("removed {}\n", path.display()))
}

/// The installed script, if there is one: the first of the places install
/// writes that holds a file of ours.
pub fn installed(shell: Shell, env: &ShellEnv) -> Option<PathBuf> {
    env.candidates(shell)
        .into_iter()
        .find(|p| std::fs::read_to_string(p).is_ok_and(|body| body.contains(MARK)))
}

/// Whether a run may print the tip at all: only for a person at a
/// terminal — stderr a TTY, no JSON asked for, not completion's own
/// commands — and never under `DATUM_NO_TIPS=1` (`no_tips`).
pub fn tip_wanted(argv: &[String], no_tips: bool, stderr_tty: bool) -> bool {
    let json = argv.iter().any(|a| a == "--json" || a == "--show=json")
        || argv.windows(2).any(|w| w[0] == "--show" && w[1] == "json");
    let completing = super::complete::position(argv).is_some()
        || argv.get(1).is_some_and(|a| a == "completions");
    !no_tips && stderr_tty && !json && !completing
}

/// The tip, the first time it is due on this machine, or `None`.
///
/// Due when `$SHELL` is a shell datum has a script for and none is
/// installed. `marker` records that the question was settled — the tip
/// shown, or a script found — so it is asked once, whether or not the user
/// installs. The caller decides whether the run is interactive.
pub fn tip(env: ShellEnv, marker: &Path) -> Option<&'static str> {
    if marker.exists() {
        return None;
    }
    let shell = env.shell?;
    let env = if shell == Shell::Zsh && env.fpath.is_empty() {
        env.with_fpath()
    } else {
        env
    };
    let due = installed(shell, &env).is_none();
    let _ = marker
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(marker, ""));
    due.then_some(TIP)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home of its own, passed in — never set on the process (#303).
    fn env(tmp: &Path, shell: Shell, fpath: Vec<PathBuf>) -> ShellEnv {
        ShellEnv {
            shell: Some(shell),
            home: tmp.join("home"),
            xdg_data_home: Some(tmp.join("data")),
            xdg_config_home: None,
            fpath,
            exe: None,
        }
    }

    #[test]
    fn install_writes_where_each_shell_loads_it_and_uninstall_removes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let writable = t.join("fpath-dir");
        std::fs::create_dir_all(&writable).unwrap();
        let cases = [
            (
                Shell::Fish,
                vec![],
                t.join("home/.config/fish/completions/datum.fish"),
            ),
            (
                Shell::Bash,
                vec![],
                t.join("data/bash-completion/completions/datum"),
            ),
            (Shell::Zsh, vec![writable.clone()], writable.join("_datum")),
            (Shell::Zsh, vec![], t.join("data/zsh/site-functions/_datum")),
        ];
        for (shell, fpath, want) in cases {
            let env = env(t, shell, fpath);
            let said = install(shell, &env).unwrap();
            assert!(said.contains(&want.display().to_string()), "{said}");
            assert_eq!(std::fs::read_to_string(&want).unwrap(), script(shell, None));
            // Idempotent: again, same file, same bytes.
            install(shell, &env).unwrap();
            assert_eq!(std::fs::read_to_string(&want).unwrap(), script(shell, None));
            assert_eq!(installed(shell, &env), Some(want.clone()));
            uninstall(shell, &env).unwrap();
            assert!(!want.exists(), "{want:?}");
        }
    }

    #[test]
    fn zsh_off_fpath_says_the_line_to_add() {
        let tmp = tempfile::tempdir().unwrap();
        let said = install(Shell::Zsh, &env(tmp.path(), Shell::Zsh, vec![])).unwrap();
        assert!(said.contains("fpath=("), "{said}");
        assert!(said.contains("oh-my-zsh.sh"), "{said}");
        assert!(!said.contains("zcompdump"), "{said}");
    }

    /// `env` with an rc file in its home, and the fpath dir install writes to.
    fn with_rc(tmp: &Path, shell: Shell, rc: &str, body: &str) -> ShellEnv {
        let dir = tmp.join("fn");
        std::fs::create_dir_all(tmp.join("home")).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(tmp.join("home").join(rc), body).unwrap();
        env(tmp, shell, vec![dir])
    }

    fn has(shell: &str) -> bool {
        let found = std::process::Command::new(shell)
            .args(["-c", "true"])
            .status()
            .is_ok();
        if !found {
            eprintln!("skipped: no {shell}");
        }
        found
    }

    #[test]
    fn zsh_that_already_completes_gets_one_line_and_no_advice() {
        if !has("zsh") {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let rc = format!(
            "fpath=({} $fpath)\nautoload -Uz compinit && compinit -u\n",
            t.join("fn").display()
        );
        let env = with_rc(t, Shell::Zsh, ".zshrc", &rc);
        let path = t.join("fn/_datum");
        let first = install(Shell::Zsh, &env).unwrap();
        assert!(first.contains("already set up (zsh)"), "{first}");
        assert!(!first.contains("fpath=("), "{first}");
        assert!(!first.contains("new shell"), "{first}");

        let again = install(Shell::Zsh, &env).unwrap();
        assert!(!again.contains("_datum"), "unchanged: {again}");
        assert!(again.contains("already set up (zsh)"), "{again}");

        std::fs::write(&path, "#compdef datum\n").unwrap();
        let updated = install(Shell::Zsh, &env).unwrap();
        assert!(
            updated.contains(&format!("updated {}", path.display())),
            "{updated}"
        );

        // Another completion registered for `datum` is not ours.
        std::fs::write(t.join("home/.zshrc"), format!("{rc}compdef _files datum\n")).unwrap();
        let other = install(Shell::Zsh, &env).unwrap();
        assert!(!other.contains("already set up"), "{other}");

        // The same zsh without the fpath line: the advice.
        std::fs::write(
            t.join("home/.zshrc"),
            "autoload -Uz compinit && compinit -u\n",
        )
        .unwrap();
        let env = ShellEnv {
            fpath: vec![],
            ..env
        };
        let advised = install(Shell::Zsh, &env).unwrap();
        assert!(advised.contains("fpath=("), "{advised}");
        assert!(!advised.contains("already set up"), "{advised}");
    }

    #[test]
    fn bash_that_already_completes_gets_one_line() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let file = t.join("data/bash-completion/completions/datum");
        let env = with_rc(t, Shell::Bash, ".bashrc", "complete -F _other datum\n");
        let advised = install(Shell::Bash, &env).unwrap();
        assert!(advised.contains("new shell"), "not ours: {advised}");
        std::fs::write(
            t.join("home/.bashrc"),
            format!("source {}\n", file.display()),
        )
        .unwrap();
        let said = install(Shell::Bash, &env).unwrap();
        assert!(said.contains("already set up (bash)"), "{said}");
        assert!(!said.contains("new shell"), "{said}");
    }

    #[test]
    fn an_rc_file_that_hangs_costs_the_timeout_and_gets_the_advice() {
        let tmp = tempfile::tempdir().unwrap();
        let env = with_rc(tmp.path(), Shell::Bash, ".bashrc", "sleep 30\n");
        let start = std::time::Instant::now();
        let said = install(Shell::Bash, &env).unwrap();
        assert!(start.elapsed() < PROBE_WAIT * 2, "{:?}", start.elapsed());
        assert!(said.contains("new shell"), "{said}");
    }

    #[test]
    fn uninstall_leaves_a_file_that_is_not_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path(), Shell::Bash, vec![]);
        let path = tmp.path().join("data/bash-completion/completions/datum");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "someone else's").unwrap();
        uninstall(Shell::Bash, &env).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn the_tip_shows_once_and_not_when_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env(tmp.path(), Shell::Fish, vec![]);
        let marker = tmp.path().join("state/tip");
        assert_eq!(tip(env.clone(), &marker), Some(TIP));
        assert_eq!(tip(env.clone(), &marker), None, "once per machine");

        let marker = tmp.path().join("state/tip2");
        install(Shell::Fish, &env).unwrap();
        assert_eq!(tip(env.clone(), &marker), None, "a script is installed");

        let unknown = ShellEnv { shell: None, ..env };
        assert_eq!(tip(unknown, &tmp.path().join("state/tip3")), None);
    }

    #[test]
    fn the_tip_is_only_for_a_person_at_a_terminal() {
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(tip_wanted(&argv(&["datum", "tail"]), false, true));
        assert!(
            !tip_wanted(&argv(&["datum", "tail"]), true, true),
            "DATUM_NO_TIPS"
        );
        assert!(
            !tip_wanted(&argv(&["datum", "tail"]), false, false),
            "not a TTY"
        );
        assert!(!tip_wanted(
            &argv(&["datum", "tail", "--json"]),
            false,
            true
        ));
        assert!(!tip_wanted(
            &argv(&["datum", "now", "--show", "json"]),
            false,
            true
        ));
        assert!(!tip_wanted(
            &argv(&["datum", "__complete", "1"]),
            false,
            true
        ));
        assert!(!tip_wanted(
            &argv(&["datum", "completions", "zsh"]),
            false,
            true
        ));
    }
}
