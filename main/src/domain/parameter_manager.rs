use crate::domain::{
    CompartmentKind, CompartmentParamIndex, CompartmentParams, ParamSetting, ParameterMainTask,
    PluginParamIndex, PluginParams, RawParamValue,
};
use base::{blocking_read_lock, blocking_write_lock, NamedChannelSender, SenderToNormalThread};
use reaper_high::Reaper;
use reaper_medium::ProjectRef;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug)]
pub struct ParameterManager {
    /// Canonical parameters.
    ///
    /// Locked by a read-write lock because this will be accessed from different threads. At least
    /// the values. But we want to keep settings and values tightly together for reasons of
    /// simplicity, so we put them into the read-write lock as well.
    params: RwLock<PluginParams>,
    parameter_main_task_sender: SenderToNormalThread<ParameterMainTask>,
}

impl ParameterManager {
    pub fn new(parameter_main_task_sender: SenderToNormalThread<ParameterMainTask>) -> Self {
        Self {
            params: Default::default(),
            parameter_main_task_sender,
        }
    }

    pub fn update_certain_compartment_param_settings(
        &self,
        compartment: CompartmentKind,
        settings: Vec<(CompartmentParamIndex, ParamSetting)>,
    ) {
        let mut plugin_params = self.params_mut();
        let compartment_params = plugin_params.compartment_params_mut(compartment);
        compartment_params.apply_given_settings(settings);
    }

    pub fn update_compartment_params(
        &self,
        compartment: CompartmentKind,
        params: CompartmentParams,
    ) {
        let mut plugin_params = self.params_mut();
        let compartment_params = plugin_params.compartment_params_mut(compartment);
        *compartment_params = params;
        // Propagate
        // send_if_space because https://github.com/helgoboss/realearn/issues/847
        self.parameter_main_task_sender
            .send_if_space(ParameterMainTask::UpdateAllParams(plugin_params.clone()));
    }

    pub fn set_all_parameters(&self, params: PluginParams) {
        let mut plugin_params = self.params_mut();
        *plugin_params = params;
        // Propagate
        // send_if_space because https://github.com/helgoboss/realearn/issues/847
        self.parameter_main_task_sender
            .send_if_space(ParameterMainTask::UpdateAllParams(plugin_params.clone()));
    }

    pub fn set_single_parameter(&self, index: PluginParamIndex, value: RawParamValue) {
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
        // We immediately send to the main processor. Sending to the session and using the
        // session parameter list as single source of truth is no option because this method
        // will be called in a processing thread, not in the main thread. Not even a mutex would
        // help here because the session is conceived for main-thread usage only! I was not
        // aware of this being called in another thread and it led to subtle errors of course
        // (https://github.com/helgoboss/realearn/issues/59).
        // When rendering, we don't do it because that will accumulate until the rendering is
        // finished, which is pointless.
        if !is_rendering() {
            self.parameter_main_task_sender
                .send_complaining(ParameterMainTask::UpdateSingleParamValue { index, value });
        }
    }

    pub fn params(&self) -> RwLockReadGuard<PluginParams> {
        blocking_read_lock(&self.params, "ParameterManager params")
    }

    fn params_mut(&self) -> RwLockWriteGuard<PluginParams> {
        blocking_write_lock(&self.params, "ParameterManager params_mut")
    }
}

fn is_rendering() -> bool {
    Reaper::get()
        .medium_reaper()
        .enum_projects(ProjectRef::CurrentlyRendering, 0)
        .is_some()
}
