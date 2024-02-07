use std::panic::AssertUnwindSafe;

/// Executes the given function ignoring any panics by temporarily setting the panic hook
/// to nothing.
///
/// Should be used **very** sparingly.
pub fn ignore_panics(f: impl FnOnce()) {
    let old_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(old_panic_hook);
}
