//! Arti-compatible transport for Snowflake bridges
//!
//! This module provides integration with arti-client by implementing
//! `ChannelFactory` and `AbstractPtMgr` for Snowflake transports.
//!
//! # Example
//!
//! ```ignore
//! use webtor_rs::arti_transport::{SnowflakePtMgr, SnowflakeMode};
//!
//! // Create a PT manager for WebSocket Snowflake
//! let pt_mgr = SnowflakePtMgr::new(SnowflakeMode::WebSocket {
//!     url: "wss://snowflake.torproject.net/".to_string(),
//!     fingerprint: "2B280B23E1107BB62ABFC40DDCC8824814F80A72".to_string(),
//! });
//!
//! // Or for WebRTC via broker
//! let pt_mgr = SnowflakePtMgr::new(SnowflakeMode::WebRtc {
//!     broker_url: "https://snowflake-broker.torproject.net/".to_string(),
//!     fingerprint: "2B280B23E1107BB62ABFC40DDCC8824814F80A72".to_string(),
//! });
//!
//! // Then set it on the ChanMgr
//! chanmgr.set_pt_mgr(Arc::new(pt_mgr));
//! ```

use std::sync::Arc;

use tor_chanmgr::factory::{AbstractPtError, AbstractPtMgr, BootstrapReporter, ChannelFactory};
use tor_error::{ErrorKind, HasKind, HasRetryTime, RetryTime};
use tor_linkspec::{HasChanMethod, HasRelayIds, IntoOwnedChanTarget, OwnedChanTarget, OwnedChanTargetBuilder, PtTransportName};
use tor_llcrypto::pk::rsa::RsaIdentity;
use tor_proto::channel::Channel;
use tor_proto::memquota::ChannelAccount;
use tor_async_compat::async_trait;
use tracing::{debug, info};

use crate::snowflake::{SnowflakeBridge, SnowflakeConfig};
use crate::snowflake_ws::{SnowflakeWsConfig, SnowflakeWsStream};
use crate::time::system_time_now;
use crate::wasm_runtime::WasmRuntime;

/// Snowflake transport mode
#[derive(Debug, Clone)]
pub enum SnowflakeMode {
    /// WebSocket direct connection to Snowflake bridge
    WebSocket {
        /// WebSocket URL (e.g., "wss://snowflake.torproject.net/")
        url: String,
        /// Bridge fingerprint (40-char hex) for identity verification.
        fingerprint: String,
    },
    /// WebRTC connection via broker
    WebRtc {
        /// Broker URL (e.g., "https://snowflake-broker.torproject.net/")
        broker_url: String,
        /// Bridge fingerprint (40-char hex) for identity verification.
        fingerprint: String,
    },
}

/// Verify that a newly-built channel's RSA identity matches the expected fingerprint.
///
/// The Tor handshake `verify()` step may not enforce RSA identity strictly (ed25519
/// is the primary identity in modern Tor). This explicit check ensures the bridge
/// we connected to is actually the one we intended.
fn verify_bridge_fingerprint(chan: &Channel, expected_fingerprint: &str) -> tor_chanmgr::Result<()> {
    let actual_rsa_id = chan.target().rsa_identity()
        .ok_or_else(|| tor_chanmgr::Error::Io {
            action: "verify bridge fingerprint",
            peer: None,
            source: std::io::Error::other("Bridge did not present an RSA identity").into(),
        })?;

    let actual_fp = hex::encode(actual_rsa_id.as_bytes()).to_uppercase();
    let expected_fp = expected_fingerprint.to_uppercase();

    if actual_fp != expected_fp {
        return Err(tor_chanmgr::Error::Io {
            action: "verify bridge fingerprint",
            peer: None,
            source: std::io::Error::other(format!(
                "Bridge fingerprint mismatch: expected {}, got {}",
                expected_fp, actual_fp
            )).into(),
        });
    }

    Ok(())
}

/// Snowflake channel factory that builds Tor channels over Snowflake transport
pub struct SnowflakeChannelFactory {
    mode: SnowflakeMode,
}

impl SnowflakeChannelFactory {
    /// Create a new Snowflake channel factory
    pub fn new(mode: SnowflakeMode) -> Self {
        Self { mode }
    }

