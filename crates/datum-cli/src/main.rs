fn main() {
    // Rust ignores SIGPIPE, so a closed stdout makes `println!` panic and the
    // process exits 101 with a panic message (da-9p3o). End quietly instead,
    // the way a unix tool does. Only stdout's broken pipe: a socket's EPIPE
    // is still an error, and every other panic keeps its report.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload_as_str().is_some_and(|m| {
            m.starts_with("failed printing to stdout") && m.contains("Broken pipe")
        }) {
            std::process::exit(datum_cli::EXIT_BROKEN_PIPE);
        }
        report(info)
    }));
    let argv: Vec<String> = std::env::args().collect();
    let result = datum_cli::run(argv.clone());
    #[cfg(not(target_family = "wasm"))]
    completion_tip(&argv);
    let code = datum_cli::exit_code(result);
    if code != 0 {
        std::process::exit(code);
    }
}

/// The one-time «tab completion is available» tip (da-bu09u).
#[cfg(not(target_family = "wasm"))]
fn completion_tip(argv: &[String]) {
    use datum_commands::commands::completions::{tip, tip_wanted, ShellEnv};
    use std::io::IsTerminal;
    let no_tips = std::env::var("DATUM_NO_TIPS").is_ok_and(|v| v == "1");
    if !tip_wanted(argv, no_tips, std::io::stderr().is_terminal()) {
        return;
    }
    let marker = datum_commands::config::data_dir_path().map(|d| d.join("completion-tip-shown"));
    if let (Some(env), Some(marker)) = (ShellEnv::from_process(), marker) {
        if let Some(tip) = tip(env, &marker) {
            eprintln!("{tip}");
        }
    }
}
