use std::fmt::Display;
#[macro_export]
macro_rules! tracing_debug {
    ($($tts:tt)*) => {
        helgoboss_allocator::permit_alloc(|| {
            tracing::debug!($($tts)*);
        });
    }
}
#[macro_export]
macro_rules! tracing_trace {
    ($($tts:tt)*) => {
        helgoboss_allocator::permit_alloc(|| {
            tracing::trace!($($tts)*);
        });
    }
}

#[macro_export]
macro_rules! tracing_warn {
    ($($tts:tt)*) => {
        helgoboss_allocator::permit_alloc(|| {
            tracing::warn!($($tts)*);
        });
    }
}

#[macro_export]
macro_rules! tracing_error {
    ($($tts:tt)*) => {
        helgoboss_allocator::permit_alloc(|| {
            tracing::error!($($tts)*);
        });
    }
}

pub fn ok_or_log_as_warn<T, E: Display>(result: Result<T, E>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("{e}");
            None
        }
    }
}
