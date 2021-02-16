use crate::core::AsyncNotifier;
use reaper_high::Fx;
use rx_util::{Event, Notifier};
use rxrust::prelude::*;
use std::collections::HashMap;

/// Feedback for most targets comes from REAPER itself but there are some targets for which ReaLearn
/// holds the state. It's in this struct.
#[derive(Default)]
pub struct RealearnTargetContext {
    fx_snapshot_loaded_subject: LocalSubject<'static, Fx, ()>,
    fx_snapshot_chunk_hash_by_fx: HashMap<Fx, u64>,
}

impl RealearnTargetContext {
    pub fn current_fx_snapshot_chunk_hash(&self, fx: &Fx) -> Option<u64> {
        self.fx_snapshot_chunk_hash_by_fx.get(fx).copied()
    }

    pub fn load_fx_snapshot(&mut self, fx: Fx, chunk: &str, chunk_hash: u64) {
        fx.set_tag_chunk(chunk);
        AsyncNotifier::notify(&mut self.fx_snapshot_loaded_subject, &fx);
        self.fx_snapshot_chunk_hash_by_fx.insert(fx, chunk_hash);
    }

    pub fn fx_snapshot_loaded(&self) -> impl Event<Fx> {
        self.fx_snapshot_loaded_subject.clone()
    }
}
