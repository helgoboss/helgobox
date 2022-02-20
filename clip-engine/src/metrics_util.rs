use once_cell::sync::Lazy;
use std::time::Instant;

static METRICS_ENABLED: Lazy<bool> = Lazy::new(|| std::env::var("CLIP_ENGINE_METRICS").is_ok());

pub fn measure_time<R>(id: &'static str, f: impl FnOnce() -> R) -> R {
    if !*METRICS_ENABLED {
        return f();
    }
    let start = Instant::now();
    let result = f();
    let delta = start.elapsed();
    metrics::histogram!(id, delta);
    result
}
