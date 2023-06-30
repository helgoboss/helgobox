use crate::proto::{
    ContinuousColumnUpdate, ContinuousMatrixUpdate, OccasionalMatrixUpdate,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate,
    QualifiedOccasionalRowUpdate, QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
};
use tokio::sync::broadcast::Sender;

/// This must be a global object because it's responsible for supplying one gRPC endpoint with
/// streaming data and we have only one endpoint for all matrices.
#[derive(Clone, Debug)]
pub struct ClipEngineSenders {
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

impl ClipEngineSenders {
    pub fn new() -> Self {
        Self {
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
}

#[derive(Clone)]
pub struct WithSessionId<T> {
    pub session_id: String,
    pub value: T,
}

pub type OccasionalMatrixUpdateBatch = WithSessionId<Vec<OccasionalMatrixUpdate>>;
pub type OccasionalTrackUpdateBatch = WithSessionId<Vec<QualifiedOccasionalTrackUpdate>>;
pub type OccasionalColumnUpdateBatch = WithSessionId<Vec<QualifiedOccasionalColumnUpdate>>;
pub type OccasionalRowUpdateBatch = WithSessionId<Vec<QualifiedOccasionalRowUpdate>>;
pub type OccasionalSlotUpdateBatch = WithSessionId<Vec<QualifiedOccasionalSlotUpdate>>;
pub type OccasionalClipUpdateBatch = WithSessionId<Vec<QualifiedOccasionalClipUpdate>>;
pub type ContinuousMatrixUpdateBatch = WithSessionId<ContinuousMatrixUpdate>;
pub type ContinuousColumnUpdateBatch = WithSessionId<Vec<ContinuousColumnUpdate>>;
pub type ContinuousSlotUpdateBatch = WithSessionId<Vec<QualifiedContinuousSlotUpdate>>;
