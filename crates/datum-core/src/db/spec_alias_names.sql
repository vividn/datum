-- da-ulu8f: the name of every derived alias a `spec` row carries, so the
-- word a command starts with is looked up by name (`Db::lookup_alias`)
-- and not by reading every spec. LOCAL and derived, like an index: every
-- `spec` row, live or not (the reader keeps to the live ones), kept true by
-- the triggers below whichever path writes a row. Laid down after
-- `schema.sql` at every open, and filled by the v62→v63 migration; a later
-- migration that rebuilds `entries` fills it again (`fill_spec_alias_names`).
CREATE TABLE IF NOT EXISTS spec_alias_names (
    name TEXT NOT NULL,
    spec INTEGER NOT NULL, -- the `spec` row's rowid
    PRIMARY KEY (name, spec)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS spec_alias_names_spec ON spec_alias_names (spec);

CREATE TRIGGER IF NOT EXISTS spec_alias_names_insert AFTER INSERT ON entries
WHEN new.field = 'spec' BEGIN
    INSERT OR IGNORE INTO spec_alias_names
        SELECT json_extract(new.data, fullkey || '.state') AS name, new.rowid
          FROM json_each(CASE WHEN json_valid(new.data) THEN new.data END, '$.derived_aliases')
         WHERE name IS NOT NULL;
END;

CREATE TRIGGER IF NOT EXISTS spec_alias_names_update AFTER UPDATE ON entries
WHEN old.field = 'spec' OR new.field = 'spec' BEGIN
    DELETE FROM spec_alias_names WHERE spec = old.rowid;
    INSERT OR IGNORE INTO spec_alias_names
        SELECT json_extract(new.data, fullkey || '.state') AS name, new.rowid
          FROM json_each(CASE WHEN new.field = 'spec' AND json_valid(new.data) THEN new.data END,
                         '$.derived_aliases')
         WHERE name IS NOT NULL;
END;

CREATE TRIGGER IF NOT EXISTS spec_alias_names_delete AFTER DELETE ON entries
WHEN old.field = 'spec' BEGIN
    DELETE FROM spec_alias_names WHERE spec = old.rowid;
END;
