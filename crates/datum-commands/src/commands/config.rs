//! `datum config` — inspect per-db and global configuration.
//!
//! Named `commands::config` to avoid colliding with the crate-level
//! `crate::config` module that holds XDG-path / registry helpers.

use anyhow::Result;

use crate::args::{ConfigCommand, ConfigShowArgs};
use crate::config;

pub fn config(sub: ConfigCommand, cli_db: Option<&str>) -> Result<()> {
    match sub {
        ConfigCommand::Show(args) => config_show(args, cli_db),
    }
}

fn config_show(args: ConfigShowArgs, cli_db: Option<&str>) -> Result<()> {
    let cfg = config::Config::load(cli_db)?;

    if args.json {
        let rows = cfg
            .rows()
            .into_iter()
            .map(|(k, v, s)| {
                (
                    k.to_string(),
                    serde_json::json!({
                        "value": v,
                        "source": s.as_str(),
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let out = serde_json::json!({
            "config": rows,
            "toml_path": cfg.toml_path.to_string_lossy(),
            "toml_loaded": cfg.toml_loaded,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let rows = cfg.rows();
    let key_w = rows
        .iter()
        .map(|(k, _, _)| k.len())
        .max()
        .unwrap_or(0)
        .max(3);
    let val_w = rows
        .iter()
        .map(|(_, v, _)| v.len())
        .max()
        .unwrap_or(0)
        .max(5);
    println!(
        "{:<key_w$}  {:<val_w$}  SOURCE",
        "KEY",
        "VALUE",
        key_w = key_w,
        val_w = val_w,
    );
    for (k, v, s) in rows {
        println!(
            "{:<key_w$}  {:<val_w$}  {}",
            k,
            v,
            s.as_str(),
            key_w = key_w,
            val_w = val_w,
        );
    }
    println!();
    println!(
        "toml: {} ({})",
        cfg.toml_path.display(),
        if cfg.toml_loaded {
            "loaded"
        } else {
            "not found"
        }
    );
    Ok(())
}
