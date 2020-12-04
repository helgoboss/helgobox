use crate::application::{Controller, ControllerManager, SharedMapping};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::MappingModelData;

use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct FileBasedControllerManager {
    controller_dir_path: PathBuf,
    controllers: Vec<Controller>,
    changed_subject: LocalSubject<'static, (), ()>,
}

impl FileBasedControllerManager {
    pub fn new(controller_dir_path: PathBuf) -> FileBasedControllerManager {
        let mut manager = FileBasedControllerManager {
            controller_dir_path,
            controllers: vec![],
            changed_subject: Default::default(),
        };
        let _ = manager.load_controllers();
        manager
    }

    pub fn load_controllers(&mut self) -> Result<(), String> {
        let controller_file_paths = fs::read_dir(&self.controller_dir_path)
            .map_err(|_| "couldn't read ReaLearn resource directory".to_string())?
            .filter_map(|result| {
                let dir_entry = result.ok()?;
                let file_type = dir_entry.file_type().ok()?;
                if !file_type.is_file() {
                    return None;
                }
                let path = dir_entry.path();
                if !path.extension().contains(&"json") {
                    return None;
                };
                Some(path)
            });
        self.controllers = controller_file_paths
            .filter_map(|p| load_controller(p).ok())
            .collect();
        Ok(())
    }

    pub fn controllers(&self) -> impl Iterator<Item = &Controller> + ExactSizeIterator {
        self.controllers.iter()
    }

    pub fn find_by_index(&self, index: usize) -> Option<&Controller> {
        self.controllers.get(index)
    }

    pub fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.controllers.iter().position(|c| c.id() == id)
    }

    pub fn add_controller(&mut self, controller: Controller) -> Result<(), &'static str> {
        let path = self.get_controller_file_path(controller.id());
        fs::create_dir_all(&self.controller_dir_path)
            .map_err(|_| "couldn't create controller directory")?;
        let mut data = ControllerData::from_model(&controller);
        // We don't want to have the ID in the file - because the file name itself is the ID
        data.id = None;
        let json =
            serde_json::to_string_pretty(&data).map_err(|_| "couldn't serialize controller")?;
        fs::write(path, json).map_err(|_| "couldn't write controller file")?;
        self.notify_changed();
        Ok(())
    }

    pub fn remove_controller(&mut self, id: &str) -> Result<(), &'static str> {
        let path = self.get_controller_file_path(id);
        fs::remove_file(path).map_err(|_| "couldn't delete controller file")?;
        self.notify_changed();
        Ok(())
    }

    pub fn update_controller(&mut self, controller: Controller) -> Result<(), &'static str> {
        self.add_controller(controller)
    }

    pub fn changed(&self) -> impl UnitEvent {
        self.changed_subject.clone()
    }

    pub fn log_debug_info(&self) {
        let msg = format!(
            "\n\
            # Controller manager\n\
            \n\
            - Controller count: {}\n\
            ",
            self.controllers.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    fn notify_changed(&mut self) {
        let _ = self.load_controllers();
        self.changed_subject.next(());
    }

    fn get_controller_file_path(&self, id: &str) -> PathBuf {
        self.controller_dir_path.join(format!("{}.json", id))
    }
}

impl ControllerManager for FileBasedControllerManager {
    fn find_by_id(&self, id: &str) -> Option<Controller> {
        self.controllers.iter().find(|c| c.id() == id).cloned()
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        let controller = match self.controllers.iter().find(|c| c.id() == id) {
            None => return false,
            Some(c) => c,
        };
        if mappings.len() != controller.mappings().len() {
            return true;
        }
        mappings
            .iter()
            .zip(controller.mappings())
            .any(|(actual_mapping, controller_mapping)| {
                let actual_mapping_data = MappingModelData::from_model(&actual_mapping.borrow());
                let controller_mapping_data = MappingModelData::from_model(controller_mapping);
                actual_mapping_data != controller_mapping_data
            })
    }
}

pub type SharedControllerManager = Rc<RefCell<FileBasedControllerManager>>;

impl ControllerManager for SharedControllerManager {
    fn find_by_id(&self, id: &str) -> Option<Controller> {
        self.borrow().find_by_id(id)
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        self.borrow().mappings_are_dirty(id, mappings)
    }
}

fn load_controller(path: impl AsRef<Path>) -> Result<Controller, String> {
    let id = path
        .as_ref()
        .file_stem()
        .ok_or_else(|| "controller file must have stem because it makes up the ID".to_string())?
        .to_string_lossy()
        .to_string();
    let json =
        fs::read_to_string(&path).map_err(|_| "couldn't read controller file".to_string())?;
    let data: ControllerData = serde_json::from_str(&json).map_err(|e| {
        format!(
            "Controller file {:?} isn't valid. Details:\n\n{}",
            path.as_ref(),
            e
        )
    })?;
    Ok(data.to_model(id))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerData {
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    custom_data: HashMap<String, serde_json::Value>,
}

impl ControllerData {
    pub fn from_model(controller: &Controller) -> ControllerData {
        ControllerData {
            id: Some(controller.id().to_string()),
            mappings: controller
                .mappings()
                .map(|m| MappingModelData::from_model(&m))
                .collect(),
            name: controller.name().to_string(),
            custom_data: controller.custom_data().clone(),
        }
    }

    pub fn to_model(&self, id: String) -> Controller {
        Controller::new(
            id,
            self.name.clone(),
            self.mappings
                .iter()
                .map(|m| m.to_model(MappingCompartment::ControllerMappings, None))
                .collect(),
            self.custom_data.clone(),
        )
    }
}
