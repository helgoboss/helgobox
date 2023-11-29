use realearn_api::runtime::{register_helgobox_api, HelgoboxApi};
use reaper_high::Reaper;
use reaper_low::raw::ReaProject;
use reaper_medium::{ReaperStr, RegistrationObject};
use std::borrow::Cow;
use std::ffi::c_long;

struct HelgoboxApiImpl;

impl HelgoboxApi for HelgoboxApiImpl {
    extern "C" fn HB_FindFirstInstanceInProject(project: *const ReaProject) -> c_long {
        42
    }

    extern "C" fn HB_ShowOrHidePlaytime(instance_id: c_long) -> () {
        Reaper::get().show_console_msg("Show or hide playtime");
    }
}

pub fn register_api() {
    let mut session = Reaper::get().medium_session();
    register_helgobox_api::<HelgoboxApiImpl>(|name, ptr| unsafe {
        session.plugin_register_add(RegistrationObject::Api(
            Cow::Borrowed(ReaperStr::from_ptr(name.as_ptr())),
            ptr,
        ));
    });
}

// Finds the first Helgobox instance in the given project.
//
// If the given project is `null`, it will look in the current project.
//
// Returns the instance ID or -1 if none exists.

// Shows or hides the app for the given Helgobox instance and makes sure that the app displays
// Playtime.
//
// If necessary, this will also start the app and create a clip matrix for the given instance.
