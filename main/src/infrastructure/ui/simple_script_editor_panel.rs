use crate::domain::{
    EelMidiSourceScript, EelTransformation, LuaFeedbackScript, LuaMidiSourceScript, SafeLua, Script,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::ID_YAML_HELP_BUTTON;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use derivative::Derivative;
use helgoboss_learn::{
    AbsoluteValue, FeedbackScript, FeedbackScriptInput, FeedbackStyle, FeedbackValue,
    MidiSourceScript, NumericFeedbackValue, RawMidiEvent, RawMidiPattern, UnitValue,
};
use reaper_low::raw;
use std::cell::RefCell;
use std::error::Error;
use swell_ui::{SharedView, View, ViewContext, Window};

pub trait ScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>>;

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

impl ScriptEngine for LuaMidiScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let script = LuaMidiSourceScript::compile(&self.lua, code)?;
        script.execute(create_midi_script_test_feedback_value(), Default::default())?;
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".lua"
    }
}

pub struct LuaFeedbackScriptEngine {
    lua: SafeLua,
}

impl LuaFeedbackScriptEngine {
    pub fn new() -> Self {
        Self {
            lua: SafeLua::new().unwrap(),
        }
    }
}

impl ScriptEngine for LuaFeedbackScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let script = LuaFeedbackScript::compile(&self.lua, code)?;
        let test_input = FeedbackScriptInput {
            prop_provider: &|_: &str| None,
        };
        script.used_props()?;
        script.feedback(test_input, Default::default())?;
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".lua"
    }
}

pub struct LuaCompartmentCommonScriptEngine {
    lua: SafeLua,
}

impl LuaCompartmentCommonScriptEngine {
    pub fn new() -> Self {
        Self {
            lua: SafeLua::new().unwrap(),
        }
    }
}

impl ScriptEngine for LuaCompartmentCommonScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let env = self.lua.create_fresh_environment(false)?;
        self.lua
            .compile_as_function("Feedback script", code, env.clone())?;
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".lua"
    }
}

pub struct PlainTextEngine;

impl ScriptEngine for PlainTextEngine {
    fn compile(&self, _: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".txt"
    }
}

pub struct EelMidiScriptEngine;

impl ScriptEngine for EelMidiScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let script = EelMidiSourceScript::compile(code)?;
        script.execute(create_midi_script_test_feedback_value(), Default::default())?;
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct RawMidiScriptEngine;

impl ScriptEngine for RawMidiScriptEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let raw_midi_pattern: RawMidiPattern = code.parse()?;
        if raw_midi_pattern.entries().len() > RawMidiEvent::MAX_LENGTH {
            return Err(format!(
                "Pattern exceeds maximum allowed MIDI message size of {} bytes",
                RawMidiEvent::MAX_LENGTH
            )
            .into());
        }
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".syx"
    }
}

pub struct OscFeedbackArgumentsEngine;

impl ScriptEngine for OscFeedbackArgumentsEngine {
    fn compile(&self, _: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".txt"
    }
}

pub struct EelControlTransformationEngine;

impl ScriptEngine for EelControlTransformationEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let transformation = EelTransformation::compile_for_control(code)?;
        transformation.evaluate(Default::default())?;
        Ok(Box::new(transformation))
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct EelFeedbackTransformationEngine;

impl ScriptEngine for EelFeedbackTransformationEngine {
    fn compile(&self, code: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        let transformation = EelTransformation::compile_for_feedback(code)?;
        transformation.evaluate(Default::default())?;
        Ok(Box::new(()))
    }

    fn file_extension(&self) -> &'static str {
        ".eel"
    }
}

pub struct TextualFeedbackExpressionEngine;

impl ScriptEngine for TextualFeedbackExpressionEngine {
    fn compile(&self, _: &str) -> Result<Box<dyn Script>, Box<dyn Error>> {
        Ok(Box::new(()))
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

pub struct ScriptEditorInput<A> {
    pub engine: Box<dyn ScriptEngine>,
    pub help_url: &'static str,
    pub initial_value: String,
    pub set_value: A,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct SimpleScriptEditorPanel {
    view: ViewContext,
    content: RefCell<String>,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(String)>,
    #[derivative(Debug = "ignore")]
    engine: Box<dyn ScriptEngine>,
    help_url: &'static str,
}

impl SimpleScriptEditorPanel {
    /// If the help URL is empty, the help button will be hidden and the info text (whether
    /// compiled successfully) as well.
    pub fn new(input: ScriptEditorInput<impl Fn(String) + 'static>) -> Self {
        Self {
            view: Default::default(),
            content: RefCell::new(input.initial_value),
            apply: Box::new(input.set_value),
            engine: input.engine,
            help_url: input.help_url,
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

impl View for SimpleScriptEditorPanel {
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

    fn on_destroy(self: SharedView<Self>, _window: Window) {
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

    /// REAPER for macOS introduced support for edit_control_changed (EN_CHANGE) notifications around version 6.81/7.0.
    /// Before that, we could rely on key-up events only (which wouldn't capture pasting text). We leave this here
    /// as backup for older REAPER versions. We could also do a version check instead, but updating content is not that
    /// expensive.
    #[cfg(target_os = "macos")]
    fn key_up(self: SharedView<Self>, _key_code: u8) -> bool {
        self.update_content();
        true
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        match resource_id {
            root::ID_YAML_EDIT_CONTROL => self.update_content(),
            _ => return false,
        };
        true
    }
}
