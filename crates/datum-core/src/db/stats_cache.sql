-- da-n7tp2: `stats` per closed local day, so all time is the cached days
-- plus today, computed live. LOCAL and derived: never synced, rebuilt from
-- the entries whenever it is missing, and wiped whole when its stamp (the
-- `_meta` key `stats_cache`) is not this datum's. The reading and the
-- filling are `datum-commands`' `commands/stats/cache.rs`; the triggers
-- below are what keep it true, whichever path writes a row. Laid down after
-- `schema.sql` at every open and by the v55→v56 migration, so it is a file
-- of its own.
--
-- One row per field, day (YYYY-MM-DD in the database's own zone) and state
-- label: the entries counted, the seconds the state held, and the classes
-- the label was read with (1 tracked, 2 closed, 4 untracked).
CREATE TABLE IF NOT EXISTS stats_cache (
    field   TEXT NOT NULL,
    day     TEXT NOT NULL,
    state   TEXT NOT NULL,
    count   INTEGER NOT NULL,
    seconds INTEGER NOT NULL,
    class   INTEGER NOT NULL,
    PRIMARY KEY (field, day, state)
) WITHOUT ROWID;

-- A cached day's first and last counted entry, RFC 3339 in their own zones.
CREATE TABLE IF NOT EXISTS stats_cache_days (
    field TEXT NOT NULL,
    day   TEXT NOT NULL,
    first TEXT NOT NULL,
    last  TEXT NOT NULL,
    PRIMARY KEY (field, day)
) WITHOUT ROWID;

-- The days of `field` from `lo` (`''`: its first) up to `day` are cached.
CREATE TABLE IF NOT EXISTS stats_cache_through (
    field TEXT NOT NULL PRIMARY KEY,
    lo    TEXT NOT NULL,
    day   TEXT NOT NULL
);

-- What the writes since the last `stats` touched: per field and `dur`, the
-- earliest `occur_time` written or removed. SQL cannot subtract an ISO
-- duration, so the leading edge is derived when the cache reads this. A
-- field of `''` is every field: the row moved where the database's zone
-- changes, which moves every day's midnights after it. A `dur` and `at` of
-- `''` is the whole field: a spec of it changed.
CREATE TABLE IF NOT EXISTS stats_cache_dirty (
    field TEXT NOT NULL,
    dur   TEXT NOT NULL,
    at    TEXT NOT NULL,
    PRIMARY KEY (field, dur)
) WITHOUT ROWID;

-- The zone a row's own `occur_time_tz` names differs from the zone before
-- it: the row is where `Db::zone_timeline` changes zone. The index answers
-- "the zone before it" with one seek; without it the check cost a write
-- ~0.2 ms on five years.
CREATE INDEX IF NOT EXISTS entries_zoned
    ON entries (occur_time, occur_time_tz)
    WHERE occur_time_tz IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit');
CREATE TRIGGER IF NOT EXISTS stats_cache_insert AFTER INSERT ON entries BEGIN
    INSERT INTO stats_cache_dirty (field, dur, at)
        SELECT new.field, coalesce(new.dur, ''), new.occur_time WHERE new.occur_time IS NOT NULL
        UNION ALL SELECT substr(new.slug, 6), '', '' WHERE new.field = 'spec'
        UNION ALL SELECT '', '', new.occur_time
            WHERE new.occur_time_tz IS NOT NULL AND new.deleted_at IS NULL
              AND new.superseded_by IS NULL
              AND new.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
              AND new.occur_time_tz IS NOT (
                  SELECT occur_time_tz FROM entries
                   WHERE occur_time_tz IS NOT NULL AND deleted_at IS NULL
                     AND superseded_by IS NULL
                     AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                     AND occur_time < new.occur_time AND rowid <> new.rowid
                   ORDER BY occur_time DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS stats_cache_update AFTER UPDATE ON entries BEGIN
    INSERT INTO stats_cache_dirty (field, dur, at)
        SELECT old.field, coalesce(old.dur, ''), old.occur_time WHERE old.occur_time IS NOT NULL
        UNION ALL SELECT substr(old.slug, 6), '', '' WHERE old.field = 'spec'
        UNION ALL SELECT '', '', old.occur_time
            WHERE old.occur_time_tz IS NOT NULL AND old.deleted_at IS NULL
              AND old.superseded_by IS NULL
              AND old.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
              AND old.occur_time_tz IS NOT (
                  SELECT occur_time_tz FROM entries
                   WHERE occur_time_tz IS NOT NULL AND deleted_at IS NULL
                     AND superseded_by IS NULL
                     AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                     AND occur_time < old.occur_time AND rowid <> old.rowid
                   ORDER BY occur_time DESC LIMIT 1)
        UNION ALL
        SELECT new.field, coalesce(new.dur, ''), new.occur_time WHERE new.occur_time IS NOT NULL
        UNION ALL SELECT substr(new.slug, 6), '', '' WHERE new.field = 'spec'
        UNION ALL SELECT '', '', new.occur_time
            WHERE new.occur_time_tz IS NOT NULL AND new.deleted_at IS NULL
              AND new.superseded_by IS NULL
              AND new.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
              AND new.occur_time_tz IS NOT (
                  SELECT occur_time_tz FROM entries
                   WHERE occur_time_tz IS NOT NULL AND deleted_at IS NULL
                     AND superseded_by IS NULL
                     AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                     AND occur_time < new.occur_time AND rowid <> new.rowid
                   ORDER BY occur_time DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;

CREATE TRIGGER IF NOT EXISTS stats_cache_delete AFTER DELETE ON entries BEGIN
    INSERT INTO stats_cache_dirty (field, dur, at)
        SELECT old.field, coalesce(old.dur, ''), old.occur_time WHERE old.occur_time IS NOT NULL
        UNION ALL SELECT substr(old.slug, 6), '', '' WHERE old.field = 'spec'
        UNION ALL SELECT '', '', old.occur_time
            WHERE old.occur_time_tz IS NOT NULL AND old.deleted_at IS NULL
              AND old.superseded_by IS NULL
              AND old.field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
              AND old.occur_time_tz IS NOT (
                  SELECT occur_time_tz FROM entries
                   WHERE occur_time_tz IS NOT NULL AND deleted_at IS NULL
                     AND superseded_by IS NULL
                     AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
                     AND occur_time < old.occur_time AND rowid <> old.rowid
                   ORDER BY occur_time DESC LIMIT 1)
        ON CONFLICT (field, dur) DO UPDATE SET at = excluded.at WHERE excluded.at < at;
END;
