use reaper_high::Reaper;
use reaper_medium::{
    AccelMsgKind, AcceleratorBehavior, AcceleratorKeyCode, AcceleratorPosition, RegistrationHandle,
    TranslateAccel, TranslateAccelArgs, TranslateAccelResult,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct EscapeCatcher {
    accel_handle: RegistrationHandle<EscapeCatcherAccel>,
    escape_was_pressed: Arc<AtomicBool>,
}

impl TranslateAccel for EscapeCatcherAccel {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        let is_escape = args.msg.behavior().contains(AcceleratorBehavior::VirtKey)
            && args.msg.message() == AccelMsgKind::KeyDown
            && args.msg.key() == AcceleratorKeyCode::new(0x1b);
        if !is_escape {
            return TranslateAccelResult::NotOurWindow;
        }
        self.escape_was_pressed.store(true, Ordering::Relaxed);
        TranslateAccelResult::Eat
    }
}

struct EscapeCatcherAccel {
    escape_was_pressed: Arc<AtomicBool>,
}

impl EscapeCatcher {
    pub fn new() -> Self {
        let escape_was_pressed = Arc::new(AtomicBool::new(false));
        let accel = EscapeCatcherAccel {
            escape_was_pressed: escape_was_pressed.clone(),
        };
        let accel_handle = Reaper::get()
            .medium_session()
            .plugin_register_add_accelerator_register(Box::new(accel), AcceleratorPosition::Front)
            .expect("couldn't register escape accelerator");
        Self {
            accel_handle,
            escape_was_pressed,
        }
    }

    pub fn escape_was_pressed(&self) -> bool {
        self.escape_was_pressed.load(Ordering::Relaxed)
    }
}

impl Drop for EscapeCatcher {
    fn drop(&mut self) {
        Reaper::get()
            .medium_session()
            .plugin_register_remove_accelerator(self.accel_handle)
            .expect("escape accelerator was not registered");
    }
}
