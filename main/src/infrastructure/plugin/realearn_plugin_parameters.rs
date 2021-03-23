use crate::core::{notification, SendOrSyncWhatever};

use lazycell::AtomicLazyCell;
use reaper_high::Reaper;
use reaper_low::firewall;
use slog::debug;

use crate::application::{SharedSession, WeakSession};
use crate::domain::{
    MappingCompartment, ParameterArray, ParameterMainTask, ZEROED_PLUGIN_PARAMETERS,
};
use crate::infrastructure::data::SessionData;
use std::rc::Rc;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use vst::plugin::PluginParameters;

#[derive(Debug)]
pub struct RealearnPluginParameters {
    session: AtomicLazyCell<SendOrSyncWhatever<WeakSession>>,
    // We may have to cache some data that the host wants us to load because we are not ready
    // for loading data as long as the session is not available.
    data_to_be_loaded: RwLock<Option<Vec<u8>>>,
    parameter_main_task_sender: crossbeam_channel::Sender<ParameterMainTask>,
    parameters: RwLock<ParameterArray>,
}

impl RealearnPluginParameters {
    pub fn new(parameter_main_task_channel: crossbeam_channel::Sender<ParameterMainTask>) -> Self {
        Self {
            session: AtomicLazyCell::new(),
            data_to_be_loaded: Default::default(),
            parameter_main_task_sender: parameter_main_task_channel,
            parameters: RwLock::new(ZEROED_PLUGIN_PARAMETERS),
        }
    }

    pub fn notify_session_is_available(&self, session: WeakSession) {
        // We will never access the session in another thread than the main thread because
        // REAPER calls the GetData/SetData functions in main thread only! So, Send or Sync,
        // whatever ... we don't care!
        self.session
            .fill(unsafe { SendOrSyncWhatever::new(session) })
            .unwrap();
        let mut guard = self.data_to_be_loaded.write().unwrap();
        if let Some(data) = guard.as_ref() {
            self.load_bank_data(data);
            *guard = None;
        }
    }

    /// This struct is the best candidate for creating the SessionData object because it knows
    /// the session (at least it holds a share) and most importantly, it owns the parameters.
    pub fn create_session_data(&self) -> SessionData {
        self.create_session_data_internal()
    }

    pub fn apply_session_data(&self, session_data: &SessionData) {
        // TODO-medium This is called from ReaLearn itself so we should maybe automate host
        //  parameters otherwise host is not updated. New feature at some point I guess.
        self.apply_session_data_internal(session_data);
    }

    pub fn load_state(&self, json: &str) {
        let session_data: SessionData =
            serde_json::from_str(json).expect("couldn't deserialize session data");
        self.apply_session_data_internal(&session_data);
    }

    fn create_session_data_internal(&self) -> SessionData {
        let session = self.session().expect("session gone");
        let session = session.borrow();
        let parameters = self.parameters();
        SessionData::from_model(&session, &parameters)
    }

    fn apply_session_data_internal(&self, session_data: &SessionData) {
        // Update session
        let shared_session = self.session().expect("session should exist already");
        let mut session = shared_session.borrow_mut();
        if session_data.was_saved_with_newer_version() {
            notification::warn(
                "The session that is about to load was saved with a newer version of ReaLearn. Things might not work as expected. Even more importantly: Saving might result in loss of the data that was saved with the new ReaLearn version! Please consider upgrading your ReaLearn installation to the latest version.",
            );
        }
        let parameters = session_data.parameters_as_array();
        if let Err(e) = session_data.apply_to_model(&mut session, &parameters) {
            notification::warn(e);
        }
        // Update parameters
        self.parameter_main_task_sender
            .try_send(ParameterMainTask::UpdateAllParameters(Box::new(parameters)))
            .unwrap();
        *self.parameters_mut() = parameters;
        // Notify
        session.notify_everything_has_changed(Rc::downgrade(&shared_session));
        session.mark_project_as_dirty();
    }

