use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{open_in_browser, open_in_text_editor};
use derivative::Derivative;
use reaper_low::raw;
use serde_yaml::Mapping as YamlMapping;
use std::cell::RefCell;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct YamlEditorPanel {
    view: ViewContext,
    content: RefCell<Result<Option<YamlMapping>, serde_yaml::Error>>,
    #[derivative(Debug = "ignore")]
    apply: Box<dyn Fn(Option<YamlMapping>)>,
}

impl YamlEditorPanel {
    pub fn new(
        initial_content: Option<YamlMapping>,
        apply: impl Fn(Option<YamlMapping>) + 'static,
    ) -> Self {
        Self {
            view: Default::default(),
            content: RefCell::new(Ok(initial_content)),
            apply: Box::new(apply),
        }
    }

    fn apply(&self) {
        if let Ok(c) = self.content.borrow().as_ref() {
            (self.apply)(c.clone());
        }
    }

    fn parse_final_result(&self) -> Result<Option<YamlMapping>, serde_yaml::Error> {
        let text = self.text();
        let trimmed_text = text.trim();
        let res = if trimmed_text.is_empty() {
            None
        } else {
            Some(serde_yaml::from_str(trimmed_text)?)
        };
        Ok(res)
    }

    fn invalidate_text_from_initial_content(&self) {
        let text = if let Ok(Some(m)) = &self.content.borrow().as_ref() {
            serde_yaml::to_string(m).unwrap()
        } else {
            "".to_owned()
        };
        self.set_text(&text);
        self.invalidate_info();
    }

    fn update_content(&self) {
        *self.content.borrow_mut() = self.parse_final_result();
        self.invalidate_info();
    }

    fn invalidate_info(&self) {
        let info_text = match self.content.borrow().as_ref() {
            Ok(None) => "Okay! No properties defined.".to_owned(),
            Ok(Some(m)) => format!(
                "Okay! Defined {} properties. Close the window to apply them.",
                m.len()
            ),
            Err(e) => e.to_string(),
        };
        self.view
            .require_control(root::ID_YAML_EDIT_INFO_TEXT)
            .set_text(info_text);
    }

    fn open_in_text_editor(&self) {
        if let Ok(edited_text) =
            open_in_text_editor(&self.text(), self.view.require_window(), ".yaml")
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

impl View for YamlEditorPanel {
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

    fn close_requested(self: SharedView<Self>) -> bool {
        if self.parse_final_result().is_err() {
            !self.view.require_window().confirm("ReaLearn", "Are you sure you want to close this window? The text you have entered is not valid YAML, so ReaLearn will discard it!")
        } else {
            false
        }
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
        "https://github.com/helgoboss/realearn/blob/master/doc/user-guide.adoc#advanced-settings",
    );
}
