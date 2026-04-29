//! User aliases: `lookup_alias`, `delete_alias`, `save_alias`.
//! Derived aliases are managed through the `field_specs` save path — see
//! [`super::Db::save_field_spec`].

use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::field_spec::Alias;

use super::Db;

impl Db {
    /// Look up a single alias by name. Returns None if not found.
    pub fn lookup_alias(&self, name: &str) -> Result<Option<Alias>> {
        let result: Option<String> = self.conn
            .query_row(
                "SELECT alias_json FROM aliases WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Delete a user-created alias. Fails silently if it doesn't exist.
    /// Does NOT delete derived aliases (those are managed by their owning spec).
    pub fn delete_alias(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aliases WHERE name = ?1 AND derived_from_field IS NULL",
            params![name],
        )?;
        Ok(())
    }

    /// Save a user-created alias (derived_from_field = NULL).
    pub fn save_alias(&self, alias: &Alias) -> Result<()> {
        let json = serde_json::to_string(alias)?;
        self.conn.execute(
            "INSERT INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, NULL)
             ON CONFLICT(name) DO UPDATE SET alias_json = excluded.alias_json,
                                             derived_from_field = NULL",
            params![alias.name, json],
        )?;
        Ok(())
    }
}
