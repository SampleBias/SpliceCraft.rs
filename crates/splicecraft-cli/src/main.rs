use clap::Parser;
use splicecraft_cli::{Cli, CliError, run};

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        if !matches!(err, CliError::Agent { .. }) {
            eprintln!("{err}");
        }
        std::process::exit(err.exit_code());
    }
}
