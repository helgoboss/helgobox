use crate::base::blocking_lock;
use crate::domain::AdditionalTransformationInput;
use crate::infrastructure::ui::util::open_in_browser;
use crate::infrastructure::ui::{ScriptEngine, ScriptTemplate, ScriptTemplateGroup};
use egui::plot::{Line, Plot};
use egui::{CentralPanel, Ui, Visuals};
use egui::{Context, SidePanel, TextEdit};
use helgoboss_learn::{
    TransformationInput, TransformationInputMetaData, TransformationOutput, UnitValue,
};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type SharedContent = Arc<Mutex<String>>;

pub fn init_ui(ctx: &Context, dark_mode_is_enabled: bool) {
    let mut style: egui::Style = (*ctx.style()).clone();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        style.visuals = if dark_mode_is_enabled {
            Visuals::dark()
        } else {
            Visuals::light()
        };
    }
    #[cfg(target_os = "linux")]
    {
        style.visuals = Visuals::light();
    }
    ctx.set_style(style);
}

pub fn run_ui(ctx: &Context, state: &mut State) {
    SidePanel::left("left-panel")
        .default_width(ctx.available_rect().width() * 0.6)
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
                                    // TODO-high clear template in preview when moving out of
                                    //  menu button area
                                }
                                if response.clicked() {
                                    // Apply template
                                    *blocking_lock(&state.content) = template.content.to_string();
                                    state.invalidate();
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
                if ui.button("Help").clicked() {
                    open_in_browser(state.toolbox.help_url);
                };
            });
            let response = {
                let mut content = blocking_lock(&state.content);
                let text_edit = TextEdit::multiline(&mut *content).code_editor();
                ui.add_sized(ui.available_size(), text_edit)
            };
            if response.changed() {
                state.invalidate();
            }
        });
    CentralPanel::default().show(ctx, |ui| {
        if let Some(template_in_preview) = &state.template_in_preview {
            // A template is being hovered. Show a preview!
            // Description
            ui.label(template_in_preview.template.description);
            // Code preview
            // TODO-high Make built-in undo work for German layout
            // TODO-high Or build a dedicated undo/redo working directly on the content
            // TODO-high Make copy/cut work (somehow the C/X keys are eaten when holding command,
            //  they don't arrive in baseview)
            // TODO-high Maybe reuse whatever clipboard code is used in ReaLearn in general
            let mut content = template_in_preview.template.content;
            let output = TextEdit::multiline(&mut content).code_editor().show(ui);
            let anything_selected = output
                .cursor_range
                .map_or(false, |cursor| !cursor.is_empty());
            output.response.context_menu(|ui| {
                if ui
                    .add_enabled(anything_selected, egui::Button::new("Copy"))
                    .clicked()
                {
                    if let Some(text_cursor_range) = output.cursor_range {
                        use egui::TextBuffer as _;
                        let selected_chars = text_cursor_range.as_sorted_char_range();
                        let selected_text = content.char_range(selected_chars);
                        ctx.output().copied_text = selected_text.to_string();
                    }
                }
            });
            // Plot preview
            plot_build_outcome(ui, &template_in_preview.build_outcome);
        } else {
            // Plot our script
            plot_build_outcome(ui, &state.last_build_outcome);
        }
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

fn plot_build_outcome(ui: &mut Ui, build_outcome: &BuildOutcome) {
    if !build_outcome.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &build_outcome.error);
        return;
    }
    Plot::new("transformation_plot")
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
        .show(ui, |plot_ui| {
            let plot_points: Vec<_> = build_outcome
                .plot_entries
                .iter()
                .filter_map(|e| {
                    let y = match e.output {
                        TransformationOutput::Stop | TransformationOutput::None => return None,
                        TransformationOutput::Control(v)
                        | TransformationOutput::ControlAndStop(v) => v.get(),
                    };
                    Some([e.input, y])
                })
                .collect();
            plot_ui.line(Line::new(plot_points));
        });
}

pub struct State {
    content: SharedContent,
    last_build_outcome: BuildOutcome,
    template_in_preview: Option<TemplateInPreview>,
    toolbox: Toolbox,
}

struct TemplateInPreview {
    template: &'static ScriptTemplate,
    build_outcome: BuildOutcome,
}

#[derive(Default)]
struct BuildOutcome {
    plot_entries: Vec<PlotEntry>,
    uses_time: bool,
    error: String,
}

struct PlotEntry {
    input: f64,
    output: TransformationOutput<UnitValue>,
}

pub struct Toolbox {
    pub engine: Box<dyn ScriptEngine>,
    pub help_url: &'static str,
    pub script_template_groups: &'static [ScriptTemplateGroup],
}

impl Toolbox {
    fn build(&self, content: &str) -> BuildOutcome {
        match self.engine.compile(content) {
            Ok(script) => {
                let uses_time = script.uses_time();
                let sample_count = if uses_time {
                    // 301 samples from 0 to 10 seconds
                    // TODO-high Check what happens to first invocation. Maybe not in time domain?
                    301
                } else {
                    // 101 samples from 0.0 to 1.0
                    101
                };
                let plot_entries = (0..sample_count)
                    .filter_map(|i| {
                        let (x, rel_time_millis) = if uses_time {
                            // TODO-high This is not enough. We must also increase the x axis bounds
                            //  to reflect the seconds.
                            (1.0, 33 * i)
                        } else {
                            (0.01 * i as f64, 0)
                        };
                        let input = TransformationInput::new(
                            UnitValue::new_clamped(x),
                            TransformationInputMetaData {
                                rel_time: Duration::from_millis(rel_time_millis),
                            },
                        );
                        let additional_input = AdditionalTransformationInput { y_last: 0.0 };
                        let output = script
                            .evaluate(input, UnitValue::MIN, additional_input)
                            .ok()?;
                        let entry = PlotEntry {
                            input: if uses_time {
                                rel_time_millis as f64 / 1000.0
                            } else {
                                x
                            },
                            output,
                        };
                        Some(entry)
                    })
                    .collect();
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
    pub fn new(content: SharedContent, toolbox: Toolbox) -> Self {
        let mut state = State {
            content,
            last_build_outcome: Default::default(),
            template_in_preview: None,
            toolbox,
        };
        state.invalidate();
        state
    }

    pub fn invalidate(&mut self) {
        let content = blocking_lock(&self.content);
        self.last_build_outcome = self.toolbox.build(&*content);
    }
}
