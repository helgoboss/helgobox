use crate::main::ClipContent;
use crate::ClipEngineResult;
use playtime_api as api;
use reaper_high::Project;
use reaper_medium::OwnedPcmSource;

pub fn load_source(
    source: &api::Source,
    project: Option<Project>,
) -> ClipEngineResult<OwnedPcmSource> {
    let content = ClipContent::load(source);
    Ok(content.create_source(project)?.into_raw())
}
