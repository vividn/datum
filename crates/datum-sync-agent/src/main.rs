//! `datum-sync-agent` binary entry — a thin argv wrapper around
//! [`datum_sync_agent::agent::run`].
//!
//! The runtime itself lives in the library so `datum sync agent` runs
//! exactly the same daemon from the `datum` binary; the installed
//! service unit execs that, not this (da-ge3e).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use datum_sync_agent::agent::{self, Options};

fn parse_args() -> Result<Options> {
    let mut out = Options::default();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pid-file" => {
                let v = it.next().context("--pid-file requires a value")?;
                out.pid_file = Some(PathBuf::from(v));
            }
            s if s.starts_with("--pid-file=") => {
                out.pid_file = Some(PathBuf::from(&s["--pid-file=".len()..]));
            }
            "--verbose" | "-v" => out.verbose = true,
            "--one-shot" => out.one_shot = true,
            "--help" | "-h" => {
                println!(
                    "datum-sync-agent — background daemon that keeps datum DBs in sync\n\
                     \n\
                     USAGE: datum-sync-agent [--pid-file PATH] [-v | --verbose] [--one-shot]\n\
                     \n\
                     The same daemon is reachable as `datum sync agent`, which is what\n\
                     `datum sync install-daemon` puts in the service unit."
                );
                std::process::exit(0);
            }
            other => bail!("unexpected argument: {other}"),
        }
    }
    Ok(out)
}

fn main() -> Result<()> {
    agent::run(parse_args()?)
}
