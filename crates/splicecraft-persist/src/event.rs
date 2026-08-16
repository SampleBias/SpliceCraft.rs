//! Structured log events. Sequence payloads are redacted.

use splicecraft_util::redact_for_log;

/// Emit a structured event. Values that look like DNA are replaced with `<dna N bp>`.
pub fn log_event(kind: &str, fields: &[(&str, &str)]) {
    let mut line = kind.to_owned();
    for (k, v) in fields {
        line.push(' ');
        line.push_str(k);
        line.push('=');
        line.push_str(&redact_for_log(v));
    }
    log::info!("{line}");
}

/// Format the same way as [`log_event`] without emitting (for tests).
#[must_use]
pub fn format_event(kind: &str, fields: &[(&str, &str)]) -> String {
    let mut line = kind.to_owned();
    for (k, v) in fields {
        line.push(' ');
        line.push_str(k);
        line.push('=');
        line.push_str(&redact_for_log(v));
    }
    line
}
