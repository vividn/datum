-- da-3zq6g: what `datum check` found at its last run, and what changed
-- since, so a second run reads again only those. LOCAL and derived: never
-- synced. The reading is `datum-commands`' `commands/check/kept.rs`; the
-- triggers below are what keep it true, whichever path writes a row. Laid
-- down by the v73→v74 migration and after `schema.sql` at every open, so it
-- is a file of its own; a file it was never laid in (one a reader opened) is
-- checked whole.
--
-- The rows a write touched, by hid, field and `occur_time` (`''` for none;
-- an update that moves a row names where it was and where it is), and the
-- hid of every op that is or was applied, or is a chain record, its field
-- and time '': a waiting op changes no fold. Appended, not upserted, so a
-- write pays one insert; a row touched twice between checks is named twice,
-- and the next check prunes both. What changed since a check is the rows
-- past the `seq` it read through. AUTOINCREMENT: a `seq` is never reused,
-- even once a prune has emptied the log.
CREATE TABLE IF NOT EXISTS check_changes (
    seq   INTEGER PRIMARY KEY AUTOINCREMENT,
    hid   TEXT NOT NULL,
    field TEXT NOT NULL,
    at    TEXT NOT NULL
);

-- Each checker's last run: the datum and database it ran as (`stamp`), the
-- change it read through, and what it found, a JSON array of the hids it
-- named (for `integrity`, the fields). Another stamp is a run from scratch.
CREATE TABLE IF NOT EXISTS check_kept (
    checker TEXT PRIMARY KEY,
    stamp   TEXT NOT NULL,
    through INTEGER NOT NULL,
    kept    TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS check_changes_insert AFTER INSERT ON entries BEGIN
    INSERT INTO check_changes (hid, field, at)
        VALUES (new.hid, new.field, coalesce(new.occur_time, ''));
END;
CREATE TRIGGER IF NOT EXISTS check_changes_update AFTER UPDATE ON entries BEGIN
    INSERT INTO check_changes (hid, field, at)
        SELECT old.hid, old.field, coalesce(old.occur_time, '')
        UNION SELECT new.hid, new.field, coalesce(new.occur_time, '');
END;
CREATE TRIGGER IF NOT EXISTS check_changes_delete AFTER DELETE ON entries BEGIN
    INSERT INTO check_changes (hid, field, at)
        VALUES (old.hid, old.field, coalesce(old.occur_time, ''));
END;
CREATE TRIGGER IF NOT EXISTS check_changes_op_insert AFTER INSERT ON ops
    WHEN new.applied = 1 OR new.hid = '~chain' BEGIN
    INSERT INTO check_changes (hid, field, at) VALUES (new.hid, '', '');
END;
CREATE TRIGGER IF NOT EXISTS check_changes_op_update AFTER UPDATE ON ops
    WHEN old.applied IS NOT new.applied OR old.body IS NOT new.body BEGIN
    INSERT INTO check_changes (hid, field, at)
        SELECT old.hid, '', '' UNION SELECT new.hid, '', '';
END;
CREATE TRIGGER IF NOT EXISTS check_changes_op_delete AFTER DELETE ON ops
    WHEN old.applied = 1 OR old.hid = '~chain' BEGIN
    INSERT INTO check_changes (hid, field, at) VALUES (old.hid, '', '');
END;

-- da-3zq6g: the live rows a `+=` / `-=` read a base for, which `datum check
-- keys` replays (`Db::key_drift`): few, and found without reading the rest.
CREATE INDEX IF NOT EXISTS entries_asserted_base
    ON entries (field)
    WHERE deleted_at IS NULL AND superseded_by IS NULL
      AND json_type(extra_metadata, '$.asserted_base') = 'object';
