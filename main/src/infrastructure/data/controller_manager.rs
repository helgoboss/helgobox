use anyhow::Context;
use nanoid::nanoid;
use realearn_api::persistence::{Controller, ControllerConfig};
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedControllerManager = Rc<RefCell<ControllerManager>>;

#[derive(Debug)]
pub struct ControllerManager {
    controller_config_path: PathBuf,
    event_handler: Box<dyn ControllerManagerEventHandler>,
    controller_config: ControllerConfig,
}

pub trait ControllerManagerEventHandler: Debug {
    fn controller_config_changed(&self, source: &ControllerManager);
}

impl ControllerManager {
    pub fn new(
        controller_config_path: PathBuf,
        event_handler: Box<dyn ControllerManagerEventHandler>,
    ) -> Self {
        let mut manager = Self {
            controller_config_path,
            event_handler,
            controller_config: Default::default(),
        };
        let _ = manager.load();
        manager
    }

    pub fn controller_config(&self) -> &ControllerConfig {
        &self.controller_config
    }

    pub fn save_controller(&mut self, mut controller: Controller) -> anyhow::Result<()> {
        if controller.id.is_empty() {
            controller.id = nanoid!();
            self.controller_config.controllers.push(controller);
        } else {
            let existing_controller = self
                .controller_config
                .controllers
                .iter_mut()
                .find(|c| c.id == controller.id)
                .context("controller not found")?;
            *existing_controller = controller;
        }
        self.event_handler.controller_config_changed(self);
        self.save()
    }

    pub fn delete_controller(&mut self, controller_id: &str) {
        self.controller_config
            .controllers
            .retain(|c| c.id != controller_id);
        self.event_handler.controller_config_changed(self);
    }

    fn load(&mut self) -> anyhow::Result<()> {
        let json = fs::read_to_string(&self.controller_config_path)?;
        self.controller_config = serde_json::from_str(&json)?;
        Ok(())
    }

    fn save(&self) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self.controller_config)?;
        fs::create_dir_all(
            self.controller_config_path
                .parent()
                .context("controller config path has no parent")?,
        )?;
        fs::write(&self.controller_config_path, json)?;
        Ok(())
    }
}
