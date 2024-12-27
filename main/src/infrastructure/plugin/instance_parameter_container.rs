use crate::base::notification;
use reaper_low::firewall;
use std::sync;

use crate::domain::{ParameterManager, PluginParamIndex};
use crate::infrastructure::data::InstanceData;
use crate::infrastructure::plugin::instance_shell::InstanceShell;
use reaper_high::Reaper;
use std::sync::{Arc, OnceLock, RwLock};
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
pub struct InstanceParameterContainer {
    /// Will be set as soon as the containing FX is known.
    lazy_data: OnceLock<LazyData>,
    /// We may have to cache some data that the host wants us to load because we are not ready
    /// for loading data as long as the unit shell is not available.
    pending_data_to_be_loaded: RwLock<Option<Vec<u8>>>,
}

#[derive(Debug)]
struct LazyData {
    instance_shell: sync::Weak<InstanceShell>,
    /// Only the parameters of the main unit are exposed as VST parameters.
    main_unit_parameter_manager: Arc<ParameterManager>,
}

impl InstanceParameterContainer {
    /// Creates the parameter container.
    pub fn new() -> Self {
        Self {
            lazy_data: OnceLock::new(),
            pending_data_to_be_loaded: Default::default(),
        }
    }

    /// Sets the instance shell.
    ///
    /// Also checks if there's pending state to be loaded into the instance shell and if yes, loads it.
    pub fn notify_instance_shell_available(
        &self,
        instance_shell: &Arc<InstanceShell>,
        main_unit_parameter_manager: Arc<ParameterManager>,
    ) {
        if let Some(data) = self.pending_data_to_be_loaded.write().unwrap().take() {
            tracing::debug!("Loading pending data into unit shell");
            load_data_or_warn(instance_shell.clone(), &data);
        }
        let lazy_data = LazyData {
            instance_shell: Arc::downgrade(instance_shell),
            main_unit_parameter_manager,
        };
        self.lazy_data.set(lazy_data).unwrap();
    }

    fn get_parameter_name_internal(&self, index: i32) -> Result<String, &'static str> {
        let index = PluginParamIndex::try_from(index as u32)?;
        let lazy_data = self.get_lazy_data()?;
        let name = lazy_data
            .main_unit_parameter_manager
            .params()
            .build_qualified_parameter_name(index);
        Ok(name)
    }

    fn get_parameter_text_internal(&self, index: i32) -> Result<String, &'static str> {
        let index = PluginParamIndex::try_from(index as u32)?;
        let lazy_data = self.get_lazy_data()?;
        let text = lazy_data
            .main_unit_parameter_manager
            .params()
            .at(index)
            .to_string();
        Ok(text)
    }

    fn set_parameter_internal(&self, index: i32, value: f32) -> Result<(), &'static str> {
        let index = PluginParamIndex::try_from(index as u32)?;
        let lazy_data = self.get_lazy_data()?;
        lazy_data
            .main_unit_parameter_manager
            .set_single_parameter(index, value);
        Ok(())
    }

    fn string_to_parameter_internal(&self, index: i32, text: String) -> Result<bool, &'static str> {
        let index = PluginParamIndex::try_from(index as u32)?;
        let lazy_data = self.get_lazy_data()?;
        let parse_result = lazy_data
            .main_unit_parameter_manager
            .params()
            .at(index)
            .setting()
            .parse_to_raw_value(&text);
        let res = if let Ok(raw_value) = parse_result {
            lazy_data
                .main_unit_parameter_manager
                .set_single_parameter(index, raw_value);
            true
        } else {
            false
        };
        Ok(res)
    }

    fn get_lazy_data(&self) -> Result<&LazyData, &'static str> {
        self.lazy_data.get().ok_or("lazy data not available yet")
    }

    fn get_parameter_internal(&self, index: i32) -> Result<f32, &'static str> {
        let index = PluginParamIndex::try_from(index as u32)?;
        let lazy_data = self.get_lazy_data()?;
        // It's super important that we don't get the parameter from the session because if
        // the parameter is set shortly before via `set_parameter()`, it can happen that we
        // don't get this latest value from the session - it will arrive there a bit later
        // because we use async messaging to let the session know about the new parameter
        // value. Getting an old value is terrible for clients which use the current value
        // for calculating a new value, e.g. ReaLearn itself when used with relative encoders.
        // Turning the encoder will result in increments not being applied reliably.
        let value = lazy_data
            .main_unit_parameter_manager
            .params()
            .at(index)
            .raw_value();
        Ok(value)
    }
}

/// This will be returned if ReaLearn cannot return reasonable bank data yet.
const NOT_READY_YET: &str = "not-ready-yet";

impl PluginParameters for InstanceParameterContainer {
    fn get_bank_data(&self) -> Vec<u8> {
        let Some(lazy_data) = self.lazy_data.get() else {
            return match self.pending_data_to_be_loaded.read().unwrap().as_ref() {
                None => NOT_READY_YET.to_string().into_bytes(),
                Some(d) => d.clone(),
            };
        };
        lazy_data
            .instance_shell
            .upgrade()
            .expect("instance shell gone")
            .save()
    }

    fn load_bank_data(&self, data: &[u8]) {
        // TODO-medium-performance We could optimize by getting the config var only once, saving it in a global
        //  struct and then just dereferencing the var whenever we need it. Justin said that the result of
        //  get_config_var never changes throughout the lifetime of REAPER.
        if let Ok(pref) = Reaper::get().get_preference_ref::<u8>("__fx_loadstate_ctx") {
            if *pref == b'U' {
                // REAPER is loading an updated undo state. We don't want to participate in REAPER's undo because
                // it often leads to unpleasant surprises. ReaLearn is its own world. And Playtime even has its
                // own undo system.
                return;
            }
        }
        if data == NOT_READY_YET.as_bytes() {
            if let Some(lazy_data) = self.lazy_data.get() {
                // Looks like someone activated the "Reset to factory default" preset.
                lazy_data
                    .instance_shell
                    .upgrade()
                    .expect("instance shell gone")
                    .apply_data(InstanceData::default())
                    .expect("couldn't load factory default");
            }
            return;
        }
        let Some(lazy_data) = self.lazy_data.get() else {
            // Unit shell is not available yet. Memorize data so we can apply it
            // as soon as the shell is available.
            self.pending_data_to_be_loaded
                .write()
                .unwrap()
                .replace(data.to_vec());
            return;
        };
        let instance_shell = lazy_data
            .instance_shell
            .upgrade()
            .expect("instance shell gone");
        load_data_or_warn(instance_shell, data);
    }

    fn get_parameter_name(&self, index: i32) -> String {
        self.get_parameter_name_internal(index).unwrap_or_default()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_internal(index).unwrap_or_default()
    }

    fn set_parameter(&self, index: i32, value: f32) {
        let _ = self.set_parameter_internal(index, value);
    }

    fn get_parameter_text(&self, index: i32) -> String {
        self.get_parameter_text_internal(index).unwrap_or_default()
    }

    fn string_to_parameter(&self, index: i32, text: String) -> bool {
        self.string_to_parameter_internal(index, text)
            .unwrap_or_default()
    }
}

pub const SET_STATE_PARAM_NAME: &str = "set-state";

fn load_data_or_warn(instance_shell: Arc<InstanceShell>, data: &[u8]) {
    notification::warn_user_on_anyhow_error(instance_shell.load(data));
}
