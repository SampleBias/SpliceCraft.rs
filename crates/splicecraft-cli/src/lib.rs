//! Command-line sidecar for the SpliceCraft.rs agent API.
//!
//! Stage 00 ships `--version` / `version`. Stage 14 adds `call` passthrough.

#![forbid(unsafe_code)]

pub use splicecraft_agent as agent;

use clap::{Parser, Subcommand};

/// Stage that implements the real agent-CLI surface.
pub const IMPLEMENTATION_STAGE: u8 = 14;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// `splicecraft-cli` arguments.
#[derive(Debug, Parser)]
#[command(
    name = "splicecraft-cli",
    version,
    about = "SpliceCraft.rs agent CLI sidecar"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Sidecar subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print the CLI version.
    Version,
}

/// Errors from the sidecar (placeholder until stage 14).
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Catch-all until real I/O lands.
    #[error("{0}")]
    Message(String),
}

/// Run a parsed CLI invocation.
pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Some(Commands::Version) | None => {
            println!("splicecraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches() {
        assert_eq!(crate_name(), "splicecraft-cli");
    }

    #[test]
    fn version_command_succeeds() {
        run(Cli {
            command: Some(Commands::Version),
        })
        .expect("version");
    }
}
