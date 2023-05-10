use futures::channel::oneshot;
use once_cell::sync::Lazy;
use std::error::Error;
use std::future::Future;
use tokio::runtime::Runtime;

type PotWorkerResult<R> = Result<R, Box<dyn Error>>;

#[derive(Debug)]
pub struct PotWorkerDispatcher<C, R> {
    task: Option<Task<C, R>>,
}

impl<C, R> Default for PotWorkerDispatcher<C, R> {
    fn default() -> Self {
        Self { task: None }
    }
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
struct Task<C, R> {
    receiver: oneshot::Receiver<R>,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn FnOnce(&mut C, R) + Send>,
}

impl<C, R> PotWorkerDispatcher<C, R>
where
    R: Send + 'static,
{
    pub fn poll(&mut self, state: &mut C) {
        let Some(mut task) = self.task.take() else {
            // No task. Don't do anything.
            return;
        };
        self.task = match task.receiver.try_recv() {
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
    }

    pub fn do_in_background_and_then(
        &mut self,
        f: impl Future<Output = R> + Send + 'static,
        handler: impl FnOnce(&mut C, R) + Send + 'static,
    ) {
        let (sender, receiver) = oneshot::channel::<R>();
        let task = Task {
            receiver,
            handler: Box::new(handler),
        };
        self.task = Some(task);
        spawn_in_pot_worker(async move {
            let output = f.await;
            let _ = sender.send(output);
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
