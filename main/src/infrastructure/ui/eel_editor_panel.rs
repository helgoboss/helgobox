use crate::base::eel::{Program, Vm};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use derivative::Derivative;
use reaper_low::raw;
use std::cell::RefCell;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct EelEditorPanel {
    view: ViewContext,
    content: RefCell<String>,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(String)>,
    vm: Vm,
}

impl EelEditorPanel {
    pub fn new(initial_content: String, apply: impl Fn(String) + 'static) -> Self {
        Self {
            view: Default::default(),
            content: RefCell::new(initial_content),
            apply: Box::new(apply),
            vm: Vm::new(),
        }
    }

    fn apply(&self) {
        (self.apply)(self.content.borrow().clone());
    }

    fn compile(&self) -> Result<Program, String> {
        let text = self.text();
        self.vm.compile(&text)
    }

    fn invalidate_text_from_initial_content(&self) {
        let initial_content = &self.content.borrow();
        self.set_text(initial_content);
        self.invalidate_info();
    }

    fn update_content(&self) {
        *self.content.borrow_mut() = self.text();
        self.invalidate_info();
    }

    fn invalidate_info(&self) {
        let info_text = match self.compile() {
            Ok(_) => "Your script compiled successfully.".to_string(),
            Err(e) => e,
        };
        self.view
            .require_control(root::ID_YAML_EDIT_INFO_TEXT)
            .set_text(info_text);
    }

    fn open_in_text_editor(&self) {
        if let Ok(edited_text) =
            open_in_text_editor(&self.text(), self.view.require_window(), ".eel")
        {
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

impl View for EelEditorPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_YAML_EDITOR_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.invalidate_text_from_initial_content();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.apply();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            root::ID_YAML_HELP_BUTTON => help(),
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

fn help() {
    open_in_browser(
        "https://github.com/helgoboss/realearn/blob/master/doc/user-guide.md#script-source",
    );
}
