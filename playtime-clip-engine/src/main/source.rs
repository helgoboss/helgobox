use crate::main::ClipContent;
use crate::ClipEngineResult;
use playtime_api as api;
use reaper_high::Project;
use reaper_medium::OwnedPcmSource;

/// The temporary project will be used to make the path relative.
pub fn load_source(
    source: &api::Source,
    project_for_relative_path: Option<Project>,
) -> ClipEngineResult<OwnedPcmSource> {
    let content = ClipContent::load(source);
    Ok(content.create_source(project_for_relative_path)?.into_raw())
}
