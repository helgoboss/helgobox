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
