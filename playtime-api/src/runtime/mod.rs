use serde::{Deserialize, Serialize};

// We don't really need a tagged enum here but it's an easy way to transmit the event as a
// JSON object (vs. just a string) ... which is better for some clients.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum MatrixInfoEvent {
    RecordedMatrixSequence,
    DiscardedMatrixSequenceBecauseEmpty,
    RemovedMatrixSequence,
    WroteMatrixSequenceToArrangement,
}
