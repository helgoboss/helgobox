use crate::base::{blocking_lock, NamedChannelSender, SenderToNormalThread};
use crate::domain::AdditionalTransformationInput;
use crate::infrastructure::ui::{ScriptEngine, ScriptTemplate, ScriptTemplateGroup};
use derivative::Derivative;
use egui::plot::{Legend, MarkerShape, Plot, Points, VLine};
use egui::{CentralPanel, Color32, RichText, Ui};
use egui::{Context, SidePanel, TextEdit};
use helgoboss_learn::{
    TransformationInput, TransformationInputMetaData, TransformationOutput, UnitValue,
};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type Value = String;

pub type SharedValue = Arc<Mutex<Value>>;

pub fn run_ui(ctx: &Context, state: &mut State) {
    SidePanel::left("left-panel")
        .default_width(ctx.available_rect().width() * 0.5)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let response = ui.menu_button("Templates", |ui| {
                    for group in state.toolbox.script_template_groups {
                        ui.menu_button(group.name, |ui| {
                            for template in group.templates {
                                let response = ui.button(template.name);
                                if response.hovered() {
                                    // Preview template
                                    let template_changed = state
                                        .template_in_preview
                                        .as_ref()
                                        .map(|t| !ptr::eq(t.template, template))
                                        .unwrap_or(true);
                                    if template_changed {
                                        let build_outcome = state.toolbox.build(template.content);
                                        let template_in_preview = TemplateInPreview {
                                            template,
                                            build_outcome,
                                        };
                                        state.template_in_preview = Some(template_in_preview);
                                    }
                                }
                                if response.clicked() {
                                    // Apply template
                                    let mut content = String::new();
                                    content += "// ";
                                    content += template.name;
                                    if template.description.is_empty() {
                                        content += "\n";
                                    } else {
                                        content += ": ";
                                        for (i, comment_line) in
                                            template.description.lines().enumerate()
                                        {
                                            if i > 0 {
                                                content += "// ";
                                            }
                                            content += comment_line;
                                            content += "\n";
                                        }
                                    };
                                    content += "\n";
                                    content += template.content;
                                    *blocking_lock(&state.shared_value) = content;
                                    state.invalidate_and_send();
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
                if response.response.clicked_elsewhere() {
                    // Menu closed
                    state.template_in_preview = None;
                }
                ui.hyperlink_to("Help", state.toolbox.help_url);
            });
            let response = {
                let mut content = blocking_lock(&state.shared_value);
                let text_edit = TextEdit::multiline(&mut *content).code_editor();
                ui.add_sized(ui.available_size(), text_edit)
            };
            if response.changed() {
                state.invalidate_and_send();
            }
        });
    CentralPanel::default().show(ctx, |ui| {
        if let Some(template_in_preview) = &state.template_in_preview {
            // A template is being hovered. Show a preview!
            // Description
            ui.label(template_in_preview.template.description);
            ui.label("Usable with:");
            for control_style in template_in_preview.template.control_styles {
                ui.horizontal(|ui| {
                    ui.label("- ");
                    ui.label(
                        RichText::new(control_style.to_string())
                            .color(ui.visuals().hyperlink_color),
                    );
                    ui.label(format!(" ({})", control_style.examples()));
                });
            }
            // Plot preview
            plot_build_outcome(ui, &template_in_preview.build_outcome);
        } else {
            // Plot our script
            plot_build_outcome(ui, &state.last_build_outcome);
        }
    });
}

fn plot_build_outcome(ui: &mut Ui, build_outcome: &BuildOutcome) {
    if !build_outcome.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &build_outcome.error);
        return;
    }
    let mut plot = Plot::new("transformation_plot")
        .allow_boxed_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .allow_zoom(false)
        .width(ui.available_width())
        .height(ui.available_height())
        .data_aspect(1.0)
        .view_aspect(1.0)
        .include_x(1.0)
        .include_y(1.0)
        .show_background(false)
        .legend(Legend::default());
    if build_outcome.uses_time {
        plot = plot.x_axis_formatter(|v, _| format!("{v}s"));
    }
    plot.show(ui, |plot_ui| {
        let mut x = 0.0;
        let mut prev_y = 0.0;
        let mut normal_points = vec![];
        let mut none_points = vec![];
        let mut stop_points = vec![];
        // plot_ui.set_plot_bounds(PlotBounds::from_min_max([0.0, 0.0], [1.0, 1.0]));
        for e in &build_outcome.plot_entries {
            x = e.input;
            prev_y = match e.output {
                TransformationOutput::None => {
                    none_points.push([x, prev_y]);
                    prev_y
                }
                TransformationOutput::Control(v) => {
                    normal_points.push([x, v.get()]);
                    v.get()
                }
                TransformationOutput::ControlAndStop(v) => {
                    stop_points.push([x, v.get()]);
                    v.get()
                }
                TransformationOutput::Stop => {
                    stop_points.push([x, prev_y]);
                    prev_y
                }
            };
        }
        let visuals = &plot_ui.ctx().style().visuals;
        plot_ui.points(
            Points::new(normal_points)
                .color(visuals.hyperlink_color)
                .name("Control"),
        );
        plot_ui.points(
            Points::new(none_points)
                .color(Color32::from_white_alpha(1))
                .name("Nothing"),
        );
        plot_ui.points(
            Points::new(stop_points)
                .shape(MarkerShape::Square)
                .color(visuals.error_fg_color)
                .filled(true)
                .name("Stop")
                .radius(6.0),
        );
        if build_outcome.uses_time {
            plot_ui.ctx().request_repaint();
            let time = plot_ui.ctx().input(|i| i.time);
            let bar_color = if visuals.dark_mode {
                Color32::LIGHT_GRAY
            } else {
                Color32::DARK_GRAY
            };
            plot_ui.vline(VLine::new(time % x).color(bar_color));
        }
    });
}

#[derive(Debug)]
pub struct State {
    shared_value: SharedValue,
    last_build_outcome: BuildOutcome,
    template_in_preview: Option<TemplateInPreview>,
    toolbox: Toolbox,
}

#[derive(Debug)]
struct TemplateInPreview {
    template: &'static ScriptTemplate,
    build_outcome: BuildOutcome,
}

#[derive(Debug, Default)]
struct BuildOutcome {
    plot_entries: Vec<PlotEntry>,
    uses_time: bool,
    error: String,
}

#[derive(Debug)]
struct PlotEntry {
    input: f64,
    output: TransformationOutput<UnitValue>,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Toolbox {
    #[derivative(Debug = "ignore")]
    pub engine: Box<dyn ScriptEngine>,
    pub help_url: &'static str,
    pub script_template_groups: &'static [ScriptTemplateGroup],
    pub value_sender: SenderToNormalThread<SharedValue>,
}

/// How much time to cover in the plot for time-dependent scripts.
const MAX_TIME_IN_MILLIS: u32 = 10_000;
/// The rate of invocations of time-dependent scripts (per second).
const INVOCATION_RATE: u32 = 30;

impl Toolbox {
    fn build(&self, content: &str) -> BuildOutcome {
        match self.engine.compile(content) {
            Ok(script) => {
                let uses_time = script.uses_time();
                let sample_count = if uses_time {
                    // One sample per invocation over 10 seconds
                    MAX_TIME_IN_MILLIS * INVOCATION_RATE / 1000
                } else {
                    // 101 samples from 0.0 to 1.0
                    101
                };
                let mut prev_y = UnitValue::MIN;
                let mut plot_entries = vec![];
                for i in 0..sample_count {
                    let (x, rel_time_millis) = if uses_time {
                        (1.0, i * 1000 / INVOCATION_RATE)
                    } else {
                        (0.01 * i as f64, 0)
                    };
                    let input = TransformationInput::new(
                        UnitValue::new_clamped(x),
                        TransformationInputMetaData {
                            rel_time: Duration::from_millis(rel_time_millis as u64),
                        },
                    );
                    let additional_input = AdditionalTransformationInput { y_last: 0.0 };
                    let output = match script.evaluate(input, prev_y, additional_input).ok() {
                        None => continue,
                        Some(e) => e,
                    };
                    let entry = PlotEntry {
                        input: if uses_time {
                            rel_time_millis as f64 / 1000.0
                        } else {
                            x
                        },
                        output,
                    };
                    if let Some(v) = output.value() {
                        prev_y = v;
                    }
                    plot_entries.push(entry);
                    if output.is_stop() {
                        break;
                    }
                }
                BuildOutcome {
                    plot_entries,
                    uses_time,
                    error: "".to_string(),
                }
            }
            Err(e) => BuildOutcome {
                plot_entries: vec![],
                uses_time: false,
                error: e.to_string(),
            },
        }
    }
}

impl State {
    pub fn new(initial_value: Value, toolbox: Toolbox) -> Self {
        let mut state = State {
            shared_value: SharedValue::new(Mutex::new(initial_value)),
            last_build_outcome: Default::default(),
            template_in_preview: None,
            toolbox,
        };
        state.invalidate();
        state
    }

    pub fn invalidate_and_send(&mut self) {
        self.invalidate();
        self.toolbox
            .value_sender
            .send_complaining(self.shared_value.clone());
    }

    pub fn invalidate(&mut self) {
        let content = blocking_lock(&self.shared_value);
        self.last_build_outcome = self.toolbox.build(&content);
    }
}
