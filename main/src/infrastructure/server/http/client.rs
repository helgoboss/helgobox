//! Contains the mainly technical HTTP/WebSocket server code.

use crate::infrastructure::server::data::{Topic, Topics};
use base::hash_util::NonCryptoHashMap;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<NonCryptoHashMap<usize, WebSocketClient>>>;

#[derive(Debug, Clone)]
pub struct WebSocketClient {
    pub topics: Topics,
    pub sender: mpsc::UnboundedSender<String>,
}

impl WebSocketClient {
    pub fn send(&self, msg: impl Serialize) -> Result<(), &'static str> {
        let json = serde_json::to_string(&msg).map_err(|_| "couldn't serialize")?;
        self.sender.send(json).map_err(|_| "couldn't send")
    }

    pub fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}
