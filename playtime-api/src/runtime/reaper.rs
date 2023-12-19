#![allow(non_snake_case)]
use realearn_macros::reaper_api;

reaper_api![
    PlaytimeApi, PlaytimeApiPointers, PlaytimeApiSession, register_playtime_api
    {
        /// Finds the first Helgobox instance in the given project that contains a Playtime clip matrix.
        ///
        /// If the given project is `null`, it will look in the current project.
        ///
        /// Returns the instance ID or -1 if none exists.
        HB_FindFirstPlaytimeHelgoboxInstanceInProject(project: *mut reaper_low::raw::ReaProject) -> std::ffi::c_int;

        /// Creates a new Playtime clip matrix in the given Helgobox instance.
        HB_CreateClipMatrix(instance_id: std::ffi::c_int);

        /// Shows or hides the app for the given Helgobox instance and makes sure that the app displays
        /// Playtime.
        ///
        /// If necessary, this will also start the app and create a clip matrix for the given instance.
        HB_ShowOrHidePlaytime(instance_id: std::ffi::c_int);
    }
];
