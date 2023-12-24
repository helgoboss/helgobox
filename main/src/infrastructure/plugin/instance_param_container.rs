use crate::base::notification;
use base::{blocking_read_lock, blocking_write_lock};
use reaper_low::firewall;

use crate::application::ParamContainer;
use crate::domain::{
    Compartment, CompartmentParams, PluginParamIndex, PluginParams, RawParamValue,
};
use crate::infrastructure::data::SessionData;
use crate::infrastructure::plugin::instance_shell::InstanceShell;
use anyhow::Context;
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use vst::plugin::PluginParameters;

/// Container that owns all plug-in parameters (descriptions and values) and is responsible
/// for handling parameter changes as well as saving and loading plug-in state.
///
/// vst-rs requires us to make this separate from the actual plug-in struct.
///
/// It's good to have this separate from the [InstanceShell] because data chunk and parameters might
/// be set or queried by the host *before* the containing FX - and thus the unit shell - is
/// available.
#[derive(Debug)]
pub struct InstanceParamContainer {
    /// Will be set as soon as the containing FX is known.
    unit_shell: OnceLock<Arc<InstanceShell>>,
    /// We may have to cache some data that the host wants us to load because we are not ready
    /// for loading data as long as the unit shell is not available.
    pending_data_to_be_loaded: RwLock<Option<Vec<u8>>>,
    /// Canonical parameters.
    ///
    /// Locked by a read-write lock because this will be accessed from different threads. At least
    /// the values. But we want to keep settings and values tightly together for reasons of
    /// simplicity, so we put them into the read-write lock as well.
    params: RwLock<PluginParams>,
}

impl InstanceParamContainer {
    /// Creates the parameter container.
    pub fn new() -> Self {
        Self {
            unit_shell: OnceLock::new(),
            pending_data_to_be_loaded: Default::default(),
            params: Default::default(),
        }
    }

    /// Sets the unit shell.
    ///
    /// Also checks if there's pending state to be loaded into the unit shell and if yes, loads it.
    pub fn notify_instance_shell_is_available(&self, unit_shell: Arc<InstanceShell>) {
        if let Some(data) = self.pending_data_to_be_loaded.write().unwrap().take() {
            match unit_shell.load(&data) {
                Ok(params) => *self.params_mut() = params,
                Err(e) => notification::notify_user_about_anyhow_error(e),
            }
        }
        self.unit_shell.set(unit_shell).unwrap();
    }

    /// This struct is the best candidate for creating the SessionData object because it knows
    /// the session (at least it holds a share) and most importantly, it owns the parameters.
    pub fn create_session_data(&self) -> anyhow::Result<SessionData> {
        let unit_shell = self.unit_shell.get().context("session gone")?;
        Ok(unit_shell.create_session_data(&self.params()))
    }

    pub fn apply_session_data(&self, session_data: &SessionData) -> anyhow::Result<()> {
        self.apply_session_data_internal(session_data)
    }

    pub fn load_state(&self, json: &str) -> anyhow::Result<()> {
        let session_data: SessionData =
            serde_json::from_str(json).expect("couldn't deserialize session data");
        self.apply_session_data_internal(&session_data)
    }

    fn apply_session_data_internal(&self, session_data: &SessionData) -> anyhow::Result<()> {
        // TODO-medium This is called from ReaLearn itself so we should maybe automate host
        //  parameters otherwise host is not updated. New feature at some point I guess.
        // At the point this is called, the unit shell must exist.
        let unit_shell = self
            .unit_shell
            .get()
            .context("unit shell not yet available when applying session data")?;
        let params = unit_shell.apply_session_data(session_data)?;
        *self.params_mut() = params;
        Ok(())
    }

    fn set_parameter_value_internal(&self, index: PluginParamIndex, value: RawParamValue) {
        let mut params = self.params_mut();
        let param = params.at_mut(index);
        let current_value = param.raw_value();
        if current_value == value {
            // No need to update. This can happen a lot if a ReaLearn parameter is being automated.
            return;
        }
        // Update synchronously so that a subsequent `get_parameter` will immediately
        // return the new value.
        param.set_raw_value(value);
        if let Some(unit_shell) = self.unit_shell.get() {
            unit_shell.set_single_parameter(index, value);
        }
    }

