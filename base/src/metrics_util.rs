use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

/// This will contain the metrics sender for async metrics recording if metrics are enabled.
static METRICS_SENDER: OnceLock<SyncSender<MetricsRecorderCommand>> = OnceLock::new();

#[derive(Debug)]
pub struct MetricsHook {
    sender: SyncSender<MetricsRecorderCommand>,
}

impl Drop for MetricsHook {
    fn drop(&mut self) {
        // This prevents the metrics recorder thread from lurking around after the library
        // is unloaded (which is of importance if "Allow complete unload of VST plug-ins"
        // is enabled in REAPER for Windows). Ideally, we would just destroy the sender to achieve
        // the same effect. But the sender is in a static variable which already has been
        // initialized once and therefore can't be set to `None`. Unloading the library will just
        // free the memory without triggering the drop, so that wouldn't work either.
        let _ = self.sender.try_send(MetricsRecorderCommand::Finish);
        // Joining the thread here somehow leads to a deadlock. Not sure why. It doesn't
        // seem to be necessary anyway. The thread will end no matter what.
    }
}

impl MetricsHook {
    /// Initializes metrics recording if the env variable `REALEARN_METRICS` is set.
    ///
    /// This starts a dedicated metrics recording thread, which is responsible for actually
    /// recording certain metrics (e.g. durations), which is especially important when measuring
    /// stuff from real-time threads. It avoids allocation and doesn't slow down real-time
    /// processing (with the exception of measuring the duration itself).
    ///
    /// This should be called only once within the lifetime of the loaded shared library! On Linux
    /// and macOS, this means it must only be called once within the lifetime of REAPER because once
    /// a shared library is loaded, it's not unloaded anymore. On Windows, it can be called again
    /// after REAPER unloaded the library via `FreeLibrary` and reloaded it again.
    ///
    /// The returned metrics hook must be dropped before the library is unloaded, otherwise the
    /// metrics thread sticks around and that can't be good.
    pub fn init() -> Option<Self> {
        std::env::var("REALEARN_METRICS").ok()?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(5000);
        thread::Builder::new()
            .name(String::from("ReaLearn metrics"))
            .spawn(move || {
                keep_recording_metrics(receiver);
            })
            .expect("ReaLearn metrics thread couldn't be created");
        METRICS_SENDER
            .set(sender.clone())
            .expect("attempting to initializing metrics hook more than once");
        let hook = Self { sender };
        Some(hook)
    }
}

/// A simple function that doesn't expose anything to the metrics endpoint but warns if a
/// threshold is exceeded. Doesn't do anything in release builds (except executing the function).
pub fn warn_if_takes_too_long<R>(label: &'static str, max: Duration, f: impl FnOnce() -> R) -> R {
    #[cfg(debug_assertions)]
    {
        let before = Instant::now();
        let r = f();
        let elapsed = before.elapsed();
        if elapsed > max {
            tracing_warn!(
                "Operation took too long: \"{label}\" ({})ms",
                elapsed.as_millis()
            );
        }
        r
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (label, max);
        f()
    }
}

/// Synchronously records the occurrence of the given event.
pub fn record_occurrence(id: &'static str) {
    if !metrics_are_enabled() {
        return;
    }
    metrics::increment_counter!(id);
}

/// Asynchronously measures and records the time of the given operation and exposes it at the
/// metrics endpoint.
pub fn measure_time<R>(id: &'static str, f: impl FnOnce() -> R) -> R {
    if !metrics_are_enabled() {
        return f();
    }
    let start = Instant::now();
    let result = f();
    record_duration_internal(id, start.elapsed());
    result
}

/// Records the given duration into a histogram.
pub fn record_duration(id: &'static str, delta: Duration) {
    record_duration_internal(id, delta);
}

pub fn metrics_are_enabled() -> bool {
    METRICS_SENDER.get().is_some()
}

pub fn record_duration_internal(id: &'static str, delta: Duration) {
    if let Some(sender) = METRICS_SENDER.get() {
        let task = MetricsRecorderCommand::Histogram { id, delta };
        if sender.try_send(task).is_err() {
            tracing::debug!("ReaLearn metrics channel is full");
        }
    }
}

enum MetricsRecorderCommand {
    Finish,
    Histogram { id: &'static str, delta: Duration },
}

fn keep_recording_metrics(receiver: Receiver<MetricsRecorderCommand>) {
    while let Ok(task) = receiver.recv() {
        match task {
            MetricsRecorderCommand::Finish => break,
            MetricsRecorderCommand::Histogram { id, delta } => {
                metrics::histogram!(id, delta);
            }
        }
    }
    println!("Recording metrics finished");
}
