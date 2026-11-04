use reaper_high::{Fx, Guid, Track};

pub fn convert_track_id_to_proto(track: &Track) -> String {
    track.guid().to_string_without_braces()
}

pub fn convert_fx_id_to_proto(fx: &Fx) -> Option<String> {
    Some(fx.get_or_query_guid().ok()?.to_string_without_braces())
}

pub fn convert_fx_id_to_engine(id: &str) -> anyhow::Result<Guid> {
    let guid = Guid::from_string_without_braces(id)?;
    Ok(guid)
}