    pub fn params(&self) -> RwLockReadGuard<PluginParams> {
        blocking_read_lock(&self.params, "RealearnPluginParameters params")
    }

    fn params_mut(&self) -> RwLockWriteGuard<PluginParams> {
        blocking_write_lock(&self.params, "RealearnPluginParameters params_mut")
    }
}

/// This will be returned if ReaLearn cannot return reasonable bank data yet.
const NOT_READY_YET: &str = "not-ready-yet";

impl PluginParameters for InstanceParamContainer {
    fn get_bank_data(&self) -> Vec<u8> {
        firewall(|| {
            let Some(unit_shell) = self.unit_shell.get() else {
                return match self.pending_data_to_be_loaded.read().unwrap().as_ref() {
                    None => NOT_READY_YET.to_string().into_bytes(),
                    Some(d) => d.clone(),
                };
            };
            unit_shell.save(&self.params())
        })
        .unwrap_or_default()
    }

    fn load_bank_data(&self, data: &[u8]) {
        firewall(|| {
            if data == NOT_READY_YET.as_bytes() {
                if let Some(unit_shell) = self.unit_shell.get() {
                    // Looks like someone activated the "Reset to factory default" preset.
                    unit_shell
                        .apply_session_data(&SessionData::default())
                        .expect("couldn't load factory default");
                }
                return;
            }
            let Some(unit_shell) = self.unit_shell.get() else {
                // Unit shell is not available yet. Memorize data so we can apply it
                // as soon as the shell is available.
                self.pending_data_to_be_loaded
                    .write()
                    .unwrap()
                    .replace(data.to_vec());
                return;
            };
            let params = unit_shell
                .load(data)
                .expect("couldn't load bank data provided by REAPER");
            *self.params_mut() = params;
        });
    }

    fn get_parameter_name(&self, index: i32) -> String {
        firewall(|| {
            let index = match PluginParamIndex::try_from(index as u32) {
                Ok(i) => i,
                Err(_) => return String::new(),
            };
            self.params().build_qualified_parameter_name(index)
        })
        .unwrap_or_default()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        firewall(|| {
            let index = match PluginParamIndex::try_from(index as u32) {
                Ok(i) => i,
                Err(_) => return 0.0,
            };
            // It's super important that we don't get the parameter from the session because if
            // the parameter is set shortly before via `set_parameter()`, it can happen that we
            // don't get this latest value from the session - it will arrive there a bit later
            // because we use async messaging to let the session know about the new parameter
            // value. Getting an old value is terrible for clients which use the current value
            // for calculating a new value, e.g. ReaLearn itself when used with relative encoders.
            // Turning the encoder will result in increments not being applied reliably.
            self.params().at(index).raw_value()
        })
        .unwrap_or(0.0)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        firewall(|| {
            let index = match PluginParamIndex::try_from(index as u32) {
                Ok(i) => i,
                Err(_) => return,
            };
            self.set_parameter_value_internal(index, value);
        });
    }

    fn get_parameter_text(&self, index: i32) -> String {
        firewall(|| {
            let index = match PluginParamIndex::try_from(index as u32) {
                Ok(i) => i,
                Err(_) => return String::new(),
            };
            self.params().at(index).to_string()
        })
        .unwrap_or_default()
    }

    fn string_to_parameter(&self, index: i32, text: String) -> bool {
        firewall(|| {
            let index = match PluginParamIndex::try_from(index as u32) {
                Ok(i) => i,
                Err(_) => return Default::default(),
            };
            let parse_result = self.params().at(index).setting().parse_to_raw_value(&text);
            if let Ok(raw_value) = parse_result {
                self.set_parameter_value_internal(index, raw_value);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
    }
}

impl ParamContainer for Arc<InstanceParamContainer> {
    fn update_compartment_params(&mut self, compartment: Compartment, params: CompartmentParams) {
        let mut plugin_params = self.params_mut();
        let compartment_params = plugin_params.compartment_params_mut(compartment);
        *compartment_params = params;
        let unit_shell = self
            .unit_shell
            .get()
            .expect("unit shell should be available when updating compartment params");
        unit_shell.set_all_parameters(plugin_params.clone());
    }
}

pub const SET_STATE_PARAM_NAME: &str = "set-state";
