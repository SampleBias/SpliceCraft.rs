//! Localhost JSON agent API (`splicecraft --agent` / `--headless`).
//!
//! Bind is **`127.0.0.1` only**. Writes go through
//! `splicecraft_persist::safe_save_json`. Sequences are never logged.
//! See `docs/stages/14-agent-api-cli.md`.

#![forbid(unsafe_code)]

pub use splicecraft_clone as clone;
pub use splicecraft_core as core;
pub use splicecraft_io as io;
pub use splicecraft_persist as persist;

mod dispatch;
mod error;
mod handlers;
mod http;
mod paths;
mod registry;
mod session;
mod token;

pub use dispatch::{AgentResponse, dirty_guard, dispatch, payload_force};
pub use error::AgentError;
pub use http::{
    BIND_HOST, BackgroundAgent, DEFAULT_PORT, ServerConfig, ServerHandle, host_is_loopback,
    run_headless, spawn_background, start_server,
};
pub use paths::{check_read_path, check_write_path, sanitize_agent_path, scrub_path};
pub use registry::{FORBIDDEN_ENDPOINT_NAMES, Registry, builtin, is_forbidden_name};
pub use session::AgentSession;
pub use token::{AGENT_TOKEN_FILE_NAME, generate_token, token_path, write_token_file};

/// Stage that implements this crate's real HTTP surface.
pub const IMPLEMENTATION_STAGE: u8 = 14;

/// Crate identity (workspace wiring check).
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests;
