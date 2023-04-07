use crate::base::{NamedChannelSender, SenderToNormalThread};
use crate::domain::ReaperTargetType;
use derivative::Derivative;
use egui::CentralPanel;
use egui::Context;
use enum_iterator::IntoEnumIterator;
use realearn_api::persistence::{LearnableTargetKind, TargetTouchCause};
use std::collections::HashSet;

pub fn run_ui(ctx: &Context, state: &mut State) {
    CentralPanel::default().show(ctx, |ui: &mut egui::Ui| {
        let mut changed = false;
        // Target types
        ui.label("Consider target types:");
        ui.horizontal_wrapped(|ui| {
            for kind in LearnableTargetKind::into_enum_iter() {
                let checked_initially = state.value.included_targets.contains(&kind);
                let mut checked = checked_initially;
                let target_type = ReaperTargetType::from_learnable_target_kind(kind);
                ui.checkbox(&mut checked, target_type.definition().short_name);
                if checked != checked_initially {
                    changed = true;
                    if checked {
                        state.value.included_targets.insert(kind);
                    } else {
                        state.value.included_targets.remove(&kind);
                    }
                }
            }
        });
        ui.separator();
        // Touch cause
        ui.label("Consider target invocations:");
        let initial_touch_cause = state.value.touch_cause;
        egui::ComboBox::from_id_source("touch-cause")
            .selected_text(state.value.touch_cause.to_string())
            .show_ui(ui, |ui| {
                ui.style_mut().wrap = Some(false);
                ui.set_min_width(60.0);
                for touch_cause in TargetTouchCause::into_enum_iter() {
                    ui.selectable_value(
                        &mut state.value.touch_cause,
                        touch_cause,
                        touch_cause.to_string(),
                    );
                }
            });
        if state.value.touch_cause != initial_touch_cause {
            changed = true;
        }
        // Notify if something changed
        if changed {
            state.value_sender.send_complaining(state.value.clone());
        }
    });
}

#[derive(Clone, Debug)]
pub struct Value {
    pub included_targets: HashSet<LearnableTargetKind>,
    pub touch_cause: TargetTouchCause,
}

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
