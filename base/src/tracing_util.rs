use std::error::Error;
use std::fmt::Display;

pub fn ok_or_log_as_warn<T, E: Display>(result: Result<T, E>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("{e}");
            None
        }
    }
}

pub fn log_if_error<T>(result: Result<T, impl AsRef<dyn Error>>) {
    if let Err(error) = result {
        let error = error.as_ref();
        tracing::error!(msg = "Error", error);
    }
}
