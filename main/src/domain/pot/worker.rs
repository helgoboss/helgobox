use futures::channel::oneshot;
use once_cell::sync::Lazy;
use std::any::Any;
use std::collections::VecDeque;
use std::error::Error;
use std::future::Future;
use tokio::runtime::Runtime;

type PotWorkerResult<R> = Result<R, Box<dyn Error>>;

/// Helper for easily dispatching background work from a non-asynchronous context and executing
/// code as soon as the result of the background work is available.
#[derive(Debug)]
pub struct PotWorkerDispatcher<C> {
    tasks: VecDeque<Task<C, Box<dyn Any + Send + 'static>>>,
}

impl<C> PotWorkerDispatcher<C> {
    /// Checks for each background task if a result is available and if yes, executes the
    /// corresponding result handler.
    ///
    /// Must be called repeatedly from the non-asynchronous code.
    pub fn poll(&mut self, state: &mut C) {
        // Take next task from queue. Do nothing if no task enqueued.
        let Some(mut task) = self.tasks.pop_front() else {
            return;
        };
        // Check if task has already produced some result.
        match task.result_receiver.try_recv() {
            // Result available. Handle it, done.
            Ok(Some(result)) => {
                (task.result_handler)(state, result);
            }
            // Result not yet available. Push task back on queue.
            Ok(None) => {
                self.tasks.push_back(task);
            }
            // Sender dropped. Discard task.
            Err(_) => {}
        }
    }

    /// Schedules the given work for execution by the Pot worker and registers a handler that
    /// will be executed as soon as the work has produced a result.
    pub fn do_in_background_and_then<R>(
        &mut self,
        work: impl Future<Output = R> + Send + 'static,
        result_handler: impl FnOnce(&mut C, R) + Send + 'static,
    ) where
        R: Send + 'static,
    {
        // Create one-shot channel for transferring task result from background thread to
        // polling thread.
        let (sender, receiver) = oneshot::channel::<Box<dyn Any + Send + 'static>>();
        // Combine this and the corresponding handler into a so-called task.
        let task = Task {
            result_receiver: receiver,
            result_handler: Box::new(|context, result| {
                if let Ok(result) = result.downcast::<R>() {
                    result_handler(context, *result);
                }
            }),
        };
        // Enqueue that task for repeated polling.
        self.tasks.push_back(task);
        // Schedule work to be done in background.
        spawn_in_pot_worker(async move {
            // Start executing work
            let result = work.await;
            // Work done. Send result.
            let _ = sender.send(Box::new(result));
            Ok(())
        });
    }
}

/// Spawns the given future on the Pot worker.
pub fn spawn_in_pot_worker(f: impl Future<Output = PotWorkerResult<()>> + Send + 'static) {
    POT_WORKER_RUNTIME.as_ref().unwrap().spawn(async {
        f.await.unwrap();
    });
}

impl<C> Default for PotWorkerDispatcher<C> {
    fn default() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
struct Task<C, R> {
    result_receiver: oneshot::Receiver<R>,
    #[derivative(Debug = "ignore")]
    result_handler: Box<dyn FnOnce(&mut C, R) + Send>,
}

static POT_WORKER_RUNTIME: Lazy<std::io::Result<Runtime>> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_name("ReaLearn Pot Worker")
        .worker_threads(1)
        .build()
});
