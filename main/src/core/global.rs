/// Use only where absolutely necessary because of static-only FFI stuff!
macro_rules! make_available_globally_in_main_thread {
    ($instance_struct:path) => {
        impl $instance_struct {
            /// Panics if not in main thread.
            pub fn get() -> &'static $instance_struct {
                /// static mut hopefully okay because we access this via `Foo::get()` function only
                /// and this one checks the thread before returning the reference.
                // TODO-high This might be wrong because the thread is checked *after* the cell in
                //   unsync::Lazy has been accessed!!! Maybe std::sync::Once::call_once() should be
                // used   instead.
                static mut GLOBAL: once_cell::unsync::Lazy<$instance_struct> =
                    once_cell::unsync::Lazy::new(Default::default);
                reaper_high::Reaper::get().require_main_thread();
                unsafe { &GLOBAL }
            }
        }
    };
}
