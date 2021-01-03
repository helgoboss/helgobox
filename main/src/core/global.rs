/// Use only where absolutely necessary because of static-only FFI stuff!
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
                    INIT_INSTANCE.call_once(|| INSTANCE = Some(Default::default()));
                    INSTANCE.as_ref().unwrap()
                }
            }
        }
    };
}
