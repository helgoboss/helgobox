use crate::application::SessionData;
use crate::core::SendOrSyncWhatever;

use crate::domain::{SharedSession, WeakSession};
use lazycell::AtomicLazyCell;
use reaper_high::Reaper;
use reaper_low::firewall;
use slog::debug;

use std::sync::RwLock;
use vst::plugin::PluginParameters;

pub struct RealearnPluginParameters {
    session: AtomicLazyCell<SendOrSyncWhatever<WeakSession>>,
    // We may have to cache some data that the host wants us to load because we are not ready
    // for loading data as long as the session is not available.
    data_to_be_loaded: RwLock<Option<Vec<u8>>>,
}

impl Default for RealearnPluginParameters {
    fn default() -> Self {
        Self {
            session: AtomicLazyCell::new(),
            data_to_be_loaded: Default::default(),
        }
    }
}

impl RealearnPluginParameters {
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

    fn session(&self) -> Option<SharedSession> {
        self.session.borrow().and_then(|s| s.upgrade())
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
            let session = match self.session.borrow() {
                None => {
                    return match self.data_to_be_loaded.read().unwrap().as_ref() {
                        None => NOT_READY_YET.to_string().into_bytes(),
                        Some(d) => d.clone(),
                    };
                }
                Some(s) => s,
            };
            let upgraded_session = session.upgrade().expect("session gone");
            let session_data = SessionData::from_model(&upgraded_session.borrow());
            serde_json::to_vec(&session_data).expect("couldn't serialize session data")
        })
        .unwrap_or_default()
    }

    fn load_bank_data(&self, data: &[u8]) {
        firewall(|| {
            if data == NOT_READY_YET.as_bytes() {
                return;
            }
            let shared_session = match self.session.borrow() {
                None => {
                    self.data_to_be_loaded
                        .write()
                        .unwrap()
                        .replace(data.to_vec());
                    return;
                }
                Some(s) => s,
            };
            let left_json_object_brace = data
                .iter()
                .position(|b| *b == 0x7b)
                .expect("couldn't find left JSON brace in bank data");
            // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find
            // start of JSON data.
            let data = &data[left_json_object_brace..];
            let session_data: SessionData =
                serde_json::from_slice(data).expect("couldn't deserialize session data");
            let upgraded_session = shared_session.upgrade().expect("session gone");
            let mut session = upgraded_session.borrow_mut();
            session_data.apply_to_model(&mut session).unwrap();
            session.notify_everything_has_changed(shared_session.get().clone());
        });
    }

    fn get_parameter_name(&self, index: i32) -> String {
        format!("Parameter {}", index + 1)
    }

    fn get_parameter(&self, index: i32) -> f32 {
        if let Some(s) = self.session() {
            s.borrow().get_parameter(index as u32)
        } else {
            0.0
        }
    }

    fn set_parameter(&self, index: i32, value: f32) {
        if let Some(s) = self.session() {
            s.borrow_mut().set_parameter(index as u32, value);
        }
    }
}
