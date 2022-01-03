use reaper_medium::{reaper_str, BorrowedPcmSource};

pub fn pcm_source_is_midi(src: &BorrowedPcmSource) -> bool {
    src.get_type(|t| t == reaper_str!("MIDI"))
}
