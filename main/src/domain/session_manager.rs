use crate::domain::{ReaperTarget, Session, SharedSession, WeakSession};
use once_cell::unsync::Lazy;
use reaper_high::{ActionKind, Reaper, Track};
use reaper_medium::MessageBoxType;
use rxrust::prelude::*;
use slog::{debug, info};
use std::cell::{Ref, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Mutex;

static mut SESSIONS: Lazy<RefCell<Vec<WeakSession>>> = Lazy::new(|| RefCell::new(vec![]));

/// Panics if not in main thread.
fn sessions() -> &'static RefCell<Vec<WeakSession>> {
    Reaper::get().require_main_thread();
    unsafe { &SESSIONS }
}

pub fn log_debug_info() {
    let msg = format!(
        "\n\
        # Session manager\n\
        \n\
        - Session count: {}
        ",
        sessions().borrow().len()
    );
    Reaper::get().show_console_msg(msg);
}

pub fn register_session(session: WeakSession) {
    let mut sessions = sessions().borrow_mut();
    debug!(Reaper::get().logger(), "Registering new session...");
    unsafe { sessions.push(session) };
    debug!(
        Reaper::get().logger(),
        "Session registered. Session count: {}",
        sessions.len()
    );
}

pub fn unregister_session(session: *const Session) {
    let mut sessions = sessions().borrow_mut();
    debug!(Reaper::get().logger(), "Unregistering session...");
    sessions.retain(|s| {
        match s.upgrade() {
            // Already gone, for whatever reason. Time to throw out!
            None => false,
            // Not gone yet.
            Some(shared_session) => shared_session.as_ptr() != session as _,
        }
    });
    debug!(
        Reaper::get().logger(),
        "Session unregistered. Remaining count of managed sessions: {}",
        sessions.len()
    );
}

type SharedReaperTarget = Rc<RefCell<Option<ReaperTarget>>>;

// TODO-medium I'm not sure if it's worth that constantly listening to target changes ...
//  But right now the control surface calls next() on the subjects anyway. And this listener
//  does nothing more than cloning the target and writing it to a variable. So maybe not so bad
//  performance-wise.
pub fn register_global_learn_action() {
    let last_touched_target: SharedReaperTarget = Rc::new(RefCell::new(None));
    let last_touched_target_clone = last_touched_target.clone();
    // TODO-low Maybe unsubscribe when last ReaLearn instance gone.
    ReaperTarget::touched().subscribe(move |target| {
        last_touched_target_clone.replace(Some((*target).clone()));
    });
    Reaper::get().register_action(
        "realearnLearnSourceForLastTouchedTarget",
        "ReaLearn: Learn source for last touched target",
        move || {
            // We borrow this only very shortly so that the mutable borrow when touching the
            // target can't interfere.
            let target = last_touched_target.borrow().clone();
            let target = match target.as_ref() {
                None => return,
                Some(t) => t,
            };
            start_learning_source_for_target(&target);
        },
        ActionKind::NotToggleable,
    );
}

fn start_learning_source_for_target(target: &ReaperTarget) {
    // Try to find an existing session which has a target with that parameter
    let session = find_first_session_in_current_project_with_target(target)
        // If not found, find the instance on the parameter's track (if there's one)
        .or_else(|| target.track().and_then(find_first_session_on_track))
        // If not found, find a random instance
        .or_else(|| find_first_session_in_current_project());
    match session {
        None => {
            Reaper::get().medium_reaper().show_message_box(
                "Please add a ReaLearn FX to this project first!",
                "ReaLearn",
                MessageBoxType::Okay,
            );
        }
        Some(s) => {
            let mapping = s.borrow_mut().toggle_learn_source_for_target(target);
            s.borrow().show_mapping(mapping.as_ptr());
        }
    }
}

fn find_first_session_in_current_project_with_target(
    target: &ReaperTarget,
) -> Option<SharedSession> {
    find_session(|session| {
        let session = session.borrow();
        session.context().project() == Reaper::get().current_project()
            && session.find_mapping_with_target(target).is_some()
    })
}

fn find_first_session_on_track(track: &Track) -> Option<SharedSession> {
    find_session(|session| {
        let session = session.borrow();
        session.context().track().contains(&track)
    })
}

fn find_first_session_in_current_project() -> Option<SharedSession> {
    find_session(|session| {
        let session = session.borrow();
        session.context().project() == Reaper::get().current_project()
    })
}

fn find_session(predicate: impl FnMut(&SharedSession) -> bool) -> Option<SharedSession> {
    sessions()
        .borrow()
        .iter()
        .filter_map(|s| s.upgrade())
        .find(predicate)
        .map(|s| s.clone())
}
