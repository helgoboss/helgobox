use crate::base::clip_manifestation::ClipOnTrackManifestation;
use reaper_high::Track;
use reaper_low::Swell;
use reaper_medium::Hwnd;
use swell_ui::Window;

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
