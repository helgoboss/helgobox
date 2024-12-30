use futures::future::BoxFuture;
use tokio::sync::mpsc::Receiver;
use tracing::debug;

/// An orchestration (task and receiver) to be used to supply the receiver with spare parts that might or might not be
/// necessary.
///
/// This is usually used in scenarios where the consumer lives in a thread that is not allowed to allocate
/// (for example, real-time threads).
pub struct ReplenishmentOrchestration<T, F> {
    pub task: F,
    pub receiver: ReplenishmentReceiver<T>,
}

/// Creates an orchestration.
///
/// The capacity should be very low, depending on how many spare items you want to create.
pub fn orchestrate_replenishment<T>(
    capacity: usize,
    mut create_next_item: impl FnMut() -> T + Send + 'static,
) -> ReplenishmentOrchestration<T, BoxFuture<'static, ()>>
where
    T: Send + 'static,
{
    let (sender, receiver) = tokio::sync::mpsc::channel::<T>(capacity);
    let task = async move {
        while let Ok(permit) = sender.reserve().await {
            debug!("Replenishment channel has capacity. Create next item.");
            let item = create_next_item();
            permit.send(item);
        }
    };
    ReplenishmentOrchestration {
        receiver: ReplenishmentReceiver { receiver },
        task: Box::pin(task),
    }
}

#[derive(Debug)]
pub struct ReplenishmentReceiver<T> {
    receiver: Receiver<T>,
}

impl<T> ReplenishmentReceiver<T> {
    /// Returns the next available item if one is available.
    pub fn request_item(&mut self) -> Option<T> {
        self.receiver.try_recv().ok()
    }
}
