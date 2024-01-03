use crate::infrastructure::proto::{
    event_reply, ContinuousColumnUpdate, ContinuousMatrixUpdate, EventReply,
    OccasionalGlobalUpdate, OccasionalMatrixUpdate, QualifiedContinuousSlotUpdate,
    QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate, QualifiedOccasionalRowUpdate,
    QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
};
use futures::future;
use tokio::sync::broadcast::{Receiver, Sender};

/// This must be a global object because it's responsible for supplying one gRPC endpoint with
/// streaming data and we have only one endpoint for all matrices.
#[derive(Clone, Debug)]
pub struct ClipEngineSenders {
    pub occasional_global_update_sender: Sender<OccasionalGlobalUpdateBatch>,
    pub occasional_matrix_update_sender: Sender<OccasionalMatrixUpdateBatch>,
    pub occasional_track_update_sender: Sender<OccasionalTrackUpdateBatch>,
    pub occasional_column_update_sender: Sender<OccasionalColumnUpdateBatch>,
    pub occasional_row_update_sender: Sender<OccasionalRowUpdateBatch>,
    pub occasional_slot_update_sender: Sender<OccasionalSlotUpdateBatch>,
    pub occasional_clip_update_sender: Sender<OccasionalClipUpdateBatch>,
    pub continuous_matrix_update_sender: Sender<ContinuousMatrixUpdateBatch>,
    pub continuous_column_update_sender: Sender<ContinuousColumnUpdateBatch>,
    pub continuous_slot_update_sender: Sender<ContinuousSlotUpdateBatch>,
}

#[derive(Debug)]
pub struct ClipEngineReceivers {
    pub occasional_global_update_receiver: Receiver<OccasionalGlobalUpdateBatch>,
    pub occasional_matrix_update_receiver: Receiver<OccasionalMatrixUpdateBatch>,
    pub occasional_track_update_receiver: Receiver<OccasionalTrackUpdateBatch>,
    pub occasional_column_update_receiver: Receiver<OccasionalColumnUpdateBatch>,
    pub occasional_row_update_receiver: Receiver<OccasionalRowUpdateBatch>,
    pub occasional_slot_update_receiver: Receiver<OccasionalSlotUpdateBatch>,
    pub occasional_clip_update_receiver: Receiver<OccasionalClipUpdateBatch>,
    pub continuous_matrix_update_receiver: Receiver<ContinuousMatrixUpdateBatch>,
    pub continuous_column_update_receiver: Receiver<ContinuousColumnUpdateBatch>,
    pub continuous_slot_update_receiver: Receiver<ContinuousSlotUpdateBatch>,
}

impl ClipEngineReceivers {
    pub async fn keep_processing_updates(
        &mut self,
        session_id: &str,
        process: &impl Fn(EventReply),
    ) {
        future::join(
            future::join5(
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.continuous_matrix_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.continuous_column_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.continuous_slot_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_matrix_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_track_update_receiver,
                ),
            ),
            future::join4(
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_column_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_row_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_slot_update_receiver,
                ),
                keep_processing_updates(
                    session_id,
                    process,
                    &mut self.occasional_clip_update_receiver,
                ),
            ),
        )
        .await;
    }

    pub fn process_pending_updates(&mut self, session_id: &str, process: &impl Fn(EventReply)) {
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_matrix_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_track_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_column_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_row_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_slot_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.occasional_clip_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.continuous_matrix_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.continuous_column_update_receiver,
        );
        process_pending_updates(
            session_id,
            process,
            &mut self.continuous_slot_update_receiver,
        );
    }
}

async fn keep_processing_updates<T>(
    session_id: &str,
    process: impl Fn(EventReply),
    receiver: &mut Receiver<WithSessionId<T>>,
) where
    T: Clone + Into<event_reply::Value>,
{
    loop {
        if let Ok(batch) = receiver.recv().await {
            if batch.session_id != session_id {
                continue;
            }
            let reply = EventReply {
                value: Some(batch.value.into()),
            };
            process(reply);
        }
    }
}

fn process_pending_updates<T>(
    session_id: &str,
    process: impl Fn(EventReply),
    receiver: &mut Receiver<WithSessionId<T>>,
    // to_reply_value: impl Fn(Vec<T>) -> event_reply::Value
) where
    T: Clone + Into<event_reply::Value>,
{
    while let Ok(batch) = receiver.try_recv() {
        if batch.session_id != session_id {
            continue;
        }
        let reply = EventReply {
            value: Some(batch.value.into()),
        };
        process(reply);
    }
}

impl Default for ClipEngineSenders {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipEngineSenders {
    pub fn new() -> Self {
        Self {
            occasional_global_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_matrix_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_track_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_column_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_row_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_slot_update_sender: tokio::sync::broadcast::channel(100).0,
            occasional_clip_update_sender: tokio::sync::broadcast::channel(100).0,
            continuous_slot_update_sender: tokio::sync::broadcast::channel(1000).0,
            continuous_column_update_sender: tokio::sync::broadcast::channel(500).0,
            continuous_matrix_update_sender: tokio::sync::broadcast::channel(500).0,
        }
    }

    pub fn subscribe_to_all(&self) -> ClipEngineReceivers {
        ClipEngineReceivers {
            occasional_global_update_receiver: self.occasional_global_update_sender.subscribe(),
            occasional_matrix_update_receiver: self.occasional_matrix_update_sender.subscribe(),
            occasional_track_update_receiver: self.occasional_track_update_sender.subscribe(),
            occasional_column_update_receiver: self.occasional_column_update_sender.subscribe(),
            occasional_row_update_receiver: self.occasional_row_update_sender.subscribe(),
            occasional_slot_update_receiver: self.occasional_slot_update_sender.subscribe(),
            occasional_clip_update_receiver: self.occasional_clip_update_sender.subscribe(),
            continuous_matrix_update_receiver: self.continuous_matrix_update_sender.subscribe(),
            continuous_column_update_receiver: self.continuous_column_update_sender.subscribe(),
            continuous_slot_update_receiver: self.continuous_slot_update_sender.subscribe(),
        }
    }

    pub fn send_initial_matrix_updates(&self) {}
}

#[derive(Clone)]
pub struct WithSessionId<T> {
    pub session_id: String,
    pub value: T,
}

pub type OccasionalGlobalUpdateBatch = Vec<OccasionalGlobalUpdate>;
pub type OccasionalMatrixUpdateBatch = WithSessionId<Vec<OccasionalMatrixUpdate>>;
pub type OccasionalTrackUpdateBatch = WithSessionId<Vec<QualifiedOccasionalTrackUpdate>>;
pub type OccasionalColumnUpdateBatch = WithSessionId<Vec<QualifiedOccasionalColumnUpdate>>;
pub type OccasionalRowUpdateBatch = WithSessionId<Vec<QualifiedOccasionalRowUpdate>>;
pub type OccasionalSlotUpdateBatch = WithSessionId<Vec<QualifiedOccasionalSlotUpdate>>;
pub type OccasionalClipUpdateBatch = WithSessionId<Vec<QualifiedOccasionalClipUpdate>>;
pub type ContinuousMatrixUpdateBatch = WithSessionId<ContinuousMatrixUpdate>;
pub type ContinuousColumnUpdateBatch = WithSessionId<Vec<ContinuousColumnUpdate>>;
pub type ContinuousSlotUpdateBatch = WithSessionId<Vec<QualifiedContinuousSlotUpdate>>;
