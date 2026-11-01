//! da-jeve9: `-o x.png` hands the SVG `-o x.svg` writes to `rsvg-convert` from
//! PATH. A fake one pins what it is handed, since CI may not have librsvg, and
//! a real render runs only where it is installed.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Each on a fixed day, so two runs of it draw the same SVG. The last two are
/// for the size's rounding: the label gutter's fraction is one a round and
/// `rsvg-convert` alone both take up, and 150.2 only a round takes down, which
/// is what the size is named for (the assert under the first test holds it there).
const VIEWS: [&[&str]; 4] = [
    &[
        "dayview",
        "--start-date",
        "2026-05-01",
        "--end-date",
        "2026-05-01",
        "--width",
        "800",
    ],
    &[
        "nowview",
        "-t",
        "2026-05-01T10:30:00Z",
        "--context",
        "1h",
        "--width",
        "300",
        "--height",
        "150",
    ],
    &[
        "nowview",
        "-t",
        "2026-05-01T10:30:00Z",
        "--context",
        "1h",
        "--labels",
    ],
    &[
        "nowview",
        "-t",
        "2026-05-01T10:30:00Z",
        "--context",
        "1h",
        "--width",
        "300.5",
        "--height",
        "150.2",
    ],
];

/// The root's `name`, as written.
fn root_attr(svg: &str, name: &str) -> f64 {
    let root = &svg[svg.find("<svg").unwrap()..];
    let from = root.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
    root[from..].split('"').next().unwrap().parse().unwrap()
}

/// `args` with PATH as given, or inherited.
fn datum(dir: &Path, path: Option<&Path>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.args(args)
        .env("HOME", dir)
        .env("DATUM_HOME", dir.join("home"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("DATUM_DB", dir.join("t.db"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", "UTC");
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    cmd.output().unwrap()
}

/// A database with a block in the views' window.
fn seeded() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let out = datum(
        d.path(),
        None,
        &["start", "work", "-t", "2026-05-01T10:00:00Z", "--setup"],
    );
    assert!(out.status.success(), "{out:?}");
    d
}

/// A directory for PATH holding only an `rsvg-convert` running `body`.
fn fake(dir: &Path, body: &str) -> std::path::PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("rsvg-convert");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn output_args<'a>(view: &[&'a str], path: &'a str) -> Vec<&'a str> {
    let mut args = view.to_vec();
    args.extend(["-o", path]);
    args
}

#[test]
fn the_converter_is_handed_the_svg_on_stdin_and_its_stdout_is_the_file() {
    let d = seeded();
    let got = |name: &str| d.path().join(name);
    let bin = fake(
        d.path(),
        &format!(
            "printf '%s\\n' \"$@\" > '{args}'\n/bin/cat > '{stdin}'\nprintf 'made by the converter'",
            args = got("args").display(),
            stdin = got("stdin").display(),
        ),
    );
    let mut taken_down = false;
    for view in VIEWS {
        let svg = got("x.svg");
        let out = datum(d.path(), None, &output_args(view, svg.to_str().unwrap()));
        assert!(out.status.success(), "{out:?}");

        let png = got("x.png");
        let out = datum(
            d.path(),
            Some(&bin),
            &output_args(view, png.to_str().unwrap()),
        );
        assert!(out.status.success(), "{view:?}: {out:?}");

        let handed = std::fs::read_to_string(got("stdin")).unwrap();
        assert!(handed.contains("</svg>"), "{view:?}: {handed}");
        assert_eq!(handed, std::fs::read_to_string(&svg).unwrap(), "{view:?}");
        let (width, height) = (root_attr(&handed, "width"), root_attr(&handed, "height"));
        taken_down |= [width, height]
            .iter()
            .any(|side| side.fract() != 0.0 && side.round() < *side);
        assert_eq!(
            std::fs::read_to_string(got("args")).unwrap(),
            format!(
                "--format\npng\n--width\n{}\n--height\n{}\n",
                width.round(),
                height.round()
            ),
            "{view:?}"
        );
        assert_eq!(std::fs::read(&png).unwrap(), b"made by the converter");
    }
    assert!(taken_down, "no view tried a size a round takes down");
}

#[test]
fn with_none_on_path_it_says_what_to_install_and_writes_nothing() {
    let d = seeded();
    let empty = d.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    for view in VIEWS {
        let png = d.path().join("x.png");
        let out = datum(
            d.path(),
            Some(&empty),
            &output_args(view, png.to_str().unwrap()),
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{view:?}");
        for word in ["librsvg", "rsvg-convert", ".svg"] {
            assert!(stderr.contains(word), "{word} in {stderr}");
        }
        assert!(!png.exists(), "{view:?} left a file");

        // And what it names as needing nothing does.
        let svg = d.path().join("x.svg");
        let out = datum(
            d.path(),
            Some(&empty),
            &output_args(view, svg.to_str().unwrap()),
        );
        assert!(out.status.success(), "{view:?}: {out:?}");
    }
}

#[test]
fn a_failure_shows_its_stderr_and_leaves_the_path_as_it_was() {
    let d = seeded();
    // Fails having read nothing and having written half a file.
    let bin = fake(d.path(), "printf half\necho 'bad svg: boom' >&2\nexit 3");
    for view in VIEWS {
        let png = d.path().join("x.png");
        let out = datum(
            d.path(),
            Some(&bin),
            &output_args(view, png.to_str().unwrap()),
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{view:?}");
        assert!(stderr.contains("bad svg: boom"), "{stderr}");
        assert!(!png.exists(), "{view:?} left a file");

        let kept = d.path().join("kept.png");
        std::fs::write(&kept, "the last one").unwrap();
        let out = datum(
            d.path(),
            Some(&bin),
            &output_args(view, kept.to_str().unwrap()),
        );
        assert!(!out.status.success(), "{view:?}");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "the last one");
    }
}

#[test]
fn a_real_render_is_a_png_of_the_svgs_size() {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("rsvg-convert is not installed: no real render here");
        return;
    }
    let d = seeded();
    for view in VIEWS {
        let svg = d.path().join("x.svg");
        let png = d.path().join("x.png");
        for path in [&svg, &png] {
            let out = datum(d.path(), None, &output_args(view, path.to_str().unwrap()));
            assert!(out.status.success(), "{view:?}: {out:?}");
        }
        let svg = std::fs::read_to_string(svg).unwrap();
        let bytes = std::fs::read(png).unwrap();
        assert_eq!(&bytes[..PNG_MAGIC.len()], PNG_MAGIC, "{view:?}");
        let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(
            (be(16) as f64, be(20) as f64),
            (
                root_attr(&svg, "width").round(),
                root_attr(&svg, "height").round()
            ),
            "{view:?}"
        );
    }
}
