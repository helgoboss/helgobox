use crate::domain::{
    AdditionalTransformationInput, EelMidiSourceScript, EelTransformation, LuaMidiSourceScript,
    SafeLua,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use crate::infrastructure::ui::ScriptEngine;
use derivative::Derivative;
use egui::TextEdit;
use helgoboss_learn::{
    AbsoluteValue, FeedbackStyle, FeedbackValue, MidiSourceScript, NumericFeedbackValue,
    Transformation, UnitValue,
};
use realearn_egui::{RealearnEgui, RealearnEguiRunArgs};
use reaper_low::raw;
use std::cell::RefCell;
use std::error::Error;
use swell_ui::{Dimensions, SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct AdvancedScriptEditorPanel {
    view: ViewContext,
}

impl AdvancedScriptEditorPanel {
    pub fn new() -> Self {
        Self {
            view: Default::default(),
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
        let size = window.size();
        let size: Dimensions<_> = window.convert_to_pixels(size);
        let args = RealearnEguiRunArgs {
            parent_window: self.view.require_window(),
            title: "Script editor".into(),
            width: size.width.get(),
            height: size.height.get(),
            state: State::new(),
            update: run_ui,
        };
        RealearnEgui::run(args);
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }
}

struct State {
    content: String,
}

impl State {
    pub fn new() -> Self {
        State {
            content: "".to_string(),
        }
    }
}

fn run_ui(context: &egui::Context, state: &mut State) {
    use egui::{emath, epaint, pos2, vec2, Color32, Frame, Pos2, Rect, Stroke, TextEdit, Window};
    Window::new("Hey").collapsible(true).show(context, |ui| {
        let text_edit = TextEdit::multiline(&mut state.content).code_editor();
        ui.add_sized(ui.available_size(), text_edit);
        // let color = if ui.visuals().dark_mode {
        //     Color32::from_additive_luminance(196)
        // } else {
        //     Color32::from_black_alpha(240)
        // };
        //
        // Frame::canvas(ui.style()).show(ui, |ui| {
        //     ui.ctx().request_repaint();
        //     let time = ui.input().time;
        //
        //     let desired_size = ui.available_width() * vec2(1.0, 0.35);
        //     let (_id, rect) = ui.allocate_space(desired_size);
        //
        //     let to_screen =
        //         emath::RectTransform::from_to(Rect::from_x_y_ranges(0.0..=1.0, -1.0..=1.0), rect);
        //
        //     let mut shapes = vec![];
        //
        //     for &mode in &[2, 3, 5] {
        //         let mode = mode as f64;
        //         let n = 120;
        //         let speed = 1.5;
        //
        //         let points: Vec<Pos2> = (0..=n)
        //             .map(|i| {
        //                 let t = i as f64 / (n as f64);
        //                 let amp = (time * speed * mode).sin() / mode;
        //                 let y = amp * (t * std::f64::consts::TAU / 2.0 * mode).sin();
        //                 to_screen * pos2(t as f32, y as f32)
        //             })
        //             .collect();
        //
        //         let thickness = 10.0 / mode as f32;
        //         shapes.push(epaint::Shape::line(points, Stroke::new(thickness, color)));
        //     }
        //
        //     ui.painter().extend(shapes);
        // });
    });
}
