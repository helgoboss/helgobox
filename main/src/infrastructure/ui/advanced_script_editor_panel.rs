use crate::base::blocking_lock;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views::advanced_script_editor;
use crate::infrastructure::ui::egui_views::advanced_script_editor::Toolbox;
use crate::infrastructure::ui::ScriptEditorInput;
use derivative::Derivative;
use reaper_low::raw;
use semver::Version;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use swell_ui::{SharedView, View, ViewContext, Window};

pub struct ScriptTemplateGroup {
    pub name: &'static str,
    pub templates: &'static [ScriptTemplate],
}

pub struct ScriptTemplate {
    pub name: &'static str,
    pub content: &'static str,
    pub description: &'static str,
    pub control_styles: &'static [ControlStyle],
    pub min_realearn_version: Option<Version>,
}

#[derive(Copy, Clone, derive_more::Display)]
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
    content: advanced_script_editor::SharedContent,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(String)>,
    #[derivative(Debug = "ignore")]
    toolbox: RefCell<Option<Toolbox>>,
}

impl AdvancedScriptEditorPanel {
    pub fn new(
        input: ScriptEditorInput<impl Fn(String) + 'static>,
        script_template_groups: &'static [ScriptTemplateGroup],
    ) -> Self {
        Self {
            view: Default::default(),
            content: Arc::new(Mutex::new(input.initial_content)),
            apply: Box::new(input.apply),
            toolbox: {
                let toolbox = Toolbox {
                    engine: input.engine,
                    help_url: input.help_url,
                    script_template_groups,
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
        use advanced_script_editor::State;
        let window_size = window.size();
        let dpi_factor = window.dpi_scaling_factor();
        let window_width = window_size.width.get() as f64 / dpi_factor;
        let window_height = window_size.height.get() as f64 / dpi_factor;
        let toolbox = self.toolbox.take().expect("toolbox already in use");
        let state = State::new(self.content.clone(), toolbox);
        let settings = baseview::WindowOpenOptions {
            title: "Script editor".into(),
            size: baseview::Size::new(window_width, window_height),
            scale: baseview::WindowScalePolicy::SystemScaleFactor,
            gl_config: Some(Default::default()),
        };
        egui_baseview::EguiWindow::open_parented(
            &self.view.require_window(),
            settings,
            state,
            |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, _state: &mut State| {
                advanced_script_editor::init_ui(ctx, Window::dark_mode_is_enabled());
            },
            |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, state: &mut State| {
                advanced_script_editor::run_ui(ctx, state);
            },
        );
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.apply();
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }
}
