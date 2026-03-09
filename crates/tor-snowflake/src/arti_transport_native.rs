//! Native Arti-compatible transport for Snowflake bridges
//!
//! This module provides integration with arti-client by implementing
//! `ChannelFactory` and `AbstractPtMgr` for Snowflake transports on native (non-WASM).

#![cfg(not(target_arch = "wasm32"))]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use safelog::{Sensitive, MaybeSensitive};
use tor_chanmgr::factory::{AbstractPtError, AbstractPtMgr, BootstrapReporter, ChannelFactory};
use tor_error::{ErrorKind, HasKind, HasRetryTime, RetryTime};
use tor_linkspec::{HasRelayIds, IntoOwnedChanTarget, OwnedChanTarget, OwnedChanTargetBuilder, PtTransportName};
use tor_llcrypto::pk::rsa::RsaIdentity;
use tor_proto::channel::Channel;
use tor_proto::memquota::ChannelAccount;
use tor_proto::peer::PeerAddr;
use tor_proto::ClientChannelBuilder;
use tor_rtcompat::{Runtime, SpawnExt};
use tor_time::SystemTime;
use tracing::{debug, info};

use crate::snowflake_ws_native::{SnowflakeWsConfig, SnowflakeWsStream};

/// Snowflake channel factory that builds Tor channels over Snowflake transport (native)
pub struct SnowflakeChannelFactory<R: Runtime> {
    url: String,
    fingerprint: String,
    runtime: R,
}

impl<R: Runtime> SnowflakeChannelFactory<R> {
    pub fn new(runtime: R, url: impl Into<String>, fingerprint: String) -> Self {
        Self {
            url: url.into(),
            fingerprint,
            runtime,
        }
    }

    /// Build a channel using WebSocket Snowflake
    async fn build_channel(
        &self,
        _target: &OwnedChanTarget,
        reporter: &BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!("Building native Snowflake channel via WebSocket: {}", self.url);

        reporter.record_attempt();

        // Configure WebSocket Snowflake
        let config = SnowflakeWsConfig::new(&self.url, self.fingerprint.clone());

        // Connect via WebSocket
        let stream = SnowflakeWsStream::connect(config)
            .await
            .map_err(|e| tor_chanmgr::Error::Io {
                action: "Snowflake WebSocket connect",
                peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                )),
                source: std::io::Error::other(e.to_string()).into(),
            })?;

        // The Snowflake stream handles transport + TLS internally
        reporter.record_tcp_success();
        reporter.record_tls_finished();

        // Parse fingerprint to RSA identity — fail immediately if format is invalid
        let rsa_id = hex::decode(&self.fingerprint)
            .ok()
            .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
            .ok_or_else(|| tor_chanmgr::Error::Io {
                action: "parse bridge fingerprint",
                peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                )),
                source: std::io::Error::other(format!(
                    "Invalid bridge fingerprint '{}': must be a 40-char hex string",
                    self.fingerprint
                )).into(),
            })?;

        // Get peer certificate from TLS stream
        let peer_cert = stream.peer_certificate().map_err(|e| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
            )),
            source: e.into(),
        })?;

        let peer_cert = peer_cert.ok_or_else(|| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
            )),
            source: std::io::Error::other("No peer certificate from TLS")
                .into(),
        })?;

        debug!("Got peer certificate: {} bytes", peer_cert.len());

        // Launch Tor channel handshake
        let builder = ClientChannelBuilder::new();
        debug!("Launching Tor channel client handshake...");
        let handshake = builder.launch(stream, self.runtime.clone(), memquota);

        debug!("Starting handshake connect...");

        // Build peer target for error reporting and verification
        let mut peer_builder = OwnedChanTargetBuilder::default();
        peer_builder.rsa_identity(rsa_id);

        let peer = peer_builder.build().map_err(|e| {
            tor_chanmgr::Error::Internal(tor_error::internal!(
                "Failed to build peer target: {}",
                e
            ))
        })?;

        let now_fn = || SystemTime::now();
        let unverified = handshake.connect(now_fn).await.map_err(|e| {
            tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            }
        })?;

        debug!("Handshake connect completed, verifying...");

        // Verify channel and finish handshake
        let verified = unverified
            .verify(&peer, &peer_cert, Some(SystemTime::now()))
            .map_err(|e| tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            })?;

        let peer_addr = Sensitive::new(PeerAddr::Direct("0.0.0.0:0".parse().unwrap()));
        let (chan, reactor) = verified.finish(peer_addr).await.map_err(|e| tor_chanmgr::Error::Proto {
            source: e,
            peer: peer.to_logged(),
            clock_skew: None,
        })?;

        // Spawn the channel reactor using SpawnExt trait
        self.runtime.spawn(async move {
            let _ = reactor.run().await;
        }).map_err(|e| tor_chanmgr::Error::Spawn {
            spawning: "channel reactor",
            cause: Arc::new(e),
        })?;

        reporter.record_handshake_done();

        // Explicitly verify the bridge's RSA identity matches our expected fingerprint.
        // The Tor handshake verify() may not enforce RSA identity strictly (ed25519 is
        // primary in modern Tor), so we do our own strict check here.
        let actual_rsa_id = chan.target().rsa_identity()
            .ok_or_else(|| tor_chanmgr::Error::Io {
                action: "verify bridge fingerprint",
                peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                )),
                source: std::io::Error::other("Bridge did not present an RSA identity").into(),
            })?;

        let actual_fp = hex::encode(actual_rsa_id.as_bytes()).to_uppercase();
        let expected_fp = self.fingerprint.to_uppercase();

        if actual_fp != expected_fp {
            return Err(tor_chanmgr::Error::Io {
                action: "verify bridge fingerprint",
                peer: MaybeSensitive::not_sensitive(PeerAddr::Direct(
                    SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                )),
                source: std::io::Error::other(format!(
                    "Bridge fingerprint mismatch: expected {}, got {}",
                    expected_fp, actual_fp
                )).into(),
            });
        }

        Ok(chan)
    }
}

#[async_trait]
impl<R: Runtime> ChannelFactory for SnowflakeChannelFactory<R> {
    async fn connect_via_transport(
        &self,
        target: &OwnedChanTarget,
        reporter: BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        self.build_channel(target, &reporter, memquota).await
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

/// In-process Snowflake pluggable transport manager (native)
///
/// This implements `AbstractPtMgr` to provide Snowflake transport
/// for arti-client without requiring an external PT binary.
pub struct SnowflakePtMgr<R: Runtime> {
    url: String,
    fingerprint: String,
    runtime: R,
}

impl<R: Runtime> SnowflakePtMgr<R> {
    pub fn new(runtime: R, url: impl Into<String>, fingerprint: String) -> Self {
        Self {
            url: url.into(),
            fingerprint,
            runtime,
        }
    }
}

#[async_trait]
impl<R: Runtime> AbstractPtMgr for SnowflakePtMgr<R> {
    async fn factory_for_transport(
        &self,
        transport: &PtTransportName,
    ) -> std::result::Result<Option<Arc<dyn ChannelFactory + Send + Sync>>, Arc<dyn AbstractPtError>>
    {
        let transport_name = transport.to_string();

        // Support "snowflake" transport name
        if transport_name == "snowflake" {
            info!(
                "Creating native Snowflake channel factory for transport: {}",
                transport_name
            );
            let factory = SnowflakeChannelFactory::new(self.runtime.clone(), &self.url, self.fingerprint.clone());
            Ok(Some(Arc::new(factory)))
        } else {
            // Unknown transport
            debug!("Unknown transport requested: {}", transport_name);
            Ok(None)
        }
    }
}
