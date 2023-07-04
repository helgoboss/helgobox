use objc2::foundation::{MainThreadMarker, NSObject};
use objc2::rc::{Id, Shared};
use objc2::runtime::Class;
use objc2::{extern_class, extern_methods, msg_send, msg_send_id, ClassType};

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

pub(crate) fn ns_app() -> Id<NSApplication, Shared> {
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
    pub(crate) struct NSWindow;

    unsafe impl ClassType for NSWindow {
        #[inherits(NSObject)]
        type Super = NSResponder;
    }
);

extern_methods!(
    unsafe impl NSWindow {
        #[sel(sendEvent:)]
        pub unsafe fn send_event(&self, event: &NSEvent);

        pub fn content_view(&self) -> Option<Id<NSView, Shared>> {
            unsafe { msg_send_id![self, contentView] }
        }

        pub fn child_windows(&self) -> Id<NSArrayOfWindows, Shared> {
            unsafe { msg_send_id![self, childWindows] }
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
        pub fn is_kind_of_class(&self, class: &Class) -> bool {
            unsafe { msg_send![self, isKindOfClass: class] }
        }

        pub fn window(&self) -> Option<Id<NSWindow, Shared>> {
            unsafe { msg_send_id![self, window] }
        }
    }
);

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSArrayOfWindows;

    unsafe impl ClassType for NSArrayOfWindows {
        #[inherits(NSObject)]
        type Super = NSResponder;
    }
);

extern_methods!(
    unsafe impl NSArrayOfWindows {
        pub fn first_object(&self) -> Option<Id<NSWindow, Shared>> {
            unsafe { msg_send_id![self, firstObject] }
        }
    }
);
