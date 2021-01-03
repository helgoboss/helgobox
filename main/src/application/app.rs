use crate::application::{Session, SharedSession, WeakSession};
use crate::core::notification;
use crate::domain::{
    MainProcessor, MappingCompartment, RealearnControlSurfaceMiddleware,
    RealearnControlSurfaceTask, ReaperTarget,
};
use reaper_high::{ActionKind, MiddlewareControlSurface, Reaper, Track};

use rx_util::UnitEvent;
use rxrust::prelude::*;
use slog::{debug, o, Drain};
use std::cell::RefCell;
use std::rc::Rc;

make_available_globally_in_main_thread!(App);

pub type RealearnControlSurface =
    Box<MiddlewareControlSurface<RealearnControlSurfaceMiddleware<WeakSession>>>;

pub struct App {
    sessions: RefCell<Vec<WeakSession>>,
    changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    /// `None` before global initialization and as long as at least one ReaLearn plugin instance
    /// instance loaded. `Some` whenever no ReaLearn plugin instance loaded. It's important that
    /// after unregistering, this is put back here, otherwise pending task executions might fail.
    control_surface: RefCell<Option<RealearnControlSurface>>,
    control_surface_task_sender: crossbeam_channel::Sender<RealearnControlSurfaceTask<WeakSession>>,
}

impl Default for App {
    fn default() -> Self {
        let (control_surface_task_sender, control_surface_task_receiver) =
            crossbeam_channel::unbounded();
        App {
            sessions: Default::default(),
            changed_subject: Default::default(),
            control_surface: {
                let s = MiddlewareControlSurface::new(RealearnControlSurfaceMiddleware::new(
                    &App::logger(),
                    control_surface_task_receiver,
                ));
                RefCell::new(Some(Box::new(s)))
            },
            control_surface_task_sender,
        }
    }
}

impl App {
    pub fn take_control_surface(&self) -> RealearnControlSurface {
        self.control_surface
            .borrow_mut()
            .take()
            .expect("control surface already taken")
    }

    // We need this to be static because we need it at plugin construction time, so we don't have
    // REAPER API access yet. App needs REAPER API to be constructed (e.g. in order to
    // know where's the resource directory that contains the app configuration).
    // TODO-low In future it might be wise to turn to a different logger as soon as REAPER API
    //  available. Then we can also do file logging to ReaLearn resource folder.
    pub fn logger() -> &'static slog::Logger {
        static APP_LOGGER: once_cell::sync::Lazy<slog::Logger> = once_cell::sync::Lazy::new(|| {
            env_logger::init_from_env("REALEARN_LOG");
            slog::Logger::root(slog_stdlog::StdLog.fuse(), slog::o!("app" => "ReaLearn"))
        });
        &APP_LOGGER
    }

    pub fn put_control_surface_back(&self, control_surface: RealearnControlSurface) {
        *self.control_surface.borrow_mut() = Some(control_surface);
    }

    pub fn register_main_processor(&self, p: MainProcessor<WeakSession>) {
        self.control_surface_task_sender
            .send(RealearnControlSurfaceTask::AddMainProcessor(p))
            .unwrap();
    }

    pub fn unregister_main_processor(&self, processor_id: String) {
        self.control_surface_task_sender
            .send(RealearnControlSurfaceTask::RemoveMainProcessor(
                processor_id,
            ))
            .unwrap();
    }

    pub fn changed(&self) -> impl UnitEvent {
        self.changed_subject.borrow().clone()
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.find_session_by_id(session_id).is_some()
    }

    pub fn find_session_by_id(&self, session_id: &str) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.id() == session_id
        })
    }

    pub fn log_debug_info(&self) {
        let msg = format!(
            "\n\
        # App\n\
        \n\
        - Session count: {}\n\
        - Module base address: {:?}\n\
        - Backtrace (GENERATED INTENTIONALLY!)
        ",
            self.sessions.borrow().len(),
            determine_module_base_address().map(|addr| format!("0x{:x}", addr)),
        );
        Reaper::get().show_console_msg(msg);
        panic!("Backtrace");
    }

    pub fn register_session(&self, session: WeakSession) {
        let mut sessions = self.sessions.borrow_mut();
        debug!(Reaper::get().logger(), "Registering new session...");
        sessions.push(session);
        debug!(
            Reaper::get().logger(),
            "Session registered. Session count: {}",
            sessions.len()
        );
        self.notify_changed();
    }

    pub fn unregister_session(&self, session: *const Session) {
        let mut sessions = self.sessions.borrow_mut();
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
        self.notify_changed();
    }

    // TODO-medium I'm not sure if it's worth that constantly listening to target changes ...
    //  But right now the control surface calls next() on the subjects anyway. And this listener
    //  does nothing more than cloning the target and writing it to a variable. So maybe not so bad
    //  performance-wise.
    pub fn register_global_learn_action(&self) {
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
                App::get()
                    .start_learning_source_for_target(MappingCompartment::MainMappings, &target);
            },
            ActionKind::NotToggleable,
        );
    }

    fn start_learning_source_for_target(
        &self,
        compartment: MappingCompartment,
        target: &ReaperTarget,
    ) {
        // Try to find an existing session which has a target with that parameter
        let session = self
            .find_first_session_in_current_project_with_target(compartment, target)
            // If not found, find the instance on the parameter's track (if there's one)
            .or_else(|| {
                target
                    .track()
                    .and_then(|t| self.find_first_session_on_track(t))
            })
            // If not found, find a random instance
            .or_else(|| self.find_first_session_in_current_project());
        match session {
            None => {
                notification::alert("Please add a ReaLearn FX to this project first!");
            }
            Some(s) => {
                let mapping =
                    s.borrow_mut()
                        .toggle_learn_source_for_target(&s, compartment, target);
                s.borrow().show_mapping(mapping.as_ptr());
            }
        }
    }

    fn find_first_session_in_current_project_with_target(
        &self,
        compartment: MappingCompartment,
        target: &ReaperTarget,
    ) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.context().project() == Reaper::get().current_project()
                && session
                    .find_mapping_with_target(compartment, target)
                    .is_some()
        })
    }

    fn find_first_session_on_track(&self, track: &Track) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.context().track().contains(&track)
        })
    }

    fn find_first_session_in_current_project(&self) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.context().project() == Reaper::get().current_project()
        })
    }

    fn find_session(&self, predicate: impl FnMut(&SharedSession) -> bool) -> Option<SharedSession> {
        self.sessions
            .borrow()
            .iter()
            .filter_map(|s| s.upgrade())
            .find(predicate)
    }

    fn notify_changed(&self) {
        self.changed_subject.borrow_mut().next(());
    }
}

type SharedReaperTarget = Rc<RefCell<Option<ReaperTarget>>>;

fn determine_module_base_address() -> Option<usize> {
    let hinstance = Reaper::get()
        .medium_reaper()
        .plugin_context()
        .h_instance()?;
    Some(hinstance.as_ptr() as usize)
}
