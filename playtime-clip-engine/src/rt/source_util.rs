use reaper_medium::BorrowedPcmSource;

pub fn pcm_source_is_midi(src: &BorrowedPcmSource) -> bool {
    get_pcm_source_type(src).is_midi()
}

pub fn get_pcm_source_type(src: &BorrowedPcmSource) -> PcmSourceType {
    use PcmSourceType::*;
    src.get_type(|t| match t.to_str() {
        "MIDI" => NormalMidi,
        "MIDIPOOL" => PooledMidi,
        "WAVE" => Wave,
        _ => Unknown,
    })
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PcmSourceType {
    NormalMidi,
    PooledMidi,
    Wave,
    Unknown,
}

impl PcmSourceType {
    pub fn is_midi(&self) -> bool {
        use PcmSourceType::*;
        matches!(self, NormalMidi | PooledMidi)
    }
}
