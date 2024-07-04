#![allow(non_snake_case)]
use helgobox_macros::reaper_api;

reaper_api![
    HelgoboxApi, HelgoboxApiPointers, HelgoboxApiSession, register_helgobox_api
    {
        /// Finds the first Helgobox instance in the given project.
        ///
        /// If the given project is `null`, it will look in the current project.
        ///
        /// Returns the instance ID or -1 if none exists.
        HB_FindFirstHelgoboxInstanceInProject(project: *mut reaper_low::raw::ReaProject) -> std::ffi::c_int;
    }
];
