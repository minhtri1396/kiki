//! Pattern: Adapter. Turns the typed task errors of the store into the HTTP status the web UI expects.

use kiki_orca as orca;
use kiki_store as store;
use serde::{Deserialize, Deserializer};

use crate::api_error::{ApiError, ApiResult};
use crate::http::Request;

/// Decodes a JSON `null` as the type's default, so the web UI may send `null` for an empty text or list.
pub(crate) fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// A task that is missing is 404, a [`store::TaskError`] (something the user can fix) is 400, anything else 500.
pub(crate) fn task_error(err: &anyhow::Error) -> ApiError {
    if store::is_task_not_found(err) {
        return ApiError::from_anyhow(404, err);
    }
    if err
        .chain()
        .any(|cause| cause.downcast_ref::<store::TaskError>().is_some())
    {
        return ApiError::from_anyhow(400, err);
    }
    ApiError::from_anyhow(500, err)
}

/// A mistake the user can fix: the message is shown as it is, with status 400.
pub(crate) fn user_error(message: impl std::fmt::Display) -> ApiError {
    ApiError::new(400, message)
}

/// The validated `{id}` path value; an id that cannot be a task id answers like a task that does not exist.
pub(crate) fn task_id(req: &Request) -> ApiResult<String> {
    let id = req.path_value("id").trim().to_string();
    if !orca::valid_task_id(&id) {
        return Err(ApiError::new(404, store::ErrTaskNotFound));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_errors_map_to_statuses() {
        let missing = task_error(&store::ErrTaskNotFound.into());
        assert_eq!((missing.status(), missing.message()), (404, "task was not found"));
        let fixable = task_error(&anyhow::Error::new(store::TaskError::new("Enter a name.")).context("saving"));
        assert_eq!((fixable.status(), fixable.message()), (400, "saving: Enter a name."));
        assert_eq!(task_error(&anyhow::anyhow!("disk full")).status(), 500);
    }

    #[test]
    fn a_bad_task_id_is_not_found() {
        let req = Request::new("GET", "/api/tasks/x");
        assert_eq!(task_id(&req).unwrap_err().status(), 404);
    }
}
