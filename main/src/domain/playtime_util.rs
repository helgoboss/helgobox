use crate::domain::{
    transport_is_enabled_unit_value, Backbone, CompartmentKind, ExtendedProcessorContext,
    TrackResolveError, VirtualPlaytimeColumn,
};
use helgoboss_learn::UnitValue;
use realearn_api::persistence::ClipColumnTrackContext;
use reaper_high::Track;

/// In clip slot targets, the resolve phase makes sure that the targeted slot actually exists.
/// So if we get a `None` value from some of the clip slot methods, it's because the slot doesn't
/// have a clip, which is a valid state and should return *something*. The contract of the target
/// `current_value()` is that if it returns `None`, it means it can't get a value at the moment,
/// probably just temporarily. In that case, the feedback is simply not updated.
pub fn interpret_current_clip_slot_value<T: Default>(value: Option<T>) -> Option<T> {
    Some(value.unwrap_or_default())
}

pub fn resolve_virtual_track_by_playtime_column(
    context: ExtendedProcessorContext,
    compartment: CompartmentKind,
    column: &VirtualPlaytimeColumn,
    track_context: &ClipColumnTrackContext,
) -> Result<Vec<Track>, TrackResolveError> {
    // TODO-low Not very helpful. Do we even use this error type?
    let generic_error = || TrackResolveError::TrackNotFound {
        guid: None,
        name: None,
        index: None,
    };
    let clip_column_index = column
        .resolve(context, compartment)
        .map_err(|_| generic_error())?;
    let track = Backbone::get()
        .with_clip_matrix(context.control_context.instance(), |matrix| {
            let column = matrix.get_column(clip_column_index)?;
            match track_context {
                ClipColumnTrackContext::Playback => column.playback_track().cloned(),
                ClipColumnTrackContext::Recording => column.effective_recording_track(),
            }
        })
        .map_err(|_| generic_error())?
        .map_err(|_| generic_error())?;
    Ok(vec![track])
}

// Panics if called with repeat or record.
#[cfg(feature = "playtime")]
pub(crate) fn clip_play_state_unit_value(
    action: realearn_api::persistence::PlaytimeSlotTransportAction,
    play_state: playtime_clip_engine::rt::ClipPlayState,
) -> UnitValue {
    use playtime_clip_engine::rt::ClipPlayState;
    use realearn_api::persistence::PlaytimeSlotTransportAction::*;
    match action {
        Trigger | PlayStop | PlayPause | RecordPlayStop => play_state.feedback_value(),
        Stop => transport_is_enabled_unit_value(matches!(
            play_state,
            ClipPlayState::Stopped | ClipPlayState::Ignited
        )),
        Pause => transport_is_enabled_unit_value(play_state == ClipPlayState::Paused),
        RecordStop | OverdubPlay => {
            transport_is_enabled_unit_value(play_state.is_and_will_keep_recording())
        }
        Looped => panic!("wrong argument"),
    }
}
