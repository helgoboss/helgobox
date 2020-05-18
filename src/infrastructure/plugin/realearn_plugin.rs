use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin::{CanDo, HostCallback, Info, Plugin};

use super::RealearnEditor;
use crate::domain::Session;
use reaper_high::{Reaper, ReaperGuard, ReaperSession};
use reaper_low::{reaper_vst_plugin, ReaperPluginContext, Swell};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use vst::api::Supported;

reaper_vst_plugin!();

/// # Design
///
/// ## Why `Rc<RefCell<Session>>`?
///
/// `Plugin#get_editor()` must return a Box of something 'static, so it's impossible to take a
/// reference here. Why? Because a reference needs a lifetime. Any non-static lifetime would
/// not satisfy the 'static requirement. Why not require a 'static reference then? Simply
/// because we don't have a session object with static lifetime. The session object is
/// owned by the `Plugin` object, which itself doesn't have a static lifetime. The only way
/// to get a 'static session would be to not let the plugin object own the session but to
/// define a static global. This, however, would be a far worse design than just using a
/// smart pointer here. So using a smart pointer is the best we can do really.
///
/// This is not the only reason why taking a reference here is not feasible. During the
/// lifecycle of a ReaLearn session we need mutable access to the session both from the
/// editor (of course) and from the plugin (e.g. when REAPER wants us to load some data).
/// When using references, Rust's borrow checker wouldn't let that happen. We can't do anything
/// about this multiple-access requirement, it's just how the VST plugin API works (and
/// many other similar plugin interfaces as well - for good reasons). And being a plugin we
/// have to conform.
///
/// Fortunately, we know that actually both DAW-plugin interaction (such as loading data) and
/// UI interaction happens in the main thread, in the so called main loop. So there's no
/// need for using a thread-safe smart pointer here. We even can and also should satisfy
/// the borrow checker, meaning that if the session is mutably accessed at a given point in
/// time, it is not accessed from another point as well. This can happen even in a
/// single-threaded environment because functions can call other functions and thereby
/// accessing the same data - just in different stack positions. Just think of reentrancy.
/// Fortunately this is something we can control. And we should, because when this kind of
/// parallel access happens, this can lead to strange bugs which are particularly hard to
/// find.
///
/// Unfortunately we can't make use of Rust's compile time borrow checker because there's no
/// way that the compiler understands what's going on here. Why? For one thing, because of
/// the VST plugin API design. But first and foremost because we use the FFI, which means
/// we interface with non-Rust code, so Rust couldn't get the complete picture even if the
/// plugin system would be designed in a different way. However, we *can* use Rust's
/// runtime borrow checker `RefCell`. And we should, because it gives us fail-fast
/// behavior. It will let us know immediately when we violated that safety rule.
/// TODO-low We must take care, however, that REAPER will not crash as a result, that would be
/// very  bad.  See https://github.com/RustAudio/vst-rs/issues/122
#[derive(Default)]
pub struct RealearnPlugin {
    host: HostCallback,
    session: Rc<RefCell<Session<'static>>>,
    reaper_guard: Option<Arc<ReaperGuard>>,
}

impl Plugin for RealearnPlugin {
    fn new(host: HostCallback) -> Self {
        Self {
            host,
            ..Default::default()
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "realearn-rs".to_string(),
            unique_id: 2964,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        let guard = ReaperSession::guarded(|| {
            let context = ReaperPluginContext::from_vst_plugin(
                &self.host,
                reaper_vst_plugin::static_context(),
            )
            .unwrap();
            Swell::make_available_globally(Swell::load(context));
            ReaperSession::setup_with_defaults(context, "info@helgoboss.org");
            let session = ReaperSession::get();
            session.activate();
            session
                .reaper()
                .show_console_msg(c_str!("Loaded realearn-rs VST plugin\n"));
        });
        self.reaper_guard = Some(guard);
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(RealearnEditor::new(self.session.clone())))
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        use CanDo::*;
        use Supported::*;
        match can_do {
            // If we don't do this, REAPER won't give us a SWELL parent window, which leads to a
            // horrible crash when doing CreateDialogParam.
            Other(s) if s == "hasCockosViewAsConfig" => Custom(0xbeef_0000),
            _ => Maybe,
        }
    }
}
