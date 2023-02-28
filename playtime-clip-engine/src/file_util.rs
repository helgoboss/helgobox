use reaper_high::{Project, Reaper};
use std::path::PathBuf;

pub fn get_path_for_new_media_file(
    file_base_name: &str,
    file_extension_without_dot: &str,
    project: Option<Project>,
) -> PathBuf {
    let project = project.unwrap_or_else(|| Reaper::get().current_project());
    let recording_path = project.recording_path();
    let name_slug = slug::slugify(file_base_name);
    let unique_id = nanoid::nanoid!(8);
    let file_name = format!("{name_slug}-{unique_id}.{file_extension_without_dot}");
    recording_path.join(file_name)
}
