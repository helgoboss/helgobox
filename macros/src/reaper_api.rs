#[macro_export]
macro_rules! reaper_api {
    (
        $trait_name:ident, $pointer_struct_name:ident, $session_struct_name:ident, $reg_func_name:ident
        {
            $(
                $( #[doc = $doc:expr] )*
                $func_name:ident ($( $param_name:ident: $param_type:ty ),*) $( -> $ret_type:ty )?;
            )+
        }
    ) => {
        #[derive(Default)]
        pub struct $pointer_struct_name {
            $(
                $func_name: Option<fn($( $param_name: $param_type ),*) $( -> $ret_type )?>
            ),+
        }

        impl $pointer_struct_name {
            pub fn load(plugin_context: &reaper_low::PluginContext) -> Option<Self> {
                let mut pointers = Self::default();
                let mut load_count = 0;
                unsafe {
                    $(
                        pointers.$func_name = std::mem::transmute(plugin_context.GetFunc(
                            concat!(stringify!($func_name), "\0").as_ptr() as *const std::ffi::c_char,
                        ));
                        if pointers.$func_name.is_some() {
                            load_count += 1;
                        }
                    )+
                }
                if load_count == 0 {
                    return None;
                }
                Some(pointers)
            }
        }

        pub struct $session_struct_name {
            pointers: $pointer_struct_name,
        }

        impl $session_struct_name {
            pub fn load(plugin_context: &reaper_low::PluginContext) -> Option<Self> {
                $pointer_struct_name::load(plugin_context).map(Self::new)
            }

            pub fn new(pointers: $pointer_struct_name) -> Self {
                Self {
                    pointers
                }
            }

            $(
                $( #[doc = $doc] )*
                pub fn $func_name(&self, $( $param_name: $param_type ),*) $( -> $ret_type )? {
                    self.pointers.$func_name.unwrap()($( $param_name ),*)
                }
            )+
        }

        pub trait $trait_name {
            $(
                extern "C" fn $func_name($( $param_name: $param_type ),*) $( -> $ret_type )?;
            )+
        }

        pub fn $reg_func_name<T: $trait_name, E>(mut register_api_fn: impl FnMut(&std::ffi::CStr, *mut std::ffi::c_void) -> Result<(), E> ) -> Result<(), E> {
            unsafe {
                $(
                    register_api_fn(
                        std::ffi::CStr::from_ptr(concat!(stringify!($func_name), "\0").as_ptr() as *const std::ffi::c_char),
                        T::$func_name as *mut std::ffi::c_void,
                    )?;
                )+
            }
            Ok(())
        }
    };
}
