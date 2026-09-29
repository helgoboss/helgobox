use std::time::Duration;

pub async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}

pub async fn retry<F, Fut, T, E>(num_attempts: u32, interval: Duration, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    for _ in 0..num_attempts - 1 {
        match f().await {
            Ok(value) => return Ok(value),
            Err(_) => futures_timer::Delay::new(interval).await,
        }
    }
    f().await
}
