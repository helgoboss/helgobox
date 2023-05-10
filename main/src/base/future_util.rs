use std::time::Duration;

pub async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}
