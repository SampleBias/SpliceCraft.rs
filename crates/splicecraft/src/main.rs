use clap::Parser;

/// SpliceCraft.rs — plasmid workbench TUI.
#[derive(Debug, Parser)]
#[command(
    name = "splicecraft",
    version,
    about = "SpliceCraft.rs plasmid workbench"
)]
struct Args {}

fn main() {
    let _args = Args::parse();
    if let Err(err) = splicecraft_tui::run() {
        eprintln!("splicecraft tui error: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sidecar_is_linked() {
        assert_eq!(splicecraft_cli::crate_name(), "splicecraft-cli");
    }
}
