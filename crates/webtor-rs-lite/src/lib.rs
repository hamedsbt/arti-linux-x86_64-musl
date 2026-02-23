//! Webtor-rs-lite - Lightweight Snowflake transport for arti-client integration
//!
//! This crate provides the minimal Snowflake WebSocket transport stack needed
//! for arti-client integration, without the full webtor-rs Tor client.

#[cfg(test)]
pub mod test_util;

pub mod error;
pub mod kcp_stream;
pub mod retry;
pub mod smux;
pub mod snowflake;
pub mod snowflake_broker;
pub mod snowflake_ws;
pub mod time;
pub mod turbo;

#[cfg(target_arch = "wasm32")]
pub mod wasm_runtime;

pub mod websocket;

#[cfg(target_arch = "wasm32")]
pub mod webrtc_stream;

#[cfg(not(target_arch = "wasm32"))]
pub mod snowflake_ws_native;

// Arti-client integration (WASM only)
#[cfg(target_arch = "wasm32")]
pub mod arti_transport;

// Arti-client integration (native)
#[cfg(not(target_arch = "wasm32"))]
pub mod arti_transport_native;

pub use error::{Result, TorError};

/// Bridge fingerprint for identity verification.
#[derive(Debug, Clone)]
pub enum BridgeFingerprint {
    /// A specific 40-char hex fingerprint to verify the bridge against.
    Pinned(String),
    /// Skip fingerprint verification (less secure).
    NotPinned,
}

impl std::fmt::Display for BridgeFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeFingerprint::Pinned(fp) => write!(f, "{}", fp),
            BridgeFingerprint::NotPinned => write!(f, "(not pinned)"),
        }
    }
}

// Re-export arti-client integration types
#[cfg(target_arch = "wasm32")]
pub use arti_transport::{SnowflakeChannelFactory, SnowflakeMode, SnowflakePtMgr};
#[cfg(not(target_arch = "wasm32"))]
pub use arti_transport_native::{SnowflakeChannelFactory, SnowflakePtMgr};