    /// Build a channel using WebSocket Snowflake
    async fn build_ws_channel(
        &self,
        url: &str,
        fingerprint: &str,
        target: &OwnedChanTarget,
        reporter: &BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!("Building Snowflake channel via WebSocket: {}", url);

        reporter.record_attempt();

        // Configure WebSocket Snowflake
        let config = SnowflakeWsConfig::new(url, fingerprint.to_string());

        // Connect via WebSocket
        let stream = SnowflakeWsStream::connect(config)
            .await
            .map_err(|e| tor_chanmgr::Error::Io {
                action: "Snowflake WebSocket connect",
                peer: None,
                source: std::io::Error::other(e.to_string()).into(),
            })?;

        // The Snowflake stream handles transport + TLS internally
        reporter.record_tcp_success();
        reporter.record_tls_finished();

        // Parse fingerprint to RSA identity — fail immediately if format is invalid
        let rsa_id = hex::decode(fingerprint)
            .ok()
            .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
            .ok_or_else(|| tor_chanmgr::Error::Io {
                action: "parse bridge fingerprint",
                peer: None,
                source: std::io::Error::other(format!(
                    "Invalid bridge fingerprint '{}': must be a 40-char hex string",
                    fingerprint
                )).into(),
            })?;

        // Build channel from the stream
        let chan = self.create_channel_from_stream(stream, Some(rsa_id), target, reporter, memquota)
            .await?;

        // Explicitly verify the bridge's RSA identity matches our expected fingerprint.
        // The Tor handshake's verify() may not enforce RSA identity (ed25519 is primary),
        // so we do our own strict check here.
        verify_bridge_fingerprint(&chan, fingerprint)?;

        Ok(chan)
    }

    /// Build a channel using WebRTC Snowflake
    async fn build_webrtc_channel(
        &self,
        broker_url: &str,
        fingerprint: &str,
        target: &OwnedChanTarget,
        reporter: &BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!(
            "Building Snowflake channel via WebRTC broker: {}",
            broker_url
        );

        reporter.record_attempt();

        // Configure WebRTC Snowflake
        let config = SnowflakeConfig::new(broker_url.to_string(), fingerprint.to_string());

        // Connect via WebRTC
        let bridge = SnowflakeBridge::with_config(config);
        let stream = bridge.connect().await.map_err(|e| tor_chanmgr::Error::Io {
            action: "Snowflake WebRTC connect",
            peer: None,
            source: std::io::Error::other(e.to_string()).into(),
        })?;

        // The Snowflake stream handles transport + TLS internally
        reporter.record_tcp_success();
        reporter.record_tls_finished();

        // Parse fingerprint to RSA identity — fail immediately if format is invalid
        let rsa_id = hex::decode(fingerprint)
            .ok()
            .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
            .ok_or_else(|| tor_chanmgr::Error::Io {
                action: "parse bridge fingerprint",
                peer: None,
                source: std::io::Error::other(format!(
                    "Invalid bridge fingerprint '{}': must be a 40-char hex string",
                    fingerprint
                )).into(),
            })?;

        // Build channel from the stream
        let chan = self.create_channel_from_stream(stream, Some(rsa_id), target, reporter, memquota)
            .await?;

        // Explicitly verify the bridge's RSA identity matches our expected fingerprint.
        verify_bridge_fingerprint(&chan, fingerprint)?;

        Ok(chan)
    }

    /// Create a Tor channel from a connected stream
    ///
    /// This is the core channel building logic, adapted from webtor-rs.
    async fn create_channel_from_stream<S>(
        &self,
        stream: S,
        rsa_id: Option<RsaIdentity>,
        target: &OwnedChanTarget,
        reporter: &BootstrapReporter,
        chan_account: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>>
    where
        S: futures::AsyncRead
            + futures::AsyncWrite
            + Send
            + Unpin
            + tor_rtcompat::StreamOps
            + tor_rtcompat::CertifiedConn
            + 'static,
    {
        use tor_proto::ClientChannelBuilder;
        use tor_proto::peer::PeerAddr;

        let runtime = WasmRuntime::default();

        // Extract peer certificate from TLS stream (convert to owned before moving stream)
        let peer_cert = stream.peer_certificate().map_err(|e| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: None,
            source: e.into(),
        })?;

        let peer_cert = peer_cert.map(|c| c.into_owned());

        let peer_cert = peer_cert.ok_or_else(|| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: None,
            source: std::io::Error::other("No peer certificate from TLS")
                .into(),
        })?;

        debug!("Got peer certificate: {} bytes", peer_cert.len());

