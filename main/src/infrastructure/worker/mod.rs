use once_cell::sync::Lazy;
use std::future::Future;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

// TODO-high-ms2 Shutdown correctly
static MAIN_WORKER_RUNTIME: Lazy<std::io::Result<Runtime>> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_name("ReaLearn Main Worker")
        .worker_threads(1)
        .build()
});

pub fn spawn_in_main_worker<R>(f: impl Future<Output = R> + Send + 'static) -> JoinHandle<R>
where
    R: Send + 'static,
{
    MAIN_WORKER_RUNTIME.as_ref().unwrap().spawn(f)
}
