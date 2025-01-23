/// Use only where absolutely necessary because of static-only FFI stuff!
// TODO-medium Make available in reaper-rs
#[macro_export]
macro_rules! make_available_globally_in_main_thread {
    ($instance_struct:path) => {
        impl $instance_struct {
            /// Panics if not in main thread.
            pub fn get() -> &'static $instance_struct {
                // This is safe (see https://doc.rust-lang.org/std/sync/struct.Once.html#examples-1).
                static mut INSTANCE: Option<$instance_struct> = None;
                static INIT_INSTANCE: std::sync::Once = std::sync::Once::new();
                reaper_high::Reaper::get().require_main_thread();
                unsafe {
                    INIT_INSTANCE.call_once(|| {
                        INSTANCE = Some(Default::default());
                        reaper_low::register_plugin_destroy_hook(|| INSTANCE = None);
                    });
                    INSTANCE.as_ref().unwrap()
                }
            }
        }
    };
}

/// Use only where absolutely necessary because of static-only FFI stuff!
#[macro_export]
macro_rules! make_available_globally_in_main_thread_on_demand {
    ($instance_struct:path) => {
        // This is safe (see https://doc.rust-lang.org/std/sync/struct.Once.html#examples-1).
        // TODO-high CONTINUE Use new once_cell-like stuff in std instead
        static mut INSTANCE: Option<$instance_struct> = None;

        impl $instance_struct {
            pub fn make_available_globally(create_instance: impl FnOnce() -> $instance_struct) {
                static INIT_INSTANCE: std::sync::Once = std::sync::Once::new();
                unsafe {
                    INIT_INSTANCE.call_once(|| {
                        INSTANCE = Some(create_instance());
                        reaper_low::register_plugin_destroy_hook(reaper_low::PluginDestroyHook {
                            name: stringify!($instance_struct),
                            callback: || INSTANCE = None,
                        });
                    });
                }
            }

            /// Whether this instance is (already/still) loaded.
            pub fn is_loaded() -> bool {
                unsafe { INSTANCE.is_some() }
            }

            /// Panics if not in main thread.
            pub fn get() -> &'static $instance_struct {
                reaper_high::Reaper::get().require_main_thread();
                unsafe {
                    INSTANCE
                        .as_ref()
                        .expect("call `make_available_globally()` before using `get()`")
                }
            }
        }
    };
}

/// Use only where absolutely necessary because of static-only FFI stuff!
///
/// The given struct must be thread-safe. If not, all of its public methods should first check if
/// the thread is correct.
#[macro_export]
macro_rules! make_available_globally_in_any_non_rt_thread {
    ($instance_struct:path) => {
        impl $instance_struct {
            pub fn get() -> &'static $instance_struct {
                assert!(
                    !reaper_high::Reaper::get()
                        .medium_reaper()
                        .is_in_real_time_audio(),
                    "this function must not be called in a real-time thread"
                );
                // This is safe (see https://doc.rust-lang.org/std/sync/struct.Once.html#examples-1).
                static mut INSTANCE: Option<$instance_struct> = None;
                static INIT_INSTANCE: std::sync::Once = std::sync::Once::new();
                unsafe {
                    INIT_INSTANCE.call_once(|| {
                        INSTANCE = Some(Default::default());
                        reaper_low::register_plugin_destroy_hook(reaper_low::PluginDestroyHook {
                            name: stringify!($instance_struct),
                            callback: || INSTANCE = None,
                        });
                    });
                    INSTANCE.as_ref().unwrap()
                }
            }
        }
    };
}
