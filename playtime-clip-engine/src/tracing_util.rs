macro_rules! debug {
    ($($tts:tt)*) => {
        assert_no_alloc::permit_alloc(|| {
            tracing::debug!($($tts)*);
        });
    }
}
