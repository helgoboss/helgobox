use crossbeam_channel::{Receiver, Sender};
use reaper_high::{
    FutureSupport, MainThreadTask, Reaper, TaskSupport, DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
    DEFAULT_MAIN_THREAD_TASK_CHANNEL_CAPACITY,
};
use reaper_rx::{ActionRx, ActionRxProvider, ControlSurfaceRx, MainRx};

make_available_globally_in_any_thread!(Global);

pub struct Global {
    main_rx: MainRx,
    task_support: TaskSupport,
    future_support: FutureSupport,
    task_sender: Sender<MainThreadTask>,
    task_receiver: Receiver<MainThreadTask>,
    executor: reaper_high::run_loop_executor::RunLoopExecutor,
    local_executor: reaper_high::local_run_loop_executor::RunLoopExecutor,
}

impl Default for Global {
    fn default() -> Self {
        let (sender, receiver) =
            crossbeam_channel::bounded(DEFAULT_MAIN_THREAD_TASK_CHANNEL_CAPACITY);
        let (spawner, executor) = reaper_high::run_loop_executor::new_spawner_and_executor(
            DEFAULT_MAIN_THREAD_TASK_CHANNEL_CAPACITY,
            DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
        );
        let (local_spawner, local_executor) =
            reaper_high::local_run_loop_executor::new_spawner_and_executor(
                DEFAULT_MAIN_THREAD_TASK_CHANNEL_CAPACITY,
                DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
            );
        Self {
            main_rx: Default::default(),
            task_support: TaskSupport::new(sender.clone()),
            future_support: FutureSupport::new(spawner, local_spawner),
            task_sender: sender,
            task_receiver: receiver,
            executor,
            local_executor,
        }
    }
}

impl Global {
    // This is kept static just for allowing easy observable subscription from everywhere. For
    // pushing to the subjects, static access is not necessary.
    pub fn control_surface_rx() -> &'static ControlSurfaceRx {
        Reaper::get().require_main_thread();
        Global::get().main_rx.control_surface()
    }

    // This really needs to be kept static for pushing to the subjects because hook commands can't
    // take user data.
    pub fn action_rx() -> &'static ActionRx {
        Reaper::get().require_main_thread();
        Global::get().main_rx.action()
    }

    pub fn task_support() -> &'static TaskSupport {
        &Global::get().task_support
    }

    pub fn future_support() -> &'static FutureSupport {
        &Global::get().future_support
    }

    pub fn task_sender(&self) -> Sender<MainThreadTask> {
        Reaper::get().require_main_thread();
        self.task_sender.clone()
    }

    pub fn task_receiver(&self) -> Receiver<MainThreadTask> {
        Reaper::get().require_main_thread();
        self.task_receiver.clone()
    }

    pub fn executor(&self) -> reaper_high::run_loop_executor::RunLoopExecutor {
        self.executor.clone()
    }
    pub fn local_executor(&self) -> reaper_high::local_run_loop_executor::RunLoopExecutor {
        self.local_executor.clone()
    }
}

impl ActionRxProvider for Global {
    fn action_rx() -> &'static ActionRx {
        Global::action_rx()
    }
}
