use anyhow::Result;

fn main() -> Result<()> {
    datum_cli::run(std::env::args().collect())
}
