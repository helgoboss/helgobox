use crate::base::default_util::{bool_true, deserialize_null_default, is_bool_true, is_default};
use crate::base::AsyncNotifier;
use crate::domain::{OscDeviceId, OscInputDevice, OscOutputDevice};
use crate::infrastructure::plugin::App;
use derive_more::Display;
use rx_util::Notifier;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedOscDeviceManager = Rc<RefCell<OscDeviceManager>>;

#[derive(Debug)]
pub struct OscDeviceManager {
    config: OscDeviceConfig,
    changed_subject: LocalSubject<'static, (), ()>,
    osc_device_config_file_path: PathBuf,
}

impl OscDeviceManager {
    pub fn new(osc_device_config_file_path: PathBuf) -> OscDeviceManager {
        let mut manager = OscDeviceManager {
            config: Default::default(),
            osc_device_config_file_path,
            changed_subject: Default::default(),
        };
        let _ = manager.load();
        manager
    }

    fn load(&mut self) -> Result<(), String> {
        let json = fs::read_to_string(&self.osc_device_config_file_path)
            .map_err(|_| "couldn't read OSC device config file".to_string())?;
        let config: OscDeviceConfig = serde_json::from_str(&json)
            .map_err(|e| format!("OSC device config file isn't valid. Details:\n\n{}", e))?;
        self.config = config;
        Ok(())
    }

