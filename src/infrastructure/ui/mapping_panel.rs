use crate::domain::{MidiControlInput, MidiFeedbackOutput, Session, SharedMappingModel};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_low::Swell;
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use rx_util::{LocalProp, UnitEvent};
use rxrust::prelude::*;
use std::cell::{Cell, Ref, RefCell};
use std::ffi::CString;
use std::iter;
use std::rc::{Rc, Weak};
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
pub struct MappingPanel {
    view: ViewContext,
    session: SharedSession,
    mapping: SharedMappingModel,
}

impl MappingPanel {
    pub fn new(session: SharedSession, mapping: SharedMappingModel) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session,
            mapping,
        }
    }
}

impl MappingPanel {
    pub fn mapping(&self) -> &SharedMappingModel {
        &self.mapping
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when_async(event, reaction, &self, self.view.closed());
    }
}

impl View for MappingPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        // TODO
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            _ => {}
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            _ => {}
        }
    }
}
