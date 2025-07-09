use crate::persistence::{
    MatrixSequenceColumnMessage, MatrixSequenceEvent, MatrixSequenceMessage,
    MatrixSequenceRowMessage, MatrixSequenceSlotMessage, MatrixSequenceStartSlotMessage,
};
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Formatter;

/// A very compact representation of an event.
///
/// The compactness is achieved using tuple serialization.
///
/// Pro:
///
/// - If used with a CSV serializer, the outcome can be made look as "noise-less" as
///   REAPER's MIDI sequence format (newline to separate events, space delimiter, unquoted strings).
/// - If used with JSON/Lua serializer, the outcome is valid JSON/Lua while still being compact.
///   No need for string embedding (which looks especially bad in JSON due to newline escaping).
/// - If used with a binary serializer (e.g. bincode or msgpack), one can achieve a *really* compact
///   serialization that  also tops REAPER's MIDI sequence format. That will come in handy with
///   large undo histories or storage within RPP (in RPPs, we are base64-encoded, so the
///   human-readable-text advantage is not present anyway).
/// - TODO-high-ms3 Especially the last point could be desirable for MIDI sequences as well.
///   Use serde for them, too!
///
/// Contra:
///
/// - Not self-describing. However: If embedded in some `Serialize` wrapper that evaluates
///   `is_human_readable`, one could switch between this non-descriptive serialization style and a
///   (derived) descriptive serialization style. So we can have both if we want.
impl Serialize for MatrixSequenceEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use MatrixSequenceMessage::*;
        match self.message {
            PanicMatrix => {
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&0u8)?;
                seq.end()
            }
            StopMatrix => {
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&1u8)?;
                seq.end()
            }
            PanicColumn(m) => {
                let mut seq = serializer.serialize_tuple(3)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&2u8)?;
                seq.serialize_element(&m.index)?;
                seq.end()
            }
            StopColumn(m) => {
                let mut seq = serializer.serialize_tuple(3)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&3u8)?;
                seq.serialize_element(&m.index)?;
                seq.end()
            }
            StartScene(m) => {
                let mut seq = serializer.serialize_tuple(3)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&4u8)?;
                seq.serialize_element(&m.index)?;
                seq.end()
            }
            PanicSlot(m) => {
                let mut seq = serializer.serialize_tuple(4)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&5u8)?;
                seq.serialize_element(&m.column_index)?;
                seq.serialize_element(&m.row_index)?;
                seq.end()
            }
            StartSlot(m) => {
                let mut seq = serializer.serialize_tuple(5)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&6u8)?;
                seq.serialize_element(&m.column_index)?;
                seq.serialize_element(&m.row_index)?;
                seq.serialize_element(&m.velocity)?;
                seq.end()
            }

            StopSlot(m) => {
                let mut seq = serializer.serialize_tuple(4)?;
                seq.serialize_element(&self.pulse_diff)?;
                seq.serialize_element(&7u8)?;
                seq.serialize_element(&m.column_index)?;
                seq.serialize_element(&m.row_index)?;
                seq.end()
            }
        }
    }
}

struct MatrixSequenceEventVisitor;

impl<'de> Visitor<'de> for MatrixSequenceEventVisitor {
    type Value = MatrixSequenceEvent;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(formatter, "a tuple")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        use MatrixSequenceMessage::*;
        macro_rules! col(() => {
            MatrixSequenceColumnMessage {
                index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected column index"))?
            }
        });
        macro_rules! row(() => {
            MatrixSequenceRowMessage {
                index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected row index"))?
            }
        });
        macro_rules! slot(() => {
            MatrixSequenceSlotMessage {
                column_index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected slot column index"))?,
                row_index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected slot row index"))?,
            }
        });
        let pulse_diff: u32 = seq
            .next_element()?
            .ok_or(Error::custom("expected pulse diff"))?;
        let msg_type: u8 = seq
            .next_element()?
            .ok_or(Error::custom("expected message type"))?;
        let message = match msg_type {
            0 => PanicMatrix,
            1 => StopMatrix,
            2 => PanicColumn(col!()),
            3 => StopColumn(col!()),
            4 => StartScene(row!()),
            5 => PanicSlot(slot!()),
            6 => StartSlot(MatrixSequenceStartSlotMessage {
                column_index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected slot column index"))?,
                row_index: seq
                    .next_element()?
                    .ok_or(Error::custom("expected slot row index"))?,
                // Full velocity by default
                velocity: seq.next_element()?.unwrap_or(1.0),
            }),
            7 => StopSlot(slot!()),
            _ => return Err(Error::custom(format!("unknown message type {msg_type}"))),
        };
        let event = MatrixSequenceEvent {
            pulse_diff,
            message,
        };
        Ok(event)
    }
}

impl<'de> Deserialize<'de> for MatrixSequenceEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(MatrixSequenceEventVisitor)
    }
}
