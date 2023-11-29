use realearn_api::runtime::{register_helgobox_api, HelgoboxApi};
use reaper_high::Reaper;
use reaper_low::raw::ReaProject;
use reaper_medium::{ReaperFunctionError, ReaperStr, RegistrationObject};
use std::borrow::Cow;
use std::ffi::c_int;

struct HelgoboxApiImpl;

impl HelgoboxApi for HelgoboxApiImpl {
    extern "C" fn HB_FindFirstInstanceInProject(project: *const ReaProject) -> c_int {
        42
    }

    extern "C" fn HB_ShowOrHidePlaytime(instance_id: c_int) {
        Reaper::get().show_console_msg("Show or hide playtime");
    }
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
