//! WebSocket-based Snowflake transport
//!
//! This module provides Snowflake connectivity using WebSocket instead of WebRTC.
//! This is simpler and more reliable in browsers since WebSocket has native support
//! without the complexity of WebRTC signaling.
//!
//! Protocol stack (bottom to top):
//!   WebSocket (wss://snowflake.torproject.net/)
//!       ↓
//!   Turbo (framing + obfuscation)
//!       ↓
//!   KCP (reliability + ordering)
//!       ↓
//!   SMUX (stream multiplexing)
//!       ↓
//!   TLS (link encryption)
//!       ↓
//!   Tor protocol

#![cfg(target_arch = "wasm32")]

use crate::error::{Result, TorError};
use crate::websocket::WebSocketStream;
use futures::{AsyncRead, AsyncWrite};
use std::borrow::Cow;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::info;

use crate::kcp_stream::{KcpConfig, KcpStream};
use crate::smux::SmuxStream;
use crate::turbo::TurboStream;
use futures_rustls::rustls;
use std::sync::Arc;

/// WebSocket Snowflake configuration
#[derive(Debug, Clone)]
pub struct SnowflakeWsConfig {
    /// WebSocket URL for Snowflake endpoint
    pub ws_url: String,
    /// Bridge fingerprint
    pub fingerprint: String,
    /// KCP conversation ID (0 for default)
    pub kcp_conv: u32,
    /// SMUX stream ID (default: 3)
    pub smux_stream_id: u32,
}

impl SnowflakeWsConfig {
    pub fn new(url: impl Into<String>, fingerprint: String) -> Self {
        Self {
            ws_url: url.into(),
            fingerprint,
            kcp_conv: 0,
            smux_stream_id: 3,
        }
    }
}

type SnowflakeWsStack = SmuxStream<KcpStream<TurboStream<WebSocketStream>>>;

enum SnowflakeWsInner {
    Connected(futures_rustls::client::TlsStream<SnowflakeWsStack>),
}

/// WebSocket-based Snowflake stream
pub struct SnowflakeWsStream {
    inner: SnowflakeWsInner,
}

// Safety: WASM is single-threaded
unsafe impl Send for SnowflakeWsStream {}

impl SnowflakeWsStream {
    /// Connect to Snowflake via WebSocket
    pub async fn connect(config: SnowflakeWsConfig) -> Result<Self> {
        info!("Connecting to Snowflake via WebSocket");
        info!("URL: {}", config.ws_url);
        info!("Fingerprint: {}", config.fingerprint);

        // 1. Establish WebSocket connection
        info!("Opening WebSocket connection...");
        let ws = WebSocketStream::connect(&config.ws_url).await?;
        info!("WebSocket connected");

        // 2. Wrap with Turbo framing
        info!("Initializing Turbo layer...");
        let mut turbo = TurboStream::new(ws);
        turbo.initialize().await?;
        info!("Turbo layer initialized");

        // 3. Wrap with KCP for reliability
        info!("Initializing KCP layer...");
        let kcp_config = KcpConfig {
            conv: config.kcp_conv,
            ..Default::default()
        };
        let kcp = KcpStream::new(turbo, kcp_config);
        info!("KCP layer initialized");

        // 4. Wrap with SMUX for multiplexing
        info!("Initializing SMUX layer...");
        let mut smux = SmuxStream::with_stream_id(kcp, config.smux_stream_id);
        smux.initialize().await?;
        info!("SMUX layer initialized");

        // 5. Wrap with TLS (using rustls with skip-verification for Tor)
        info!("Establishing TLS...");
        let connector = make_tor_tls_connector();
        let server_name = rustls_pki_types::ServerName::try_from("www.example.com".to_string())
            .map_err(|e| TorError::tls(format!("Invalid server name: {}", e)))?;
        info!("Snowflake: calling TLS connect...");
        let tls_result = connector.connect(server_name, smux).await;
        info!("Snowflake: TLS connect returned");
        let tls_stream =
            tls_result.map_err(|e| TorError::tls(format!("TLS handshake failed: {}", e)))?;
        info!("TLS layer established");

        info!("Snowflake WS connection established: WebSocket → Turbo → KCP → SMUX → TLS");

        Ok(Self {
            inner: SnowflakeWsInner::Connected(tls_stream),
        })
    }
}

impl tor_rtcompat::StreamOps for SnowflakeWsStream {}

impl tor_rtcompat::CertifiedConn for SnowflakeWsStream {
    fn peer_certificate(&self) -> io::Result<Option<Cow<'_, [u8]>>> {
        match &self.inner {
            SnowflakeWsInner::Connected(tls) => {
                let (_, session) = tls.get_ref();
                Ok(session
                    .peer_certificates()
                    .and_then(|certs| certs.first().map(|c| Cow::from(c.as_ref()))))
            }
        }
    }

    fn own_certificate(&self) -> io::Result<Option<Cow<'_, [u8]>>> {
        Ok(None)
    }

    fn export_keying_material(
        &self,
        len: usize,
        label: &[u8],
        context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        match &self.inner {
            SnowflakeWsInner::Connected(tls) => {
                let (_, session) = tls.get_ref();
                session
                    .export_keying_material(Vec::with_capacity(len), label, context)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
        }
    }
}

impl AsyncRead for SnowflakeWsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for SnowflakeWsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_flush(cx),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            SnowflakeWsInner::Connected(tls) => Pin::new(tls).poll_close(cx),
        }
    }
}

/// Create a TLS connector for Tor relay connections.
///
/// Skips certificate verification (Tor validates via CERTS cells) but
/// verifies TLS handshake signatures.
fn make_tor_tls_connector() -> futures_rustls::TlsConnector {
    let provider = rustls_rustcrypto::provider();
    let algorithms = provider.signature_verification_algorithms;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .expect("default protocol versions should be supported")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(
            tor_rtcompat::wasm::TorCertVerifier::new(algorithms),
        ))
        .with_no_client_auth();
    futures_rustls::TlsConnector::from(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::portable_test;

    #[portable_test]
    fn test_config_defaults() {
        let config = SnowflakeWsConfig::new("wss://test.example.com", "AAAA00000000000000000000000000000000000000".to_string());
        assert_eq!(config.kcp_conv, 0);
        assert_eq!(config.smux_stream_id, 3);
    }
}
