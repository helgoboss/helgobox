use crossbeam_channel::{Receiver, Sender};
use once_cell::sync::Lazy;

enum PotTask {
    RebuildCollections,
}

struct PotChannel {
    sender: Sender<PotTask>,
    receiver: Receiver<PotTask>,
}

impl Default for PotChannel {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::bounded(100);
        Self { sender, receiver }
    }
}

static POT_WORKER_CHANNEL: Lazy<PotChannel> = Lazy::new(Default::default);
