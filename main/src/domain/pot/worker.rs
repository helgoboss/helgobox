use futures::channel::oneshot;
use once_cell::sync::Lazy;
use std::any::Any;
use std::collections::VecDeque;
use std::error::Error;
use std::future::Future;
use tokio::runtime::Runtime;

type PotWorkerResult<R> = Result<R, Box<dyn Error>>;

#[derive(Debug)]
pub struct PotWorkerDispatcher<C> {
    tasks: VecDeque<Task<C, Box<dyn Any + Send + 'static>>>,
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
    receiver: oneshot::Receiver<R>,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn FnOnce(&mut C, R) + Send>,
}

impl<C> PotWorkerDispatcher<C> {
    pub fn poll(&mut self, state: &mut C) {
        let Some(mut task) = self.tasks.pop_front() else {
            // No task. Don't do anything.
            return;
        };
        let next_task = match task.receiver.try_recv() {
            // Output available. Handle it.
            Ok(Some(output)) => {
                (task.handler)(state, output);
                None
            }
            // Output not yet available. Just try again later.
            Ok(None) => Some(task),
            // Sender dropped. Discard task.
            Err(_) => None,
        };
        // Push task back on queue
        if let Some(t) = next_task {
            self.tasks.push_back(t);
        }
    }

    pub fn do_in_background_and_then<R>(
        &mut self,
        f: impl Future<Output = R> + Send + 'static,
        handler: impl FnOnce(&mut C, R) + Send + 'static,
    ) where
        R: Send + 'static,
    {
        let (sender, receiver) = oneshot::channel::<Box<dyn Any + Send + 'static>>();
        let task = Task {
            receiver,
            handler: Box::new(|context: &mut C, output: Box<dyn Any + Send + 'static>| {
                if let Ok(output) = output.downcast::<R>() {
                    handler(context, *output);
                }
            }),
        };
        self.tasks.push_back(task);
        spawn_in_pot_worker(async move {
            let output = f.await;
            let _ = sender.send(Box::new(output));
            Ok(())
        });
    }
}

static POT_WORKER_RUNTIME: Lazy<std::io::Result<Runtime>> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_name("ReaLearn Pot Worker")
        .worker_threads(1)
        .build()
});

pub fn spawn_in_pot_worker(f: impl Future<Output = PotWorkerResult<()>> + Send + 'static) {
    POT_WORKER_RUNTIME.as_ref().unwrap().spawn(async {
        f.await.unwrap();
    });
}
