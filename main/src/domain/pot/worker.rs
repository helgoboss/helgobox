use once_cell::sync::Lazy;
use std::error::Error;
use std::future::Future;
use tokio::runtime::Runtime;

static POT_WORKER_RUNTIME: Lazy<std::io::Result<Runtime>> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
});

pub fn spawn(f: impl Future<Output = Result<(), Box<dyn Error>>> + Send + 'static) {
    POT_WORKER_RUNTIME.as_ref().unwrap().spawn(async {
        f.await.unwrap();
    });
}
