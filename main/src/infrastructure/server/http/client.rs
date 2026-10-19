//! Contains the mainly technical HTTP/WebSocket server code.

use crate::infrastructure::server::data::{Topic, Topics};
use anyhow::Context;
use axum::extract::ws::Utf8Bytes;
use base::hash_util::NonCryptoHashMap;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<NonCryptoHashMap<usize, WebSocketClient>>>;

#[derive(Debug, Clone)]
pub struct WebSocketClient {
    pub topics: Topics,
    pub sender: mpsc::UnboundedSender<Utf8Bytes>,
}

impl WebSocketClient {
    pub fn send(&self, msg: impl Serialize) -> anyhow::Result<()> {
        let json = serde_json::to_string(&msg).context("couldn't serialize")?;
        let utf8_bytes = Utf8Bytes::from(json);
        let result = self.sender.send(utf8_bytes);
        result.context("couldn't send")?;
        Ok(())
    }

    pub fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}
