macro_rules! tracing_debug {
    ($($tts:tt)*) => {
        assert_no_alloc::permit_alloc(|| {
            tracing::debug!($($tts)*);
        });
    }
}
