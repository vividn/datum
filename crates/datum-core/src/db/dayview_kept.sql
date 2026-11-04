-- da-7z5i9: a past day's picture as `dayview --digests` hashed it, so a check
-- of a day no write has reached since answers without drawing it. LOCAL and
-- derived: never synced, read by `datum-commands`' `commands/dayview.rs`, laid
-- after `schema.sql` at every open. The triggers below are what keep it true,
-- whichever path writes a row.
--
-- `key` is the SHA-256 of the build, the host zone and the day's flags, so
-- another build's or zone's picture is never read; `end_ms` is the end of the
-- day's window, and `reach` each field whose drawing reads a row after it, to
-- the last instant it reads: a block running past midnight names where it
-- ends (`data-to`). A write of a field reaches a day when its leading edge is
-- at or before that field's reach, `end_ms` for every other field: a row
-- before the day carries its state in.
CREATE TABLE IF NOT EXISTS dayview_kept (
    key    TEXT NOT NULL UNIQUE,
    end_ms INTEGER NOT NULL,
    reach  TEXT NOT NULL,
    digest TEXT NOT NULL
);

-- What the writes since the last keeping touched: per field and `dur`, the
-- earliest `at` written or removed, as `strftime`'s `…T…:SS.SSSZ`, which
-- compares as time (`''`: one that reads as none, before every day). SQL cannot
-- subtract an ISO duration, so the leading edge is derived when it is read. A
-- field of `''` is a row where the database's zone changes, which moves the
-- cut of the days around it. A meta row is at the epoch, before every day, so
-- it reaches each; a spec forgets every day there and then whatever its `at`.
CREATE TABLE IF NOT EXISTS dayview_kept_dirty (
    field TEXT NOT NULL,
    dur   TEXT NOT NULL,
    at    TEXT NOT NULL,
    PRIMARY KEY (field, dur)
) WITHOUT ROWID;

CREATE TRIGGER IF NOT EXISTS dayview_kept_insert AFTER INSERT ON entries
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    DELETE FROM dayview_kept WHERE new.field = 'spec';
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT new.field, coalesce(new.dur, ''),
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', new.at), '')
         WHERE new.at IS NOT NULL
        UNION ALL
        SELECT '', '', coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', new.at), '')
         WHERE new.at_tz IS NOT NULL AND new.deleted_at IS NULL
           AND new.superseded_by IS NULL
           AND new.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
           AND new.at_tz IS NOT (
               SELECT at_tz FROM entries
                WHERE at_tz IS NOT NULL AND deleted_at IS NULL
                  AND superseded_by IS NULL
                  AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                  AND at < new.at AND rowid <> new.rowid
                ORDER BY at DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS dayview_kept_update AFTER UPDATE ON entries
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    DELETE FROM dayview_kept WHERE 'spec' IN (old.field, new.field);
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT old.field, coalesce(old.dur, ''),
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', old.at), '')
         WHERE old.at IS NOT NULL
        UNION ALL
        SELECT new.field, coalesce(new.dur, ''),
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', new.at), '')
         WHERE new.at IS NOT NULL
        UNION ALL
        SELECT '', '', coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', old.at), '')
         WHERE old.at_tz IS NOT NULL AND old.deleted_at IS NULL
           AND old.superseded_by IS NULL
           AND old.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
           AND old.at_tz IS NOT (
               SELECT at_tz FROM entries
                WHERE at_tz IS NOT NULL AND deleted_at IS NULL
                  AND superseded_by IS NULL
                  AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                  AND at < old.at AND rowid <> old.rowid
                ORDER BY at DESC LIMIT 1)
        UNION ALL
        SELECT '', '', coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', new.at), '')
         WHERE new.at_tz IS NOT NULL AND new.deleted_at IS NULL
           AND new.superseded_by IS NULL
           AND new.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
           AND new.at_tz IS NOT (
               SELECT at_tz FROM entries
                WHERE at_tz IS NOT NULL AND deleted_at IS NULL
                  AND superseded_by IS NULL
                  AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                  AND at < new.at AND rowid <> new.rowid
                ORDER BY at DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS dayview_kept_delete AFTER DELETE ON entries
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    DELETE FROM dayview_kept WHERE old.field = 'spec';
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT old.field, coalesce(old.dur, ''),
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', old.at), '')
         WHERE old.at IS NOT NULL
        UNION ALL
        SELECT '', '', coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', old.at), '')
         WHERE old.at_tz IS NOT NULL AND old.deleted_at IS NULL
           AND old.superseded_by IS NULL
           AND old.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
           AND old.at_tz IS NOT (
               SELECT at_tz FROM entries
                WHERE at_tz IS NOT NULL AND deleted_at IS NULL
                  AND superseded_by IS NULL
                  AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                  AND at < old.at AND rowid <> old.rowid
                ORDER BY at DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

-- A day's error marks are the integrity warnings pending at its instants, each
-- drawn where its entry was when it was found (`local_entry`).
CREATE TRIGGER IF NOT EXISTS dayview_kept_warning_insert AFTER INSERT ON integrity_warnings
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT new.local_entry ->> '$.field', '',
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', new.local_entry ->> '$.at.utc'), '')
         WHERE json_valid(new.local_entry) AND new.local_entry ->> '$.field' IS NOT NULL
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS dayview_kept_warning_update AFTER UPDATE ON integrity_warnings
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT w ->> '$.field', '',
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', w ->> '$.at.utc'), '')
          FROM (SELECT old.local_entry AS w UNION ALL SELECT new.local_entry)
         WHERE json_valid(w) AND w ->> '$.field' IS NOT NULL
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS dayview_kept_warning_delete AFTER DELETE ON integrity_warnings
WHEN EXISTS (SELECT 1 FROM dayview_kept) BEGIN
    INSERT INTO dayview_kept_dirty (field, dur, at)
        SELECT old.local_entry ->> '$.field', '',
               coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', old.local_entry ->> '$.at.utc'), '')
         WHERE json_valid(old.local_entry) AND old.local_entry ->> '$.field' IS NOT NULL
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;
