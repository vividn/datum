mod args;
mod check;
mod cmd;
mod config;
mod output;

use anyhow::Result;
use args::Cli;
use clap::Parser;

fn main() -> Result<()> {
    let cli = Cli::parse_with_alias_resolution()?;
    let db_path = config::db_path()?;
    let db = datum_core::db::Db::open(&db_path)?;
    cmd::dispatch(cli, &db)
}