    fn session(&self) -> Option<SharedSession> {
        let session = self.session.borrow()?;
        session.upgrade()
    }

    fn parameters(&self) -> RwLockReadGuard<ParameterArray> {
        self.parameters.read().expect("writer should never panic")
    }

    fn parameters_mut(&self) -> RwLockWriteGuard<ParameterArray> {
        self.parameters.write().expect("writer should never panic")
    }
}

impl Drop for RealearnPluginParameters {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping plug-in parameters...");
    }
}

/// This will be returned if ReaLearn cannot return reasonable bank data yet.
const NOT_READY_YET: &str = "not-ready-yet";

impl PluginParameters for RealearnPluginParameters {
    fn get_bank_data(&self) -> Vec<u8> {
        firewall(|| {
            if self.session.borrow().is_none() {
                return match self.data_to_be_loaded.read().unwrap().as_ref() {
                    None => NOT_READY_YET.to_string().into_bytes(),
                    Some(d) => d.clone(),
                };
            }
            let session_data = self.create_session_data_internal();
            serde_json::to_vec(&session_data).expect("couldn't serialize session data")
        })
        .unwrap_or_default()
    }

    fn load_bank_data(&self, data: &[u8]) {
        firewall(|| {
            if data == NOT_READY_YET.as_bytes() {
                if self.session().is_some() {
                    // This looks like someone activates the "Reset to factory default" preset.
                    self.apply_session_data_internal(&SessionData::default())
                }
                return;
            }
            if self.session.borrow().is_none() {
                self.data_to_be_loaded
                    .write()
                    .unwrap()
                    .replace(data.to_vec());
                return;
            }
            let left_json_object_brace = data
                .iter()
                .position(|b| *b == 0x7b)
                .expect("couldn't find left JSON brace in bank data");
            // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find
            // start of JSON data.
            let data = &data[left_json_object_brace..];
            let session_data: SessionData =
                serde_json::from_slice(data).expect("couldn't deserialize session data");
            self.apply_session_data_internal(&session_data);
        });
    }

    fn get_parameter_name(&self, index: i32) -> String {
        firewall(|| {
            let name = if let Some(s) = self.session() {
                let index = index as u32;
                if let Some(compartment) = MappingCompartment::by_absolute_param_index(index) {
                    let rel_index = compartment.relativize_absolute_index(index);
                    Some(
                        s.borrow()
                            .get_qualified_parameter_name(compartment, rel_index),
                    )
                } else {
                    None
                }
            } else {
                None
            };
            name.unwrap_or_else(|| format!("Parameter {}", index + 1))
        })
        .unwrap_or_default()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        firewall(|| {
            // It's super important that we don't get the parameter from the session because if
            // the parameter is set shortly before via `set_parameter()`, it can happen that we
            // don't get this latest value from the session - it will arrive there a bit later
            // because we use async messaging to let the session know about the new parameter
            // value. Getting an old value is terrible for clients which use the current value
            // for calculating a new value, e.g. ReaLearn itself when used with relative encoders.
            // Turning the encoder will result in increments not being applied reliably.
            self.parameters()[index as usize]
        })
        .unwrap_or_default()
    }

    fn set_parameter(&self, index: i32, value: f32) {
        firewall(|| {
            // We immediately send to the main processor. Sending to the session and using the
            // session parameter list as single source of truth is no option because this method
            // will be called in a processing thread, not in the main thread. Not even a mutex would
            // help here because the session is conceived for main-thread usage only! I was not
            // aware of this being called in another thread and it led to subtle errors of course
            // (https://github.com/helgoboss/realearn/issues/59).
            self.parameter_main_task_sender
                .try_send(ParameterMainTask::UpdateParameter {
                    index: index as _,
                    value,
                })
                .unwrap();
            // Also update synchronously so that a subsequent `get_parameter` will immediately
            // return the new value.
            self.parameters_mut()[index as usize] = value;
        });
    }
}

pub const SET_STATE_PARAM_NAME: &str = "set-state";
