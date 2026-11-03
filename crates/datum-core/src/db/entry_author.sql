-- da-zpc98: an entry's `author` is its latest change's. A write carries its
-- own on the entry where it can (a create's insert), so this is skipped where
-- the entry already says it, and a run of one author's changes writes no
-- more. Laid by the v79→v80 migration and after `schema.sql` at every open.
CREATE TRIGGER IF NOT EXISTS entry_history_author AFTER INSERT ON entry_history BEGIN
    UPDATE entries SET author = new.author
     WHERE hid = new.entry_hid AND author IS NOT new.author;
END;
