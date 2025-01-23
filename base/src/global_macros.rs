/// Use only where absolutely necessary because of static-only FFI stuff!
#[macro_export]
macro_rules! make_available_globally_in_main_thread_on_demand {
    ($instance_struct:path) => {
        static INSTANCE: std::sync::OnceLock<fragile::Fragile<$instance_struct>> =
            std::sync::OnceLock::new();

        impl $instance_struct {
            pub fn make_available_globally(create_instance: impl FnOnce() -> $instance_struct) {
                if INSTANCE.get().is_some() {
                    return;
                }
                let _ = INSTANCE.set(fragile::Fragile::new(create_instance()));
            }

            /// Whether this instance is (already/still) loaded.
            pub fn is_loaded() -> bool {
                INSTANCE.get().is_some()
            }

            /// Panics if not in main thread.
            pub fn get() -> &'static $instance_struct {
                INSTANCE
                    .get()
                    .expect("call `make_available_globally()` before using `get()`")
                    .get()
            }
        }
    };
}
