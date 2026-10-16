//! `spec F --json` prints the same bytes in every process (da-l6950): a
//! spec's maps are sorted by name, not in a process's hash order.

use std::process::Command;

#[test]
fn spec_json_is_byte_identical_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let datum = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", dir.path().join("o.db"))
            .env("DATUM_HOME", dir.path())
            .env("DATUM_CONFIG_HOME", dir.path().join("config"))
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    // Eight names in each map: a hash order matching across five processes
    // is a chance of 1 in 8!^4 per map.
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let subfields: serde_json::Map<_, _> = names
        .iter()
        .map(|n| (format!("{n}k"), serde_json::json!({"value_type": "any"})))
        .collect();
    let json = serde_json::json!({"verb": "switch", "subfields": subfields});
    datum(&["spec", "mood", "--from-json", &json.to_string()]);
    let mut args = vec!["spec", "mood"];
    let flags: Vec<String> = names
        .iter()
        .flat_map(|n| {
            [
                format!("--state-color={n}=red"),
                format!("--state-description={n}=it is {n}"),
                format!("--state-glyph={n}=*"),
                format!("--state-key={n}={n}x"),
                format!("--state-assign={n}=y=1"),
            ]
        })
        .collect();
    args.extend(flags.iter().map(String::as_str));
    datum(&args);
    let first = datum(&["spec", "mood", "--json"]);
    for _ in 0..4 {
        assert_eq!(datum(&["spec", "mood", "--json"]), first);
    }
}
