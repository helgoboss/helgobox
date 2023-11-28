use reaper_low::raw::ReaProject;
use reaper_low::PluginContext;
use std::ffi::{c_char, c_long, c_void, CStr};
use std::mem::transmute;

macro_rules! api {
    ($func_name:ident ($( $param_name:ident: $param_type:ty ),*) -> $ret_type:ty) => {
        pub struct HelgoboxApiPointers {
            $func_name: Option<fn($( $param_name: $param_type ),*) -> $ret_type>,
        }

        impl HelgoboxApiPointers {
            pub fn load(plugin_context: &PluginContext) -> Self {
                unsafe {
                    Self {
                        $func_name: transmute(plugin_context.GetFunc(
                            concat!(stringify!($func_name), "\0").as_ptr() as *const c_char,
                        )),
                    }
                }
            }
        }

        pub struct HelgoboxApiSession {
            pointers: HelgoboxApiPointers,
        }

        impl HelgoboxApiSession {
            pub fn new(pointers: HelgoboxApiPointers) -> Self {
                Self {
                    pointers
                }
            }

            pub fn $func_name(&self, $( $param_name: $param_type ),*) -> $ret_type {
                self.pointers.$func_name.unwrap()($( $param_name ),*)
            }
        }

        pub trait HelgoboxApi {
            extern "C" fn $func_name($( $param_name: $param_type ),*) -> $ret_type;
        }

        pub fn register_helgobox_api<T: HelgoboxApi>(mut register_api_fn: impl FnMut(&CStr, *mut c_void)) {
            unsafe {
                register_api_fn(
                    CStr::from_ptr(concat!(stringify!($func_name), "\0").as_ptr() as *const c_char),
                    T::$func_name as *mut c_void,
                );
            }
        }
    };
}

api![
    HB_FindFirstInstanceInProject(project: *const ReaProject) -> c_long
];
