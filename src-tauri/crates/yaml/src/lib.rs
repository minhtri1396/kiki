//! Shared YAML layer: one emitter and one reader that behave like yaml.v3.
//!
//! Pattern: Builder. [`Mapping`] assembles an ordered document that [`emit`] writes with yaml.v3's layout
//! and scalar quoting, so every file KIKI writes (setup, repos, tools, MCP settings, agent front matter,
//! shared team repos) is byte-for-byte what yaml.v3 would write.
//!
//! * Writing: [`Mapping`] / [`Value`] / [`emit`], and [`scalar`] for one inline string.
//! * Reading: [`read::parse`] builds a [`read::Node`] tree; [`read::Fields`] decodes a mapping with the
//!   string, bool and int rules of yaml.v3 (null is the zero value, numbers read as their text).
//! * [`resolve`] is the typing of plain scalars used by both sides.
//!
//! `server/team.rs` can replace its `yaml_scalar` with [`scalar`] and its hand-built `canonical()` text with
//! `Mapping::new().set("id", ..).set("services", ..).set_unless_empty("index", ..).emit()`.

mod emit;
pub mod read;
mod resolve;
mod scalar;

pub use emit::{emit, Mapping, Value};
pub use resolve::{resolve, Resolved};
pub use scalar::scalar;