    fn save(&mut self) -> Result<(), String> {
        fs::create_dir_all(self.osc_device_config_file_path.parent().unwrap())
            .map_err(|_| "couldn't create OSC device config file parent directory")?;
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|_| "couldn't serialize OSC device config")?;
        fs::write(&self.osc_device_config_file_path, json)
            .map_err(|_| "couldn't write OSC devie config file")?;
        Ok(())
    }

    pub fn devices(&self) -> impl Iterator<Item = &OscDevice> + ExactSizeIterator {
        self.config.devices.iter()
    }

    pub fn find_index_by_id(&self, id: &OscDeviceId) -> Option<usize> {
        self.config.devices.iter().position(|dev| dev.id() == id)
    }

    pub fn find_device_by_id(&self, id: &OscDeviceId) -> Option<&OscDevice> {
        self.config.devices.iter().find(|dev| dev.id() == id)
    }

    pub fn find_device_by_index(&self, index: usize) -> Option<&OscDevice> {
        self.config.devices.get(index)
    }

    pub fn connect_all_enabled_inputs_and_outputs(
        &mut self,
    ) -> (Vec<OscInputDevice>, Vec<OscOutputDevice>) {
        let (input_devs, output_devs): (Vec<_>, Vec<_>) = self
            .config
            .devices
            .iter_mut()
            .flat_map(|dev| dev.connect())
            .unzip();
        (
            input_devs.into_iter().flatten().collect(),
            output_devs.into_iter().flatten().collect(),
        )
    }

    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.changed_subject.clone()
    }

    pub fn add_device(&mut self, dev: OscDevice) -> Result<(), &'static str> {
        self.config.devices.push(dev);
        self.save_and_notify_changed()?;
        Ok(())
    }

    pub fn update_device(&mut self, dev: OscDevice) -> Result<(), &'static str> {
        let old_dev = self
            .config
            .devices
            .iter_mut()
            .find(|d| d.id() == dev.id())
            .ok_or("couldn't find OSC device")?;
        let _ = std::mem::replace(old_dev, dev);
        self.save_and_notify_changed()?;
        Ok(())
    }

    pub fn remove_device_by_id(&mut self, dev_id: OscDeviceId) -> Result<(), &'static str> {
        self.config.devices.retain(|dev| dev.id != dev_id);
        self.save_and_notify_changed()?;
        Ok(())
    }

    fn save_and_notify_changed(&mut self) -> Result<(), &'static str> {
        self.save()
            .map_err(|_| "error when saving OSC device configuration")?;
        AsyncNotifier::notify(&mut self.changed_subject, &());
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OscDeviceConfig {
    #[serde(default)]
    devices: Vec<OscDevice>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OscDevice {
    id: OscDeviceId,
    name: String,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    is_enabled_for_control: bool,
    /// For receiving control messages.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    local_port: Option<u16>,
    #[serde(skip)]
    has_input_connection_problem: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    is_enabled_for_feedback: bool,
    /// For sending feedback messages.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    device_host: Option<Ipv4Addr>,
    /// For sending feedback messages.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    device_port: Option<u16>,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    can_deal_with_bundles: bool,
    #[serde(skip)]
    has_output_connection_problem: bool,
}

impl Default for OscDevice {
    fn default() -> Self {
        Self {
            id: OscDeviceId::random(),
            name: "".to_string(),
            is_enabled_for_control: true,
            is_enabled_for_feedback: true,
            local_port: None,
            device_host: None,
            device_port: None,
            can_deal_with_bundles: true,
            has_input_connection_problem: false,
            has_output_connection_problem: false,
        }
    }
}

impl OscDevice {
    pub fn connect(
        &mut self,
    ) -> Result<(Option<OscInputDevice>, Option<OscOutputDevice>), Box<dyn Error>> {
        if !self.is_enabled_for_control && !self.is_enabled_for_feedback {
            return Err("neither control nor feedback enabled".into());
        }
        let ip = Ipv4Addr::UNSPECIFIED;
        let bind_address = if self.is_enabled_for_control {
            // Control. We need to bind to the defined local port.
            SocketAddrV4::new(ip, self.local_port.ok_or("local port not specified")?)
        } else {
            // Feedback only. We don't care to which port to connect locally because we don't
            // want to receive control messages.
            SocketAddrV4::new(ip, 0)
        };
        let socket = UdpSocket::bind(bind_address)?;
        let input_dev = if self.is_enabled_for_control {
            let result = self.connect_input_internal(socket.try_clone()?);
            self.has_input_connection_problem = result.is_err();
            Some(result?)
        } else {
            None
        };
        let output_dev = if self.is_enabled_for_feedback {
            let result = self.connect_output_internal(socket);
            self.has_output_connection_problem = result.is_err();
            Some(result?)
        } else {
            None
        };
        Ok((input_dev, output_dev))
    }

    fn connect_input_internal(&self, socket: UdpSocket) -> Result<OscInputDevice, Box<dyn Error>> {
        socket.set_nonblocking(true)?;
        OscInputDevice::bind(
            self.id,
            socket,
            App::logger().new(slog::o!("struct" => "OscInputDevice", "id" => self.id.to_string())),
        )
    }

    fn connect_output_internal(
        &self,
        socket: UdpSocket,
    ) -> Result<OscOutputDevice, Box<dyn Error>> {
        let dest_addr = SocketAddrV4::new(
            self.device_host.ok_or("device host not specified")?,
            self.device_port.ok_or("local port not specified")?,
        );
        let dev = OscOutputDevice::new(
            self.id,
            socket,
            dest_addr,
            App::logger().new(slog::o!("struct" => "OscOutputDevice", "id" => self.id.to_string())),
            self.can_deal_with_bundles,
        );
        Ok(dev)
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

    pub fn can_deal_with_bundles(&self) -> bool {
        self.can_deal_with_bundles
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

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_local_port(&mut self, local_port: Option<u16>) {
        self.local_port = local_port;
    }

    pub fn set_device_host(&mut self, device_host: Option<Ipv4Addr>) {
        self.device_host = device_host;
    }

    pub fn set_device_port(&mut self, device_port: Option<u16>) {
        self.device_port = device_port;
    }

    pub fn toggle_control(&mut self) {
        self.is_enabled_for_control = !self.is_enabled_for_control;
    }

    pub fn toggle_feedback(&mut self) {
        self.is_enabled_for_feedback = !self.is_enabled_for_feedback;
    }

    pub fn toggle_can_deal_with_bundles(&mut self) {
        self.can_deal_with_bundles = !self.can_deal_with_bundles;
    }

    pub fn get_list_label(&self, is_output: bool) -> String {
        format!(
            "{}{}",
            self.name(),
            if is_output {
                self.output_status()
            } else {
                self.input_status()
            }
        )
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