        // Launch Tor channel handshake
        let mut builder = ClientChannelBuilder::new();
        builder.set_declared_method(target.chan_method());
        debug!("Launching Tor channel client handshake...");
        let handshake = builder.launch(stream, runtime, chan_account);

        debug!("Starting handshake connect...");

        // Build peer target for error reporting and verification
        let mut peer_builder = OwnedChanTargetBuilder::default();
        if let Some(id) = rsa_id {
            peer_builder.rsa_identity(id);
        }

        let peer = peer_builder.build().map_err(|e| {
            tor_chanmgr::Error::Internal(tor_error::internal!(
                "Failed to build peer target: {}",
                e
            ))
        })?;

        let unverified = handshake.connect(system_time_now).await.map_err(|e| {
            tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            }
        })?;

        debug!("Handshake connect completed, verifying...");

        // Verify channel and finish handshake
        let verified = unverified
            .verify(&peer, &peer_cert, Some(system_time_now()))
            .map_err(|e| tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            })?;

        let peer_addr = PeerAddr::Direct("0.0.0.0:0".parse().unwrap());
        let (chan, reactor) = verified.finish(peer_addr).await.map_err(|e| tor_chanmgr::Error::Proto {
            source: e,
            peer: peer.to_logged(),
            clock_skew: None,
        })?;

        // Spawn the channel reactor
        wasm_bindgen_futures::spawn_local(async move {
            let _ = reactor.run().await;
        });

        reporter.record_handshake_done();

        Ok(chan)
    }
}

#[async_trait]
impl ChannelFactory for SnowflakeChannelFactory {
    async fn connect_via_transport(
        &self,
        target: &OwnedChanTarget,
        reporter: BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        match &self.mode {
            SnowflakeMode::WebSocket { url, fingerprint } => {
                self.build_ws_channel(url, fingerprint, target, &reporter, memquota)
                    .await
            }
            SnowflakeMode::WebRtc {
                broker_url,
                fingerprint,
            } => {
                self.build_webrtc_channel(broker_url, fingerprint, target, &reporter, memquota)
                    .await
            }
        }
    }
}

/// Error type for Snowflake PT manager
#[derive(Debug, Clone)]
pub struct SnowflakePtError {
    message: String,
}

impl std::fmt::Display for SnowflakePtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snowflake PT error: {}", self.message)
    }
}

impl std::error::Error for SnowflakePtError {}

impl HasKind for SnowflakePtError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::TorAccessFailed
    }
}

impl HasRetryTime for SnowflakePtError {
    fn retry_time(&self) -> RetryTime {
        RetryTime::AfterWaiting
    }
}

impl AbstractPtError for SnowflakePtError {}

/// In-process Snowflake pluggable transport manager
///
/// This implements `AbstractPtMgr` to provide Snowflake transport
/// for arti-client without requiring an external PT binary.
pub struct SnowflakePtMgr {
    mode: SnowflakeMode,
}

impl SnowflakePtMgr {
    /// Create a new Snowflake PT manager
    pub fn new(mode: SnowflakeMode) -> Self {
        Self { mode }
    }
}

#[async_trait]
impl AbstractPtMgr for SnowflakePtMgr {
    async fn factory_for_transport(
        &self,
        transport: &PtTransportName,
    ) -> std::result::Result<Option<Arc<dyn ChannelFactory + Send + Sync>>, Arc<dyn AbstractPtError>>
    {
        let transport_name = transport.to_string();

        // Support "snowflake" transport name
        if transport_name == "snowflake" {
            info!(
                "Creating Snowflake channel factory for transport: {}",
                transport_name
            );
            let factory = SnowflakeChannelFactory::new(self.mode.clone());
            Ok(Some(Arc::new(factory)))
        } else {
            // Unknown transport
            debug!("Unknown transport requested: {}", transport_name);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pt_mgr_creation() {
        let _mgr = SnowflakePtMgr::new(SnowflakeMode::WebSocket {
            url: "wss://snowflake.torproject.net/".to_string(),
            fingerprint: "2B280B23E1107BB62ABFC40DDCC8824814F80A72".to_string(),
        });
        let _mgr = SnowflakePtMgr::new(SnowflakeMode::WebRtc {
            broker_url: "https://snowflake-broker.torproject.net/".to_string(),
            fingerprint: "2B280B23E1107BB62ABFC40DDCC8824814F80A72".to_string(),
        });
    }
}