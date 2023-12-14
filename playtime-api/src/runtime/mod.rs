//! Usually we use Protocol Buffers for the runtime API but there are a few things that are
//! not performance-critical and better expressed in a Rust-first manner.
use serde::{Deserialize, Serialize};
