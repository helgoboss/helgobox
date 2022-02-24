use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::Lazy;
use std::thread;
use std::time::{Duration, Instant};

static METRICS_ENABLED: Lazy<bool> = Lazy::new(|| std::env::var("CLIP_ENGINE_METRICS").is_ok());
static METRICS_CHANNEL: Lazy<MetricsChannel> = Lazy::new(|| Default::default());

/// Initializes the metrics channel.  
pub fn init_metrics() {
    let _ = *METRICS_ENABLED;
    if !*METRICS_ENABLED {
        return;
    }
    let _ = *METRICS_CHANNEL;
    // We record metrics async because we are mostly in real-time threads when recording metrics.
    // The metrics and metrics-exporter-prometheus crates sometimes do allocations. If this would
    // just provoke audio dropouts, then fine ... users shouldn't collect metrics anyway under
    // normal circumstances, in live scenarios certainly never! But it could also distort results.
    thread::Builder::new()
        .name(String::from("Playtime metrics"))
        .spawn(move || {
            keep_recording_metrics((*METRICS_CHANNEL).receiver.clone());
        });
}

pub fn measure_time<R>(id: &'static str, f: impl FnOnce() -> R) -> R {
    if !*METRICS_ENABLED {
        return f();
    }
    let start = Instant::now();
    let result = f();
    let task = MetricsTask::Histogram {
        id,
        delta: start.elapsed(),
    };
    if METRICS_CHANNEL.sender.try_send(task).is_err() {
        debug!("Metrics channel is full");
    }
    result
}

struct MetricsChannel {
    sender: Sender<MetricsTask>,
    receiver: Receiver<MetricsTask>,
}

impl Default for MetricsChannel {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::bounded(5000);
        Self { sender, receiver }
    }
}

enum MetricsTask {
    Histogram { id: &'static str, delta: Duration },
}

fn keep_recording_metrics(receiver: Receiver<MetricsTask>) {
    while let Ok(task) = receiver.recv() {
        match task {
            MetricsTask::Histogram { id, delta } => {
                metrics::histogram!(id, delta);
            }
        }
    }
}
