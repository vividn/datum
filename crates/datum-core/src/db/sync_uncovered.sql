-- da-8hfta: the entries no op covers, the baselines a sync signs, kept by
-- triggers so a session finds them without reading every entry. It may hold
-- more than those (a row a REPLACE took, which fires no delete trigger), so
-- a reader joins it to `entries` and asks `ops` again (`UNCOVERED_SQL` in
-- `db/sync.rs`); it never holds fewer. Laid by the v78→v79 migration, which
-- fills it, and after `schema.sql` at every open; a migration that rebuilds
-- `entries` or `ops` drops the triggers and fills it again. LOCAL and
-- derived, never synced. `datum check sync`'s `sync_no_op` reads every
-- entry, and is its control.
CREATE TABLE IF NOT EXISTS sync_uncovered (
    hid TEXT NOT NULL PRIMARY KEY
);
CREATE TRIGGER IF NOT EXISTS sync_uncovered_insert AFTER INSERT ON entries
    WHEN NOT EXISTS (SELECT 1 FROM ops WHERE hid = NEW.hid) BEGIN
    INSERT OR IGNORE INTO sync_uncovered (hid) VALUES (NEW.hid);
END;
CREATE TRIGGER IF NOT EXISTS sync_uncovered_rehid AFTER UPDATE OF hid ON entries
    WHEN NOT EXISTS (SELECT 1 FROM ops WHERE hid = NEW.hid) BEGIN
    INSERT OR IGNORE INTO sync_uncovered (hid) VALUES (NEW.hid);
END;
CREATE TRIGGER IF NOT EXISTS sync_uncovered_delete AFTER DELETE ON entries BEGIN
    DELETE FROM sync_uncovered WHERE hid = OLD.hid;
END;
CREATE TRIGGER IF NOT EXISTS sync_uncovered_op_insert AFTER INSERT ON ops BEGIN
    DELETE FROM sync_uncovered WHERE hid = NEW.hid;
END;
-- An op gone, or a purge's stub taking another hid: its entry, if it has no
-- other op.
CREATE TRIGGER IF NOT EXISTS sync_uncovered_op_delete AFTER DELETE ON ops
    WHEN EXISTS (SELECT 1 FROM entries WHERE hid = OLD.hid)
     AND NOT EXISTS (SELECT 1 FROM ops WHERE hid = OLD.hid) BEGIN
    INSERT OR IGNORE INTO sync_uncovered (hid) VALUES (OLD.hid);
END;
CREATE TRIGGER IF NOT EXISTS sync_uncovered_op_rehid AFTER UPDATE OF hid ON ops BEGIN
    DELETE FROM sync_uncovered WHERE hid = NEW.hid;
    INSERT OR IGNORE INTO sync_uncovered (hid)
        SELECT OLD.hid
         WHERE EXISTS (SELECT 1 FROM entries WHERE hid = OLD.hid)
           AND NOT EXISTS (SELECT 1 FROM ops WHERE hid = OLD.hid);
END;
