use crate::base::clip_manifestation::ClipOnTrackManifestation;
use reaper_high::Track;
use reaper_medium::Hwnd;

#[derive(Clone, Debug)]
pub enum ClipEditSession {
    Audio(AudioClipEditSession),
    Midi(MidiClipEditSession),
}

#[derive(Clone, Debug)]
pub struct AudioClipEditSession {
    pub clip_manifestation: ClipOnTrackManifestation,
}

#[derive(Clone, Debug)]
pub struct MidiClipEditSession {
    pub midi_editor_window: Hwnd,
    pub clip_manifestation: ClipOnTrackManifestation,
}
