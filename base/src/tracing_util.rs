#[macro_export]
macro_rules! tracing_debug {
    ($($tts:tt)*) => {
        assert_no_alloc::permit_alloc(|| {
            tracing::debug!($($tts)*);
        });
    }
}

#[macro_export]
macro_rules! tracing_warn {
    ($($tts:tt)*) => {
        assert_no_alloc::permit_alloc(|| {
            tracing::warn!($($tts)*);
        });
    }
}

#[macro_export]
macro_rules! tracing_error {
    ($($tts:tt)*) => {
        assert_no_alloc::permit_alloc(|| {
            tracing::error!($($tts)*);
        });
    }
}

pub fn ok_or_log_as_warn<T, E: tracing::Value>(result: Result<T, E>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(e);
            None
        }
    }
}
