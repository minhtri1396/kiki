//! Errors the callers branch on.

use std::fmt;

/// A problem with the text or the chosen tool, before a CLI runs.
///
/// Detect it on an `anyhow::Error` with `err.downcast_ref::<InputError>()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputError {
    pub message: String,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InputError {}

/// An `anyhow::Error` that carries an [`InputError`].
pub(crate) fn input_error(message: &str) -> anyhow::Error {
    anyhow::Error::new(InputError {
        message: message.to_string(),
    })
}
