use datum_core::error::DatumError;

/// Exit code for a duplicate / "already have this" outcome. Matches datum v2
/// (`reference/datum-v2/src/index.ts`, `process.exit(11)` on `DocExistsError`)
/// so idempotent loggers can treat a re-log as a benign skip:
///
/// ```sh
/// datum occur coffee ; [ $? -eq 11 ] && echo "already logged, skipping"
/// ```
///
/// Fires for both flavours of duplicate:
///   * a content-identical re-log (deduped to `EXISTS`, surfaced via the
///     `took_duplicate_noop` side-signal — a *successful* no-op, not an error)
///   * a slug collision with conflicting data (a `DatumError::Duplicate` error)
///
/// Kept to this single distinct code — exit-code sprawl is its own footgun.
const EXIT_DUPLICATE: i32 = 11;

/// Exit code when the reader of stdout goes away (`datum tail | head -1`):
/// 128 + SIGPIPE, what a shell reports for any unix tool killed that way.
const EXIT_BROKEN_PIPE: i32 = 141;

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
            std::process::exit(EXIT_BROKEN_PIPE);
        }
        report(info)
    }));
    match datum_cli::run(std::env::args().collect()) {
        Ok(()) => {
            if datum_cli::args::took_duplicate_noop() {
                std::process::exit(EXIT_DUPLICATE);
            }
        }
        Err(e) => {
            // Mirror anyhow's default `main`: print the full error chain.
            eprintln!("Error: {e:?}");
            std::process::exit(exit_code(&e));
        }
    }
}

/// Map a hard error to its exit code: a `Duplicate` (slug collision) gets the
/// distinct, scriptable code; everything else is a generic failure. Walks the
/// whole source chain so a wrapped (`.context()`'d) `Duplicate` still maps.
fn exit_code(e: &anyhow::Error) -> i32 {
    let is_duplicate = e.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<DatumError>(),
            Some(DatumError::Duplicate(_))
        )
    });
    if is_duplicate {
        EXIT_DUPLICATE
    } else {
        1
    }
}
