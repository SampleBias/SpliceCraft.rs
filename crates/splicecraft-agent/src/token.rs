//! Bearer token + `agent_token` file (port + token). Lives under `splicecraft-rs`.

use std::fs;
use std::path::{Path, PathBuf};

use splicecraft_persist::{DataLayout, atomic_write_bytes};

use crate::error::AgentError;

/// Filename under the Rust data dir (never the Python `splicecraft/` leaf).
pub const AGENT_TOKEN_FILE_NAME: &str = "agent_token";

/// `<data-dir>/agent_token`.
#[must_use]
pub fn token_path(layout: &DataLayout) -> PathBuf {
    layout.root.join(AGENT_TOKEN_FILE_NAME)
}

/// 256-bit hex token (`secrets.token_urlsafe(32)` analogue).
pub fn generate_token() -> Result<String, AgentError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| AgentError::Message(format!("token rng failed: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Write `port\\ntoken\\n` with mode 0600 on POSIX.
pub fn write_token_file(path: &Path, port: u16, token: &str) -> Result<(), AgentError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!("{port}\n{token}\n");
    atomic_write_bytes(path, body.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
