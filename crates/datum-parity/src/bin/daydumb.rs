//! daydumb's `datum`, built in this crate so the text goldens can spawn it:
//! cargo hands an integration test `CARGO_BIN_EXE_<name>` only for its own
//! package's binaries. Same entry as `datum-cli`'s `main`, less its tips and
//! exit-code mapping, which no golden reads.

fn main() {
    if let Err(e) = datum_cli::run(std::env::args().collect()) {
        eprintln!("Error: {e:?}");
        std::process::exit(1);
    }
}
