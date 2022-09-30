use crate::base::{blocking_lock, non_blocking_lock};
use crate::domain::{
    AdditionalTransformationInput, EelMidiSourceScript, EelTransformation, LuaMidiSourceScript,
    SafeLua,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use crate::infrastructure::ui::{ScriptEditorInput, ScriptEngine};
use baseview::WindowHandle;
use derivative::Derivative;
use egui::plot::{PlotPoint, PlotPoints};
use egui::{CentralPanel, Style, TextEdit, Visuals};
use helgoboss_learn::{
    AbsoluteValue, FeedbackStyle, FeedbackValue, MidiSourceScript, NumericFeedbackValue,
    Transformation, TransformationInput, TransformationInputMetaData, UnitValue,
};
use reaper_low::raw;
use std::cell::RefCell;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use swell_ui::{Dimensions, SharedView, View, ViewContext, Window};

pub type SharedContent = Arc<Mutex<String>>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct AdvancedScriptEditorPanel {
    view: ViewContext,
    content: SharedContent,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(String)>,
    #[derivative(Debug = "ignore")]
    toolbox: RefCell<Option<Toolbox>>,
}

impl AdvancedScriptEditorPanel {
    pub fn new(input: ScriptEditorInput<impl Fn(String) + 'static, EelTransformation>) -> Self {
        Self {
            view: Default::default(),
            content: Arc::new(Mutex::new(input.initial_content)),
            apply: Box::new(input.apply),
            toolbox: {
                let toolbox = Toolbox {
                    engine: input.engine,
                    help_url: input.help_url,
                };
                RefCell::new(Some(toolbox))
            },
        }
    }

    fn apply(&self) {
        let content = blocking_lock(&self.content);
        (self.apply)(content.clone());
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
        let toolbox = self.toolbox.take().expect("toolbox already in use");
        let state = State::new(self.content.clone(), toolbox);
        // let args = RealearnEguiRunArgs {
        //     parent_window: self.view.require_window(),
        //     title: "Script editor".into(),
        //     width: size.width.get(),
        //     height: size.height.get(),
        //     state,
        //     update: run_ui,
        // };
        // RealearnEgui::run(args);
        let settings = baseview::WindowOpenOptions {
            title: "Script editor".into(),
            size: baseview::Size::new(size.width.get() as _, size.height.get() as _),
            scale: baseview::WindowScalePolicy::SystemScaleFactor,
            gl_config: Some(Default::default()),
        };
        egui_baseview::EguiWindow::open_parented(
            &self.view.require_window(),
            settings,
            state,
            |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, _state: &mut State| {
                let mut style: egui::Style = (*ctx.style()).clone();
                style.visuals = Visuals::light();
                ctx.set_style(style);
            },
            |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, state: &mut State| {
                run_ui(ctx, state);
            },
        );
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.apply();
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
    content: SharedContent,
    last_compilation_error: String,
    last_plot_points: Vec<PlotPoint>,
    toolbox: Toolbox,
}

struct Toolbox {
    engine: Box<dyn ScriptEngine<Script = EelTransformation>>,
    help_url: &'static str,
}

impl State {
    pub fn new(content: SharedContent, toolbox: Toolbox) -> Self {
        State {
            content,
            last_compilation_error: "".into(),
            last_plot_points: Default::default(),
            toolbox,
        }
    }
}

fn run_ui(ctx: &egui::Context, state: &mut State) {
    use egui::plot::{Line, Plot, PlotPoints};
    use egui::{
        emath, epaint, pos2, vec2, Color32, Frame, Pos2, Rect, SidePanel, Stroke, TextEdit, Window,
    };
    SidePanel::left("left-panel").show(ctx, |ui| {
        let mut content = blocking_lock(&state.content);
        let text_edit = TextEdit::multiline(&mut *content)
            .code_editor()
            .desired_rows(20)
            .desired_width(f32::INFINITY);
        let response = ui.add(text_edit);
        if response.changed() {
            let (points, error) = match state.toolbox.engine.compile(&*content) {
                Ok(script) => {
                    let uses_time = script.wants_to_be_polled();
                    let sample_count = if uses_time {
                        // 301 samples from 0 to 10 seconds
                        // TODO-high Check what happens to first invocation. Maybe not in time domain?
                        301
                    } else {
                        // 101 samples from 0.0 to 1.0
                        101
                    };
                    let points = (0..sample_count)
                        .filter_map(|i| {
                            let (x, rel_time) = if uses_time {
                                (1.0, Duration::from_millis(33 * i))
                            } else {
                                (0.01 * i as f64, Duration::ZERO)
                            };
                            let input = TransformationInput::new(
                                x,
                                TransformationInputMetaData { rel_time },
                            );
                            let additional_input = AdditionalTransformationInput { y_last: 0.0 };
                            let output = script.transform(input, 0.0, additional_input).ok()?;
                            let y = output.value()?;
                            Some(PlotPoint::new(x, y))
                        })
                        .collect();
                    (points, "".to_string())
                }
                Err(e) => (vec![], e.to_string()),
            };
            state.last_compilation_error = error;
            state.last_plot_points = points;
        }
        ui.label(&state.last_compilation_error);
    });
    CentralPanel::default().show(ctx, |ui| {
        let line = Line::new(PlotPoints::Owned(state.last_plot_points.clone()));
        Plot::new("transformation_plot")
            .view_aspect(2.0)
            .show(ui, |plot_ui| plot_ui.line(line));
    });
    // Window::new("Hey")
    //     .collapsible(true)
    //     .show(context, |ui| {
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
    // });
}
