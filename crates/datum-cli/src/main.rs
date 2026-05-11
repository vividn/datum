use anyhow::Result;
use datum_cli::repl;

fn main() -> Result<()> {
    if std::env::args_os().len() <= 1 {
        return repl::run();
    }
    datum_cli::run(std::env::args().collect())
}
