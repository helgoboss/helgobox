use crate::application::{Controller, ControllerManager};
use crate::domain::MappingCompartment;
use crate::infrastructure::data::MappingModelData;
use crate::infrastructure::plugin::App;
use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct FileBasedControllerManager {
    controllers: Vec<Controller>,
    changed_subject: LocalSubject<'static, (), ()>,
}

impl FileBasedControllerManager {
    pub fn new() -> FileBasedControllerManager {
        let mut manager = FileBasedControllerManager {
            controllers: vec![],
            changed_subject: Default::default(),
        };
        let _ = manager.load_controllers();
        manager
    }

    pub fn load_controllers(&mut self) -> Result<(), String> {
        let controller_file_paths = fs::read_dir(App::controller_dir_path())
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
        let path = get_controller_file_path(controller.id());
        fs::create_dir_all(App::controller_dir_path())
            .map_err(|_| "couldn't create controller directory")?;
        let data = ControllerData::from_model(&controller);
        let json =
            serde_json::to_string_pretty(&data).map_err(|_| "couldn't serialize controller")?;
        fs::write(path, json).map_err(|_| "couldn't write controller file")?;
        self.notify_changed();
        Ok(())
    }

    pub fn remove_controller(&mut self, id: &str) -> Result<(), &'static str> {
        let path = get_controller_file_path(id);
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

    fn notify_changed(&mut self) {
        let _ = self.load_controllers();
        self.changed_subject.next(());
    }
}

fn get_controller_file_path(id: &str) -> PathBuf {
    App::controller_dir_path().join(format!("{}.json", id))
}

impl ControllerManager for FileBasedControllerManager {
    fn find_by_id(&self, id: &str) -> Option<Controller> {
        self.controllers.iter().find(|c| c.id() == id).cloned()
    }
}

pub type SharedControllerManager = Rc<RefCell<FileBasedControllerManager>>;

impl ControllerManager for SharedControllerManager {
    fn find_by_id(&self, id: &str) -> Option<Controller> {
        self.borrow().find_by_id(id)
    }
}

fn load_controller(path: impl AsRef<Path>) -> Result<Controller, String> {
    let id = path
        .as_ref()
        .file_stem()
        .ok_or("controller file must have stem because it makes up the ID".to_string())?
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
    name: String,
    mappings: Vec<MappingModelData>,
}

impl ControllerData {
    pub fn from_model(controller: &Controller) -> ControllerData {
        ControllerData {
            mappings: controller
                .mappings()
                .map(|m| MappingModelData::from_model(&m))
                .collect(),
            name: controller.name().to_string(),
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
        )
    }
}
