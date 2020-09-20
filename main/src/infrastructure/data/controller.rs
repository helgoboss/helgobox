use crate::application::{Controller, ControllerManager};
use crate::infrastructure::data::MappingModelData;
use crate::infrastructure::plugin::App;
use reaper_high::Reaper;
use rx_util::UnitEvent;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct FileBasedControllerManager {
    controllers: Vec<Controller>,
}

impl FileBasedControllerManager {
    pub fn new() -> FileBasedControllerManager {
        let mut manager = FileBasedControllerManager {
            controllers: vec![],
        };
        manager.load_controllers();
        manager
    }

    pub fn load_controllers(&mut self) -> Result<(), &str> {
        let controller_file_paths = fs::read_dir(App::resource_path())
            .map_err(|_| "couldn't read ReaLearn resource directory")?
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

    pub fn add_controller(&mut self, controller: Controller) {
        unimplemented!()
    }

    pub fn remove_controller(&mut self, id: &str) {
        unimplemented!()
    }

    pub fn update_controller(&mut self, controller: Controller) {
        unimplemented!()
    }

    pub fn changed<E>(&self) -> E
    where
        E: UnitEvent,
    {
        unimplemented!()
    }
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

fn load_controller(path: impl AsRef<Path>) -> Result<Controller, &'static str> {
    let id = path
        .as_ref()
        .file_stem()
        .ok_or("controller file must have stem because it makes up the ID")?
        .to_string_lossy()
        .to_string();
    let controller = Controller::new(id.clone(), id, vec![]);
    Ok(controller)
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
            // mappings: controller
            //     .mappings()
            //     .map(|m| MappingModelData::from_model(&m, session.context()))
            //     .collect()
            name: controller.name().to_string(),
            mappings: vec![],
        }
    }

    pub fn to_model(&self, id: String) -> Controller {
        Controller::new(id, self.name.clone(), vec![])
    }
}
