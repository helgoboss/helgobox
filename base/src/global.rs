use crossbeam_channel::{Receiver, Sender};
use fragile::Fragile;
use reaper_high::{
    FutureMiddleware, FutureSupport, MainTaskMiddleware, MainThreadTask, TaskSupport,
    DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
};
use reaper_rx::{ActionRx, ActionRxProvider, ControlSurfaceRx, MainRx};
use std::sync::LazyLock;

static INSTANCE: LazyLock<Global> = LazyLock::new(Global::default);

/// Spawns the given future in the main thread.
///
/// This only works if the future support is already running (= if the backbone shell is already woken up).
pub fn spawn_in_main_thread(
    future: impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
) {
    Global::future_support().spawn_in_main_thread_from_main_thread(future);
}

pub struct Global {
    main_rx: Fragile<MainRx>,
    task_support: TaskSupport,
    future_support: FutureSupport,
    task_sender: Sender<MainThreadTask>,
    task_receiver: Receiver<MainThreadTask>,
    send_future_executor: reaper_high::run_loop_executor::RunLoopExecutor,
    non_send_future_executor: reaper_high::local_run_loop_executor::RunLoopExecutor,
}

impl Default for Global {
    fn default() -> Self {
        // It's important that all of the below channels are unbounded. It's not just that they
        // can run full and then panic, it's worse. If sending and receiving happens on the same
        // thread (which we use quite often in order to schedule/spawn something on the main
        // thread) and the channel is full, we will get a deadlock! It's okay that they allocate
        // on sending because `Global` can't be used from a real-time thread.
        // See https://github.com/helgoboss/helgobox/issues/875.
        let (task_sender, task_receiver) = crossbeam_channel::unbounded();
        let (send_future_spawner, send_future_executor) =
            reaper_high::run_loop_executor::new_spawner_and_executor(
                DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
            );
        let (non_send_future_spawner, non_send_future_executor) =
            reaper_high::local_run_loop_executor::new_spawner_and_executor(
                DEFAULT_MAIN_THREAD_TASK_BULK_SIZE,
            );
        Self {
            main_rx: Default::default(),
            task_support: TaskSupport::new(task_sender.clone()),
            future_support: FutureSupport::new(send_future_spawner, non_send_future_spawner),
            task_sender,
            task_receiver,
            send_future_executor,
            non_send_future_executor,
        }
    }
}

impl Global {
    pub fn get() -> &'static Self {
        assert!(
            !reaper_high::Reaper::get()
                .medium_reaper()
                .is_in_real_time_audio(),
            "this function must not be called in a real-time thread"
        );
        &INSTANCE
    }

    // This is kept static just for allowing easy observable subscription from everywhere. For
    // pushing to the subjects, static access is not necessary.

    // Don't use from real-time thread!
    pub fn control_surface_rx() -> &'static ControlSurfaceRx {
        Global::get().main_rx.get().control_surface()
    }

    // This really needs to be kept static for pushing to the subjects because hook commands can't
    // take user data.
    //
    // Don't use from real-time thread!
    pub fn action_rx() -> &'static ActionRx {
        Global::get().main_rx.get().action()
    }

    /// Allows you to schedule tasks for execution on the main thread from anywhere.
    ///
    /// Important: Don't use this to schedule tasks from a real-time thread! This is backed by an
    /// unbounded channel now because of https://github.com/helgoboss/helgobox/issues/875, so
    /// sending can allocate!
    pub fn task_support() -> &'static TaskSupport {
        &Global::get().task_support
    }

    /// Allows you to spawn futures from anywhere.
    ///
    /// Important: Don't use this to spawn futures from a real-time thread! This is backed by an
    /// unbounded channel now because of https://github.com/helgoboss/helgobox/issues/875, so
    /// sending can allocate!
    pub fn future_support() -> &'static FutureSupport {
        &Global::get().future_support
    }

    /// Creates the middleware that drives the task support.
    pub fn create_task_support_middleware(&self) -> MainTaskMiddleware {
        MainTaskMiddleware::new(self.task_sender.clone(), self.task_receiver.clone())
    }

    /// Creates the middleware that drives the future support.
    pub fn create_future_support_middleware(&self) -> FutureMiddleware {
        FutureMiddleware::new(
            self.send_future_executor.clone(),
            self.non_send_future_executor.clone(),
        )
    }
}

impl ActionRxProvider for Global {
    fn action_rx() -> &'static ActionRx {
        Global::action_rx()
    }
}
