use std::io::IsTerminal;

use clap::Parser;

/// SpliceCraft.rs — plasmid workbench TUI + optional localhost agent API.
#[derive(Debug, Parser)]
#[command(
    name = "splicecraft",
    version,
    about = "SpliceCraft.rs plasmid workbench"
)]
struct Args {
    /// Serve the localhost JSON agent API alongside the TUI.
    #[arg(long)]
    agent: bool,
    /// API only (no TUI). Alias: `--agent-headless`.
    #[arg(long, alias = "agent-headless")]
    headless: bool,
    /// Loopback port (default 6701). Never binds `0.0.0.0`.
    #[arg(long, default_value_t = splicecraft_agent::DEFAULT_PORT)]
    agent_port: u16,
    /// Skip the greyscale DNA splash (also `SPLICECRAFT_NO_SPLASH=1`).
    #[arg(long)]
    no_splash: bool,
}

fn main() {
    let args = Args::parse();
    let env_headless = std::env::var("SPLICECRAFT_HEADLESS")
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"));
    let no_tty = !std::io::stdin().is_terminal();
    let headless = args.headless || env_headless || (args.agent && no_tty);

    if headless {
        if let Err(err) = splicecraft_agent::run_headless(args.agent_port) {
            eprintln!("splicecraft agent error: {err}");
            std::process::exit(1);
        }
        return;
    }
    if args.agent
        && let Err(err) = splicecraft_agent::spawn_background(args.agent_port, false)
    {
        eprintln!("splicecraft agent error: {err}");
        std::process::exit(1);
    }
    let splash = !args.no_splash && splicecraft_tui::splash_enabled_from_env();
    if let Err(err) = splicecraft_tui::run_with(splicecraft_tui::RunOptions { splash }) {
        eprintln!("splicecraft tui error: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Args;

    #[test]
    fn sidecar_is_linked() {
        assert_eq!(splicecraft_cli::crate_name(), "splicecraft-cli");
    }

    #[test]
    fn agent_flags_parse() {
        let a = Args::try_parse_from(["splicecraft", "--headless", "--agent-port", "0"]).unwrap();
        assert!(a.headless);
        assert_eq!(a.agent_port, 0);
        let b = Args::try_parse_from(["splicecraft", "--agent"]).unwrap();
        assert!(b.agent);
        assert_eq!(b.agent_port, splicecraft_agent::DEFAULT_PORT);
        let c = Args::try_parse_from(["splicecraft", "--no-splash"]).unwrap();
        assert!(c.no_splash);
    }
}
