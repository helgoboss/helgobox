use crate::domain::ui_util::format_raw_midi;
use crate::domain::RequestMidiDeviceIdentityReply;
use anyhow::Context;
use nanoid::nanoid;
use realearn_api::persistence::{
    Controller, ControllerConfig, ControllerConnection, MidiControllerConnection, MidiInputPort,
};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::cell::RefCell;
use std::fmt::Debug;
use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, mem};

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
        Self {
            controller_config_path,
            event_handler,
            controller_config: Default::default(),
        }
    }

    pub fn controller_config(&self) -> &ControllerConfig {
        &self.controller_config
    }

    pub fn find_controller_connected_to_midi_input(
        &self,
        dev_id: MidiInputDeviceId,
    ) -> Option<&Controller> {
        self.find_controller_by_midi_connection(|con| {
            con.input_port
                .is_some_and(|p| p.get() == dev_id.get() as u32)
        })
    }

    pub fn find_controller_connected_to_midi_output(
        &self,
        dev_id: MidiOutputDeviceId,
    ) -> Option<&Controller> {
        self.find_controller_by_midi_connection(|con| {
            con.output_port
                .is_some_and(|p| p.get() == dev_id.get() as u32)
        })
    }

    pub fn find_controller_by_midi_connection(
        &self,
        f: impl Fn(&MidiControllerConnection) -> bool,
    ) -> Option<&Controller> {
        self.controller_config
            .controllers
            .iter()
            .find(|controller| match &controller.connection {
                Some(ControllerConnection::Midi(con)) => f(con),
                _ => false,
            })
    }

    pub fn save_controller(
        &mut self,
        mut controller: Controller,
    ) -> anyhow::Result<SaveControllerOutcome> {
        let new_midi_output_device_id = get_controller_midi_output_device_id(&controller);
        let outcome = if controller.id.is_empty() {
            let new_id = nanoid!();
            controller.id = new_id.clone();
            self.controller_config.controllers.push(controller);
            SaveControllerOutcome {
                id: new_id,
                new_midi_output_device_id,
                connection_changed: true,
            }
        } else {
            let existing_controller = self.find_controller_by_id_mut(&controller.id)?;
            let connection_changed = controller.connection != existing_controller.connection;
            let old_controller = mem::replace(existing_controller, controller);
            SaveControllerOutcome {
                id: old_controller.id,
                new_midi_output_device_id,
                connection_changed,
            }
        };
        self.save()?;
        self.event_handler.controller_config_changed(self);
        Ok(outcome)
    }

    pub fn update_controller_device_identity(
        &mut self,
        controller_id: &str,
        reply: Option<RequestMidiDeviceIdentityReply>,
    ) -> anyhow::Result<()> {
        let controller = self.find_controller_by_id_mut(controller_id)?;
        let connection = controller
            .connection
            .as_mut()
            .context("controller has no connection")?;
        match connection {
            ControllerConnection::Midi(c) => {
                if let Some(reply) = reply {
                    // Convenience feature: Set input port automatically
                    c.input_port = Some(MidiInputPort::new(reply.input_device_id.get() as u32));
                    c.identity_response =
                        Some(format_raw_midi(&reply.device_inquiry_reply.message));
                } else {
                    c.identity_response = None;
                }
            }
            ControllerConnection::Osc(_) => {}
        }
        self.save()?;
        self.event_handler.controller_config_changed(self);
        Ok(())
    }

    fn find_controller_by_id_mut(
        &mut self,
        controller_id: &str,
    ) -> anyhow::Result<&mut Controller> {
        self.controller_config
            .controllers
            .iter_mut()
            .find(|c| c.id == controller_id)
            .context("controller not found")
    }

    pub fn delete_controller(&mut self, controller_id: &str) {
        self.controller_config
            .controllers
            .retain(|c| c.id != controller_id);
        self.event_handler.controller_config_changed(self);
    }

    pub fn load_controllers_from_disk(&mut self) -> anyhow::Result<()> {
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

pub struct SaveControllerOutcome {
    pub id: String,
    pub new_midi_output_device_id: Option<MidiOutputDeviceId>,
    pub connection_changed: bool,
}

fn get_controller_midi_output_device_id(controller: &Controller) -> Option<MidiOutputDeviceId> {
    let id = match controller.connection.as_ref()? {
        ControllerConnection::Midi(c) => MidiOutputDeviceId::new(c.output_port?.get() as u8),
        ControllerConnection::Osc(_) => return None,
    };
    Some(id)
}
