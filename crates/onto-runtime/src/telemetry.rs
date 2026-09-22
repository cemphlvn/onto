//! Telemetry: one JSON object per line, one line per event.
//!
//! Every line carries `timestamp`, `level` and `event` (for example
//! `chooser.call`, `proposer.call`, `potentiality`, `mem.sample`), then the
//! event's own fields. Only the `onto` target is recorded, so dependency
//! logs never mix in.

use std::fs::File;
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

/// Sends telemetry to `path` as JSON lines. Call once, before running.
pub fn init_jsonl(path: &Path) -> std::io::Result<()> {
    let file = File::create(path)?;
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(false)
        .with_writer(Mutex::new(file))
        .with_env_filter(EnvFilter::new("onto=info"))
        .init();
    Ok(())
}
