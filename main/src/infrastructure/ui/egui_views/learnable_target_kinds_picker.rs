use crate::base::{NamedChannelSender, SenderToNormalThread};
use crate::domain::ReaperTargetType;
use derivative::Derivative;
use egui::CentralPanel;
use egui::Context;
use enum_iterator::IntoEnumIterator;
use realearn_api::persistence::LearnableTargetKind;
use std::collections::HashSet;

pub fn run_ui(ctx: &Context, state: &mut State) {
    CentralPanel::default().show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let mut changed = false;
            for kind in LearnableTargetKind::into_enum_iter() {
                let checked_initially = state.value.contains(&kind);
                let mut checked = checked_initially;
                let target_type = ReaperTargetType::from_learnable_target_kind(kind);
                ui.checkbox(&mut checked, target_type.definition().short_name);
                if checked != checked_initially {
                    changed = true;
                    if checked {
                        state.value.insert(kind);
                    } else {
                        state.value.remove(&kind);
                    }
                }
            }
            if changed {
                state.value_sender.send_complaining(state.value.clone());
            }
        });
    });
}

pub type Value = HashSet<LearnableTargetKind>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct State {
    value: Value,
    #[derivative(Debug = "ignore")]
    value_sender: SenderToNormalThread<Value>,
}

impl State {
    pub fn new(initial_value: Value, value_sender: SenderToNormalThread<Value>) -> Self {
        Self {
            value: initial_value,
            value_sender,
        }
    }
}
