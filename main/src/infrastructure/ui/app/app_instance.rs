use crate::domain::{InstanceId, UnitId};
use crate::infrastructure::proto::Reply;
use crate::infrastructure::ui::{
    AppCallback, InProcessStandaloneAppInstance, SeparateProcessAppInstance,
};
use anyhow::Result;
use reaper_medium::Hwnd;
use std::cell::RefCell;
use std::fmt::{Debug, Display, Formatter};
use std::rc::Rc;
use swell_ui::Window;

pub type SharedAppInstance = Rc<RefCell<dyn AppInstance>>;

pub trait AppInstance: Debug {
    fn is_running(&self) -> bool;

    fn has_focus(&self) -> bool;

    fn is_visible(&self) -> bool;

    fn start_or_show(&mut self, owning_window: Window, location: Option<AppPage>) -> Result<()>;

    fn hide(&mut self) -> Result<()>;

    fn stop(&mut self) -> Result<()>;

    fn send(&self, reply: &Reply) -> Result<()>;

    /// Only relevant and called for in-process app instances.
    fn notify_app_is_ready(&mut self, callback: AppCallback);

    /// Returns the app window.
    ///
    /// On Windows, that is the app handle (HWND), which is the **parent** window of whatever REAPER passes into
    /// the `HwndInfo` hook.
    ///
    /// On macOS, this is the content view (NSView) of the app handle (NSWindow), which is exactly what
    /// REAPER passes into the `HwndInfo` hook.
    fn window(&self) -> Option<Hwnd>;

    /// Only relevant and called for in-process app instances.
    fn notify_app_is_in_text_entry_mode(&mut self, value: bool) {
        let _ = value;
    }

    /// Only relevant and called for separate-process app instances.
    fn notify_app_has_focus(&mut self, value: bool) {
        let _ = value;
    }
}

#[allow(clippy::if_same_then_else)]
pub fn create_shared_app_instance(instance_id: InstanceId) -> SharedAppInstance {
    fn share(value: impl AppInstance + 'static) -> SharedAppInstance {
        Rc::new(RefCell::new(value))
    }
    // I was experimenting with 2 different ways of embedding the app GUI into REAPER:
    //
    // - Parented mode: We create a new SWELL window on ReaLearn side (HWND on Windows, NSView on
    //   macOS) and the app renders its GUI *within* it.
    // - Standalone mode: The app fires up its own window.
    //
    // Embedding the app in "parented" mode is in theory preferable because:
    //
    // 1. Only "parented" mode makes it possible to dock the app GUI (in case we want to do that
    //    one day).
    // 2. ReaLearn has full control over the window and can listen to its events.
    // 3. We stop sending events when the app window is hidden, not wasting resources when the
    //    app is not shown anyway. (However, with just a bit more effort, we could implement this
    //    for standalone mode as well.)
    if cfg!(target_os = "windows") {
        // On Windows, parented mode works in general. With a few tricks (see AppPanel View
        // implementation). However, I had issues completely removing the window title bar, maybe
        // because SWELL windows are dialog windows and they work differently? Anyway, this was
        // the reason that I switched to a standalone window.
        // let app_panel = AppPanel::new(session);
        // let instance = ParentedAppInstance {
        //     panel: SharedView::new(app_panel),
        // };
        // share(instance)
        share(InProcessStandaloneAppInstance::new(instance_id))
        // share(SeparateProcessAppInstance::new(instance_id))
    } else if cfg!(target_os = "macos") {
        // On macOS, parented mode is possible only by using Cocoa child windows (see app side
        // embedding docs). This means that the app doesn't really render itself in the NSView
        // provided by ReaLearn but places an NSWindow on top of the NSWindow provided by ReaLearn.
        // It works but it needs a few keyboard tricks and - most importantly - it doesn't work
        // well if the app itself wants to control its window (e.g. going full screen or changing
        // the window opacity). It will try to control the child window, not the outer window.
        // This could be solved on app side by navigating up the child/parent window chain, but
        // it's not something I want to do now as long as we don't support docking anyway.
        // Therefore: Standalone mode on macOS!
        share(InProcessStandaloneAppInstance::new(instance_id))
    } else {
        // share(InProcessStandaloneAppInstance::new(instance_id))
        share(SeparateProcessAppInstance::new(instance_id))
    }
}

pub enum AppPage {
    Projection(UnitId),
    Playtime,
}

pub enum InstanceRef {
    Host,
    Id(InstanceId),
}

impl Display for InstanceRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceRef::Host => f.write_str(".host"),
            InstanceRef::Id(id) => std::fmt::Display::fmt(&id, f),
        }
    }
}

impl AppPage {
    pub fn location(&self, instance_ref: InstanceRef) -> String {
        match self {
            AppPage::Playtime => format!("/instance/{instance_ref}/playtime"),
            AppPage::Projection(unit_id) => {
                format!("/instance/{instance_ref}/unit/{unit_id}/projection")
            }
        }
    }
}
