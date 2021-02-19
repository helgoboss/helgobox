use crate::core::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{OscDeviceId, OscInputDevice, OscOutputDevice};
use crate::infrastructure::plugin::App;
use derive_more::Display;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use std::rc::Rc;
use std::str::FromStr;

pub type SharedOscDeviceManager = Rc<RefCell<OscDeviceManager>>;

#[derive(Debug)]
pub struct OscDeviceManager {
    devices: Vec<OscDevice>,
    changed_subject: LocalSubject<'static, (), ()>,
    osc_device_config_file_path: PathBuf,
}

impl OscDeviceManager {
    pub fn new(osc_device_config_file_path: PathBuf) -> OscDeviceManager {
        let mut manager = OscDeviceManager {
            osc_device_config_file_path,
            devices: vec![],
            changed_subject: Default::default(),
        };
        let _ = manager.load().unwrap();
        manager
    }

    fn load(&mut self) -> Result<(), String> {
        let json = fs::read_to_string(&self.osc_device_config_file_path)
            .map_err(|_| "couldn't read OSC device config file".to_string())?;
        let config: OscDeviceConfig = serde_json::from_str(&json)
            .map_err(|e| format!("OSC device config file isn't valid. Details:\n\n{}", e))?;
        self.devices = config.devices;
        Ok(())
    }

    pub fn devices(&self) -> impl Iterator<Item = &OscDevice> + ExactSizeIterator {
        self.devices.iter()
    }

    pub fn find_index_by_id(&self, id: &OscDeviceId) -> Option<usize> {
        self.devices.iter().position(|dev| dev.id() == id)
    }

    pub fn find_device_by_index(&self, index: usize) -> Option<&OscDevice> {
        self.devices.get(index)
    }

    pub fn connect_all_enabled_inputs(&mut self) -> Vec<OscInputDevice> {
        self.devices
            .iter_mut()
            .filter(|dev| dev.is_enabled_for_control())
            .flat_map(|dev| dev.connect_input())
            .collect()
    }

    pub fn connect_all_enabled_outputs(&mut self) -> Vec<OscOutputDevice> {
        self.devices
            .iter_mut()
            .filter(|dev| dev.is_enabled_for_feedback())
            .flat_map(|dev| dev.connect_output())
            .collect()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OscDeviceConfig {
    #[serde(default)]
    devices: Vec<OscDevice>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OscDevice {
    id: OscDeviceId,
    name: String,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    is_enabled_for_control: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    local_port: Option<u16>,
    #[serde(skip)]
    has_input_connection_problem: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    is_enabled_for_feedback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    device_host: Option<Ipv4Addr>,
    #[serde(default, skip_serializing_if = "is_default")]
    device_port: Option<u16>,
    #[serde(skip)]
    has_output_connection_problem: bool,
}

impl Default for OscDevice {
    fn default() -> Self {
        Self {
            id: OscDeviceId::from_str(nanoid::nanoid!(8).as_str()).unwrap(),
            name: "".to_string(),
            is_enabled_for_control: true,
            is_enabled_for_feedback: true,
            local_port: None,
            device_host: None,
            device_port: None,
            has_input_connection_problem: false,
            has_output_connection_problem: false,
        }
    }
}

impl OscDevice {
    pub fn connect_input(&mut self) -> Result<OscInputDevice, Box<dyn Error>> {
        let result = self.connect_input_internal();
        self.has_input_connection_problem = result.is_err();
        result
    }

    fn connect_input_internal(&self) -> Result<OscInputDevice, Box<dyn Error>> {
        OscInputDevice::bind(
            self.id.clone(),
            SocketAddrV4::new(
                Ipv4Addr::UNSPECIFIED,
                self.local_port.ok_or("local port not specified")?,
            ),
            App::logger().new(slog::o!("struct" => "OscInputDevice", "id" => self.id.to_string())),
        )
    }

    pub fn connect_output(&mut self) -> Result<OscOutputDevice, Box<dyn Error>> {
        let result = self.connect_output_internal();
        self.has_output_connection_problem = result.is_err();
        result
    }

    fn connect_output_internal(&self) -> Result<OscOutputDevice, Box<dyn Error>> {
        OscOutputDevice::connect(
            self.id.clone(),
            SocketAddrV4::new(
                self.device_host.ok_or("device host not specified")?,
                self.device_port.ok_or("local port not specified")?,
            ),
            App::logger().new(slog::o!("struct" => "OscOutputDevice", "id" => self.id.to_string())),
        )
    }

    pub fn id(&self) -> &OscDeviceId {
        &self.id
    }

    fn is_configured_for_input(&self) -> bool {
        self.local_port.is_some()
    }

    fn is_configured_for_output(&self) -> bool {
        self.device_host.is_some() && self.device_port.is_some()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn local_port(&self) -> Option<u16> {
        self.local_port
    }

    pub fn device_host(&self) -> Option<Ipv4Addr> {
        self.device_host
    }

    pub fn device_port(&self) -> Option<u16> {
        self.device_port
    }

    pub fn is_enabled_for_control(&self) -> bool {
        self.is_enabled_for_control
    }

    pub fn is_enabled_for_feedback(&self) -> bool {
        self.is_enabled_for_feedback
    }

    pub fn input_status(&self) -> OscDeviceStatus {
        use OscDeviceStatus::*;
        if !self.is_configured_for_input() {
            return Incomplete;
        }
        if !self.is_enabled_for_control {
            return Disabled;
        }
        if self.has_input_connection_problem {
            return UnableToBind;
        }
        Connected
    }

    pub fn output_status(&self) -> OscDeviceStatus {
        use OscDeviceStatus::*;
        if !self.is_configured_for_output() {
            return Incomplete;
        }
        if !self.is_enabled_for_feedback {
            return Disabled;
        }
        if self.has_output_connection_problem {
            return UnableToBind;
        }
        Connected
    }
}

#[derive(Display)]
pub enum OscDeviceStatus {
    #[display(fmt = " <needs config>")]
    Incomplete,
    #[display(fmt = " <disabled>")]
    Disabled,
    #[display(fmt = " <unable to connect>")]
    UnableToBind,
    #[display(fmt = "")]
    Connected,
}
