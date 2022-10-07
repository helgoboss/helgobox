use objc2::foundation::{MainThreadMarker, NSObject};
use objc2::rc::{Id, Shared};
use objc2::{class, extern_class, extern_methods, msg_send, msg_send_id, ClassType};

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSApplication;

    unsafe impl ClassType for NSApplication {
        #[inherits(NSObject)]
        type Super = NSResponder;
    }
);

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSResponder;

    unsafe impl ClassType for NSResponder {
        type Super = NSObject;
    }
);

pub(crate) fn NSApp() -> Id<NSApplication, Shared> {
    // TODO: Only allow access from main thread
    NSApplication::shared(unsafe { MainThreadMarker::new_unchecked() })
}

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSEvent;

    unsafe impl ClassType for NSEvent {
        type Super = NSObject;
    }
);

extern_methods!(
    unsafe impl NSApplication {
        /// This can only be called on the main thread since it may initialize
        /// the application and since it's parameters may be changed by the main
        /// thread at any time (hence it is only safe to access on the main thread).
        pub fn shared(_mtm: MainThreadMarker) -> Id<Self, Shared> {
            let app: Option<_> = unsafe { msg_send_id![Self::class(), sharedApplication] };
            // SAFETY: `sharedApplication` always initializes the app if it isn't already
            unsafe { app.unwrap_unchecked() }
        }

        pub fn current_event(&self) -> Option<Id<NSEvent, Shared>> {
            unsafe { msg_send_id![self, currentEvent] }
        }
    }
);

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSView;

    unsafe impl ClassType for NSView {
        #[inherits(NSObject)]
        type Super = NSResponder;
    }
);

extern_methods!(
    unsafe impl NSView {
        #[sel(sendEvent:)]
        pub unsafe fn send_event(&self, event: &NSEvent);

        pub unsafe fn is_text_field(&self) -> bool {
            let cls = class!(NSTextField);
            msg_send![self, isKindOfClass: cls]
        }
    }
);
