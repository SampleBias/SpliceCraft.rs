use clap::Parser;
use splicecraft_cli::{Cli, run};

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
