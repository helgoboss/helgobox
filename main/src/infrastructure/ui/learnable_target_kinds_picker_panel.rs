use crate::base::SenderToNormalThread;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use crate::infrastructure::ui::egui_views::learnable_target_kinds_picker::{run_ui, State, Value};
use crossbeam_channel::Receiver;
use derivative::Derivative;
use reaper_low::raw;
use std::cell::RefCell;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct LearnableTargetKindsPickerPanel {
    view: ViewContext,
    state: RefCell<Option<State>>,
    value_receiver: Receiver<Value>,
    #[derivative(Debug = "ignore")]
    set_value: Box<dyn Fn(Value)>,
}

const TIMER_ID: usize = 320;

impl LearnableTargetKindsPickerPanel {
    pub fn new(initial_value: Value, set_value: impl Fn(Value) + 'static) -> Self {
        let (value_sender, value_receiver) =
            SenderToNormalThread::new_unbounded_channel("learnable targets");
        Self {
            view: Default::default(),
            state: RefCell::new(Some(State::new(initial_value, value_sender))),
            value_receiver,
            set_value: Box::new(set_value),
        }
    }
}

impl View for LearnableTargetKindsPickerPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_EMPTY_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.set_timer(TIMER_ID, Duration::from_millis(30));
        let state = self.state.take().expect("state already in use");
        egui_views::open(window, "Learnable targets", state, run_ui);
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
            (self.set_value)(v);
        }
        true
    }
}
