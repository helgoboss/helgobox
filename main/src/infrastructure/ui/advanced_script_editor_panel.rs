use crate::base::{blocking_lock, SenderToNormalThread};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views::advanced_script_editor;
use crate::infrastructure::ui::egui_views::advanced_script_editor::{
    SharedValue, State, Toolbox, Value,
};
use crate::infrastructure::ui::{egui_views, ScriptEditorInput};
use crossbeam_channel::Receiver;
use derivative::Derivative;
use reaper_low::raw;
use semver::Version;
use std::cell::RefCell;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct ScriptTemplateGroup {
    pub name: &'static str,
    pub templates: &'static [ScriptTemplate],
}

#[derive(Debug)]
pub struct ScriptTemplate {
    pub min_realearn_version: Option<Version>,
    pub name: &'static str,
    pub description: &'static str,
    pub control_styles: &'static [ControlStyle],
    pub content: &'static str,
}

#[derive(Copy, Clone, Debug, derive_more::Display)]
pub enum ControlStyle {
    #[display(fmt = "range elements")]
    RangeElement,
    #[display(fmt = "buttons")]
    Button,
}

impl ControlStyle {
    pub fn examples(&self) -> &'static str {
        match self {
            ControlStyle::RangeElement => {
                "knobs, faders, touch strips, wheels, aftertouch, velocity"
            }
            ControlStyle::Button => "buttons, pads, keys",
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct AdvancedScriptEditorPanel {
    view: ViewContext,
    state: RefCell<Option<State>>,
    value_receiver: Receiver<SharedValue>,
    #[derivative(Debug = "ignore")]
    set_value: Box<dyn Fn(Value)>,
}
const TIMER_ID: usize = 321;

impl AdvancedScriptEditorPanel {
    pub fn new(
        input: ScriptEditorInput<impl Fn(String) + 'static>,
        script_template_groups: &'static [ScriptTemplateGroup],
    ) -> Self {
        let (value_sender, value_receiver) =
            SenderToNormalThread::new_unbounded_channel("advanced script editor apply");
        Self {
            view: Default::default(),
            state: {
                let toolbox = Toolbox {
                    engine: input.engine,
                    help_url: input.help_url,
                    script_template_groups,
                    value_sender,
                };
                RefCell::new(Some(State::new(input.initial_value, toolbox)))
            },
            value_receiver,
            set_value: Box::new(input.set_value),
        }
    }
}

impl View for AdvancedScriptEditorPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_EMPTY_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.set_timer(TIMER_ID, Duration::from_millis(30));
        let state = self.state.take().expect("state already in use");
        egui_views::open(
            window,
            "Script editor",
            state,
            advanced_script_editor::run_ui,
        );
        true
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }

    fn timer(&self, _: usize) -> bool {
        if let Some(v) = self.value_receiver.try_iter().last() {
            let v = blocking_lock(&v);
            (self.set_value)(v.clone());
        }
        true
    }
}
