use anyhow::Result;
use datum_cli::{
    args::{Cli, Command, InfoArgs},
    cmd, config,
};

fn main() -> Result<()> {
    let cli = Cli::parse_with_alias_resolution()?;

    // `info --all` and `info <target>` inspect other databases; don't
    // force-create the default DB just to satisfy dispatch plumbing.
    if matches!(
        &cli.command,
        Command::Info(InfoArgs { all: true, .. })
            | Command::Info(InfoArgs { target: Some(_), .. })
    ) && cli.db.is_none()
    {
        let placeholder = datum_core::db::Db::open_in_memory()?;
        return cmd::dispatch(cli, &placeholder);
    }

    let resolved = config::resolve_db(cli.db.as_deref())?;

    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }

    let db = datum_core::db::Db::open(&resolved.path.to_string_lossy())?;

    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }

    cmd::dispatch(cli, &db)
}
