use crate::application::SessionData;
use crate::core::SendOrSyncWhatever;
use crate::domain::Session;
use crate::domain::SharedSession;
use lazycell::{AtomicLazyCell, LazyCell};
use reaper_high::Reaper;
use slog::debug;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::{Mutex, RwLock};
use vst::plugin::PluginParameters;

pub struct RealearnPluginParameters {
    session: AtomicLazyCell<SendOrSyncWhatever<SharedSession>>,
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
    pub fn notify_session_is_available(&self, session: SharedSession) {
        // We will never access the session in another thread than the main thread because
        // REAPER calls the GetData/SetData functions in main thread only! So, Send or Sync,
        // whatever ... we don't care!
        self.session
            .fill(unsafe { SendOrSyncWhatever::new(session) });
        let mut guard = self.data_to_be_loaded.write().unwrap();
        if let Some(data) = guard.as_ref() {
            self.load_bank_data(data);
            *guard = None;
        }
    }
}

impl Drop for RealearnPluginParameters {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping plug-in parameters...");
    }
}

/// This will be returned if ReaLearn cannot return reasonable bank data yet.
const NOT_READY_YET: &'static str = "not-ready-yet";

impl PluginParameters for RealearnPluginParameters {
    fn get_bank_data(&self) -> Vec<u8> {
        let session = match self.session.borrow() {
            None => {
                return match self.data_to_be_loaded.read().unwrap().as_ref() {
                    None => NOT_READY_YET.to_string().into_bytes(),
                    Some(d) => d.clone(),
                };
            }
            Some(s) => s,
        };
        let session_data = SessionData::from_model(&session.borrow());
        serde_json::to_vec(&session_data).expect("couldn't serialize session data")
    }

    fn load_bank_data(&self, data: &[u8]) {
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
        // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find start
        // of JSON data.
        let data = &data[left_json_object_brace..];
        let session_data: SessionData =
            serde_json::from_slice(data).expect("couldn't deserialize session data");
        let mut session = shared_session.borrow_mut();
        session_data.apply_to_model(&mut session);
        session.notify_everything_has_changed(shared_session);
    }
}
