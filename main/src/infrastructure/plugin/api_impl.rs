use crate::domain::BackboneState;
use crate::infrastructure::plugin::App;
use anyhow::Context;
use itertools::Itertools;
use realearn_api::runtime::{register_helgobox_api, HelgoboxApi};
use reaper_high::{OrCurrentProject, Project, Reaper};
use reaper_low::raw::ReaProject;
use reaper_medium::{ReaperFunctionError, ReaperStr, RegistrationObject};
use std::borrow::Cow;
use std::ffi::c_int;

struct HelgoboxApiImpl;

impl HelgoboxApi for HelgoboxApiImpl {
    extern "C" fn HB_FindFirstPlaytimeInstanceInProject(project: *mut ReaProject) -> c_int {
        find_first_playtime_instance_in_project(project).unwrap_or(-1)
    }

    extern "C" fn HB_CreateClipMatrix(instance_id: c_int) {
        let _ = create_clip_matrix(instance_id);
    }

    extern "C" fn HB_ShowOrHidePlaytime(instance_id: c_int) {
        let _ = show_or_hide_playtime(instance_id);
    }
}

fn find_first_playtime_instance_in_project(project: *mut ReaProject) -> anyhow::Result<c_int> {
    let project = reaper_medium::ReaProject::new(project)
        .map(Project::new)
        .or_current_project();
    let instance_id = App::get()
        .with_instances(|instances| {
            instances
                .iter()
                .filter_map(|instance| {
                    let instance_project = instance.processor_context.project()?;
                    if instance_project != project {
                        return None;
                    }
                    let instance_state = instance.instance_state.upgrade()?;
                    let instance_state = instance_state.borrow();
                    instance_state.owned_clip_matrix()?;
                    let track_index = instance.processor_context.track()?.index()?;
                    Some((track_index, instance))
                })
                .sorted_by_key(|(track_index, _)| *track_index)
                .next()
                .map(|(_, instance)| instance.instance_id)
        })
        .context("Project doesn't contain Helgobox instance with a Playtime Clip Matrix")?;
    Ok(u32::from(instance_id) as _)
}

fn create_clip_matrix(instance_id: c_int) -> anyhow::Result<()> {
    let instance_id = u32::try_from(instance_id)?.into();
    let instance_state = App::get()
        .with_instances(|instance| {
            instance
                .iter()
                .find(|i| i.instance_id == instance_id)
                .and_then(|i| i.instance_state.upgrade())
        })
        .context("Instance not found")?;
    BackboneState::get()
        .get_or_insert_owned_clip_matrix_from_instance_state(&mut instance_state.borrow_mut());
    Ok(())
}

fn show_or_hide_playtime(instance_id: c_int) -> anyhow::Result<()> {
    let instance_id = u32::try_from(instance_id)?;
    let main_panel = App::get()
        .find_main_panel_by_instance_id(instance_id.into())
        .context("Instance not found")?;
    main_panel.start_show_or_hide_app_instance()
}

pub fn register_api() -> anyhow::Result<()> {
    let mut session = Reaper::get().medium_session();
    register_helgobox_api::<HelgoboxApiImpl, ReaperFunctionError>(|name, ptr| unsafe {
        session.plugin_register_add(RegistrationObject::Api(
            Cow::Borrowed(ReaperStr::from_ptr(name.as_ptr())),
            ptr,
        ))?;
        Ok(())
    })?;
    Ok(())
}
