use crate::base::clip_manifestation::ClipOnTrackManifestation;

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
    clip_manifestation: ClipOnTrackManifestation,
    midi_editor_window: Window,
    previous_content_hash: Option<u64>,
}

impl MidiClipEditSession {
    pub fn new(clip_manifestation: ClipOnTrackManifestation, midi_editor_window: Hwnd) -> Self {
        Self {
            midi_editor_window: Window::from_non_null(midi_editor_window),
            previous_content_hash: clip_manifestation.source_hash().ok(),
            clip_manifestation,
        }
    }

    pub fn midi_editor_window(&self) -> Window {
        self.midi_editor_window
    }

    pub fn clip_manifestation(&self) -> &ClipOnTrackManifestation {
        &self.clip_manifestation
    }

    /// Returns `true` if hash has changed.
    pub fn update_source_hash(&mut self) -> bool {
        let new_hash = self.clip_manifestation.source_hash().ok();
        if new_hash != self.previous_content_hash {
            self.previous_content_hash = new_hash;
            true
        } else {
            false
        }
    }
}
