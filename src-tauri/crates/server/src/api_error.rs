//! Pattern: Adapter. Turns a failure inside a handler into the HTTP reply the web UI expects.
//!
//! Handlers return [`ApiResult`] and use `?`; the router adapter calls [`respond`] once, so no handler builds an
//! error `Response` by hand and the `{"error": "..."}` shape lives in one place.

use std::fmt::Display;

use crate::http::{write_anyhow, write_err, Response};

/// A failed request: the status code and the text shown to the user.
#[derive(Debug)]
pub struct ApiError {
    status: u16,
    message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: u16, message: impl Display) -> ApiError {
        ApiError {
            status,
            message: message.to_string(),
        }
    }

    pub fn bad_request(message: impl Display) -> ApiError {
        ApiError::new(400, message)
    }

    pub fn not_found(message: impl Display) -> ApiError {
        ApiError::new(404, message)
    }

    /// Keeps the whole cause chain (`outer: inner`), like the replies the app always sent.
    pub fn from_anyhow(status: u16, err: &anyhow::Error) -> ApiError {
        ApiError {
            status,
            message: format!("{err:#}"),
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn into_response(self) -> Response {
        write_err(self.status, &self.message)
    }
}

impl From<Response> for ApiError {
    /// Lets `?` pass through helpers that still return a ready-made error reply (for example `read_json`).
    fn from(response: Response) -> ApiError {
        let status = response.status;
        let message = serde_json::from_str::<serde_json::Value>(&response.into_text())
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(str::to_string)))
            .unwrap_or_default();
        ApiError { status, message }
    }
}

/// `result.or_status(400)?`: the status to answer with when the call fails.
pub trait OrStatus<T> {
    fn or_status(self, status: u16) -> ApiResult<T>;
}

impl<T, E: Into<anyhow::Error>> OrStatus<T> for Result<T, E> {
    fn or_status(self, status: u16) -> ApiResult<T> {
        self.map_err(|err| ApiError::from_anyhow(status, &err.into()))
    }
}

/// Runs a fallible handler body and produces the final reply.
pub fn respond(result: ApiResult<Response>) -> Response {
    result.unwrap_or_else(ApiError::into_response)
}

/// Replies for an `anyhow` error that a domain classified (see `classify_*` in the domain modules).
pub fn reply_anyhow(status: u16, err: &anyhow::Error) -> Response {
    write_anyhow(status, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::write_json;

    #[test]
    fn error_reply_has_the_error_shape() {
        let reply = ApiError::new(400, "nope").into_response();
        assert_eq!(reply.status, 400);
        assert_eq!(reply.into_text(), "{\"error\":\"nope\"}\n");
    }

    #[test]
    fn or_status_keeps_the_cause_chain() {
        let failed: Result<(), anyhow::Error> = Err(anyhow::anyhow!("inner").context("outer"));
        let err = failed.or_status(500).unwrap_err();
        assert_eq!((err.status(), err.message()), (500, "outer: inner"));
    }

    #[test]
    fn a_ready_made_error_reply_converts_back() {
        let err = ApiError::from(crate::http::write_err(404, &"missing"));
        assert_eq!((err.status(), err.message()), (404, "missing"));
        assert_eq!(respond(Ok(write_json(200, &1))).status, 200);
    }
}
