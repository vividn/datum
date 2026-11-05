//! `-o` reads an entry's own columns as `--id` writes them (da-noy6z):
//! `%at%`, `%uid%`, `%create_time%` and `%modify_time%` rendered empty.

#[test]
fn format_string_resolves_the_columns_an_id_does() {
    let tmp = tempfile::tempdir().unwrap();
    let datum = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", tmp.path().join("t.db"))
            .env("DATUM_HOME", tmp.path())
            .env("DATUM_CONFIG_HOME", tmp.path())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "datum {args:?} failed");
        String::from_utf8(out.stdout).unwrap()
    };
    // The same entry slugged by `--id` with each column, and read back by `-o`.
    let id = "%uid%|%at%|%create_time%|%modify_time%";
    datum(&["moment", "m", "n=3", "--id", id]);
    let slug = datum(&["tail", "-o", "%slug%"]);
    let from_id = slug.trim().strip_prefix("m:").unwrap();
    let from_o = datum(&["tail", "-o", id]);
    assert_eq!(from_o.trim(), from_id);
    assert_eq!(from_id.split('|').filter(|v| v.is_empty()).count(), 0);
    // A data key still reads the key.
    assert_eq!(datum(&["tail", "-o", "%n%"]).trim(), "3");
}
