use crate::domain::{
    AdditionalTransformationInput, EelMidiSourceScript, EelTransformation, LuaMidiSourceScript,
    SafeLua,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::ID_YAML_HELP_BUTTON;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use derivative::Derivative;
use helgoboss_learn::{
    AbsoluteValue, FeedbackStyle, FeedbackValue, MidiSourceScript, NumericFeedbackValue,
    Transformation, UnitValue,
};
use reaper_low::raw;
use std::cell::RefCell;
use std::error::Error;
use swell_ui::{SharedView, View, ViewContext, Window};

pub trait ScriptEngine {
    fn compile(&self, code: &str) -> Result<(), Box<dyn Error>>;

    /// Must include the dot!
    fn file_extension(&self) -> &'static str;
}

pub struct LuaMidiScriptEngine {
    lua: SafeLua,
}

impl LuaMidiScriptEngine {
    pub fn new() -> Self {
        Self {
            lua: SafeLua::new().unwrap(),
        }
    }
}

pub struct PlainTextEngine;

impl ScriptEngine for PlainTextEngine {
    fn compile(&self, _: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".txt"
    }
}

pub struct EelMidiScriptEngine;

impl ScriptEngine for EelMidiScriptEngine {
    fn compile(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let script = EelMidiSourceScript::compile(code)?;
        script.execute(create_midi_script_test_feedback_value())?;
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct EelControlTransformationEngine;

impl ScriptEngine for EelControlTransformationEngine {
    fn compile(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let transformation = EelTransformation::compile_for_control(code)?;
        transformation.transform_continuous(
            Default::default(),
            Default::default(),
            AdditionalTransformationInput::default(),
        )?;
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct EelFeedbackTransformationEngine;

impl ScriptEngine for EelFeedbackTransformationEngine {
    fn compile(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let transformation = EelTransformation::compile_for_feedback(code)?;
        transformation.transform_continuous(
            Default::default(),
            Default::default(),
            AdditionalTransformationInput::default(),
        )?;
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct TextualFeedbackExpressionEngine;

impl ScriptEngine for TextualFeedbackExpressionEngine {
    fn compile(&self, _: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".mustache"
    }
}

fn create_midi_script_test_feedback_value() -> FeedbackValue<'static> {
    FeedbackValue::Numeric(NumericFeedbackValue::new(
        FeedbackStyle::default(),
        AbsoluteValue::Continuous(UnitValue::new(0.0)),
    ))
}

impl ScriptEngine for LuaMidiScriptEngine {
    fn compile(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let script = LuaMidiSourceScript::compile(&self.lua, code)?;
        script.execute(create_midi_script_test_feedback_value())?;
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        ".lua"
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ScriptEditorPanel {
    view: ViewContext,
    content: RefCell<String>,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(String)>,
    #[derivative(Debug = "ignore")]
    engine: Box<dyn ScriptEngine>,
    help_url: &'static str,
}

impl ScriptEditorPanel {
    /// If the help URL is empty, the help button will be hidden and the info text (whether
    /// compiled successfully) as well.
    pub fn new(
        initial_content: String,
        engine: Box<dyn ScriptEngine>,
        help_url: &'static str,
        apply: impl Fn(String) + 'static,
    ) -> Self {
        Self {
            view: Default::default(),
            content: RefCell::new(initial_content),
            apply: Box::new(apply),
            engine,
            help_url,
        }
    }

    fn apply(&self) {
        (self.apply)(self.content.borrow().clone());
    }

    fn invalidate_initial(&self) {
        let initial_content = &self.content.borrow();
        self.set_text(initial_content);
        self.invalidate_info();
        if self.help_url.is_empty() {
            self.view.require_control(ID_YAML_HELP_BUTTON).hide();
        }
    }

    fn update_content(&self) {
        *self.content.borrow_mut() = self.text();
        if !self.help_url.is_empty() {
            self.invalidate_info();
        }
    }

    fn invalidate_info(&self) {
        let info_text = if self.help_url.is_empty() {
            "".to_string()
        } else {
            match self.engine.compile(&self.text()) {
                Ok(_) => "Your script compiled successfully and seems to work.".to_string(),
                Err(e) => e.to_string(),
            }
        };
        self.view
            .require_control(root::ID_YAML_EDIT_INFO_TEXT)
            .set_text(info_text);
    }

    fn open_in_text_editor(&self) {
        if let Ok(edited_text) = open_in_text_editor(
            &self.text(),
            self.view.require_window(),
            self.engine.file_extension(),
        ) {
            self.set_text(&edited_text);
            self.update_content();
        }
    }

    fn text(&self) -> String {
        self.view
            .require_control(root::ID_YAML_EDIT_CONTROL)
            .multi_line_text()
            .unwrap_or_default()
    }

    fn set_text(&self, text: &str) {
        self.view
            .require_control(root::ID_YAML_EDIT_CONTROL)
            .set_multi_line_text(text);
    }
}

impl View for ScriptEditorPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_YAML_EDITOR_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.invalidate_initial();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.apply();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            root::ID_YAML_HELP_BUTTON => open_in_browser(self.help_url),
            root::ID_YAML_TEXT_EDITOR_BUTTON => self.open_in_text_editor(),
            _ => {}
        }
    }

    #[cfg(target_os = "macos")]
    fn key_up(self: SharedView<Self>, _key_code: u8) -> bool {
        self.update_content();
        true
    }

    #[cfg(not(target_os = "macos"))]
    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        match resource_id {
            root::ID_YAML_EDIT_CONTROL => self.update_content(),
            _ => return false,
        };
        true
    }
}
