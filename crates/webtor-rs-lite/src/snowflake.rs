//! Snowflake bridge implementation for Tor connections
//!
//! Snowflake is a pluggable transport that routes traffic through volunteer
//! proxies using WebRTC. The protocol stack is:
//!
//!   WebRTC DataChannel (to volunteer proxy)
//!       ↓
//!   Turbo (framing + obfuscation)
//!       ↓
//!   KCP (reliability + ordering)
//!       ↓
//!   SMUX (stream multiplexing)
//!       ↓
//!   Tor protocol
//!
//! Note: Direct WebSocket to wss://snowflake.torproject.net/ is for volunteer
//! proxies, not clients. Clients must use WebRTC via the broker.

#![cfg(target_arch = "wasm32")]

use crate::error::Result;
use crate::kcp_stream::{KcpConfig, KcpStream};
use crate::smux::SmuxStream;
use crate::turbo::TurboStream;
use futures::{AsyncRead, AsyncWrite};
use futures::io::AsyncWriteExt;
use std::borrow::Cow;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{info, warn};

use crate::webrtc_stream::WebRtcStream;

use futures_rustls::rustls;
use std::sync::Arc;

/// Snowflake bridge configuration
#[derive(Debug, Clone)]
pub struct SnowflakeConfig {
    /// Broker URL for WebRTC signaling
    pub broker_url: String,
    /// Bridge fingerprint for verification
    pub fingerprint: String,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// KCP conversation ID (0 for Snowflake)
    pub kcp_conv: Option<u32>,
    /// SMUX stream ID (default: 3)
    pub smux_stream_id: Option<u32>,
}

impl SnowflakeConfig {
    pub fn new(broker_url: String, fingerprint: String) -> Self {
        Self {
            broker_url,
            fingerprint,
            connection_timeout: Duration::from_secs(60),
            kcp_conv: None,
            smux_stream_id: None,
        }
    }

    /// Set connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set SMUX stream ID
    pub fn with_stream_id(mut self, stream_id: u32) -> Self {
        self.smux_stream_id = Some(stream_id);
        self
    }
}

/// Snowflake bridge connection manager
pub struct SnowflakeBridge {
    #[allow(dead_code)] // Used in wasm32 target
    config: SnowflakeConfig,
}

impl SnowflakeBridge {
    /// Create with custom configuration
    pub fn with_config(config: SnowflakeConfig) -> Self {
        Self { config }
    }

    /// Connect to the Snowflake bridge via WebRTC
    pub async fn connect(&self) -> Result<SnowflakeStream> {
        use crate::error::TorError;

        const MAX_WEBRTC_RETRIES: u32 = 3;

        info!("Connecting to Snowflake via WebRTC");
        info!("Broker: {}", self.config.broker_url);
        info!("Fingerprint: {}", self.config.fingerprint);

        // 1. Establish WebRTC connection via broker (with retry for unreliable proxies)
        let mut webrtc = None;
        let mut last_error = None;

        for attempt in 1..=MAX_WEBRTC_RETRIES {
            info!(
                "Connecting to volunteer proxy via WebRTC (attempt {}/{})...",
                attempt, MAX_WEBRTC_RETRIES
            );

            match WebRtcStream::connect(&self.config.broker_url, &self.config.fingerprint).await {
                Ok(stream) => {
                    info!("WebRTC DataChannel established on attempt {}", attempt);
                    webrtc = Some(stream);
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    warn!("WebRTC connection attempt {} failed: {}", attempt, err_str);
                    last_error = Some(e);

                    // Only retry on timeout errors (proxy didn't respond)
                    if !err_str.contains("timeout") {
                        return Err(last_error.unwrap());
                    }

                    if attempt < MAX_WEBRTC_RETRIES {
                        info!("Retrying with a different volunteer proxy...");
                    }
                }
            }
        }

        let webrtc = webrtc.ok_or_else(|| {
            last_error.unwrap_or_else(|| {
                TorError::Network("WebRTC connection failed after all retries".to_string())
            })
        })?;
        info!("WebRTC DataChannel established");

        // 2. Wrap with Turbo framing
        info!("Initializing Turbo layer...");
        let mut turbo = TurboStream::new(webrtc);
        turbo.initialize().await?;
        info!("Turbo layer initialized");

        // 3. Wrap with KCP for reliability
        info!("Initializing KCP layer...");
        let kcp_config = KcpConfig {
            conv: self.config.kcp_conv.unwrap_or(0),
            ..Default::default()
        };
        let kcp = KcpStream::new(turbo, kcp_config);
        info!("KCP layer initialized");

        // 4. Wrap with SMUX for multiplexing
        info!("Initializing SMUX layer...");
        let stream_id = self.config.smux_stream_id.unwrap_or(3);
        let mut smux = SmuxStream::with_stream_id(kcp, stream_id);
        smux.initialize().await?;
        info!("SMUX layer initialized");

        // 5. Wrap with TLS for Tor link encryption (using rustls with skip-verification)
        info!("Establishing TLS over SMUX...");
        let connector = make_tor_tls_connector();
        // Use a placeholder server name since Tor doesn't use SNI
        let server_name = rustls_pki_types::ServerName::try_from("www.example.com".to_string())
            .map_err(|e| crate::error::TorError::tls(format!("Invalid server name: {}", e)))?;
        let tls_stream = connector
            .connect(server_name, smux)
            .await
            .map_err(|e| crate::error::TorError::tls(format!("TLS handshake failed: {}", e)))?;
        info!("TLS layer established over SMUX");

        info!("Snowflake connection established: WebRTC → Turbo → KCP → SMUX → TLS");

        Ok(SnowflakeStream {
            inner: SnowflakeInner::WebRtc(tls_stream),
        })
    }
}

/// Inner stream type (WebRTC on WASM, wrapped with TLS)
type SnowflakeSmuxStack = SmuxStream<KcpStream<TurboStream<WebRtcStream>>>;

enum SnowflakeInner {
    WebRtc(futures_rustls::client::TlsStream<SnowflakeSmuxStack>),
}

/// Snowflake stream for Tor communication
pub struct SnowflakeStream {
    inner: SnowflakeInner,
}

// Safety: WASM is single-threaded. Native streams handle their own thread safety.
unsafe impl Send for SnowflakeStream {}

impl tor_rtcompat::StreamOps for SnowflakeStream {
    // Use default implementation
}

impl tor_rtcompat::CertifiedConn for SnowflakeStream {
    fn peer_certificate(&self) -> io::Result<Option<Cow<'_, [u8]>>> {
        match &self.inner {
            SnowflakeInner::WebRtc(tls) => {
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
            SnowflakeInner::WebRtc(tls) => {
                let (_, session) = tls.get_ref();
                session
                    .export_keying_material(Vec::with_capacity(len), label, context)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
        }
    }
}

impl SnowflakeStream {
    /// Close the Snowflake stream
    pub async fn close(&mut self) -> io::Result<()> {
        info!("Closing Snowflake stream");
        match &mut self.inner {
            SnowflakeInner::WebRtc(tls) => AsyncWriteExt::close(tls).await,
        }
    }
}

impl AsyncRead for SnowflakeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_read(_cx, _buf),
        }
    }
}

impl AsyncWrite for SnowflakeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_write(_cx, _buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_flush(_cx),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            SnowflakeInner::WebRtc(tls) => Pin::new(tls).poll_close(_cx),
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

/// Create a Snowflake stream with full configuration
pub async fn create_snowflake_stream_with_config(
    config: SnowflakeConfig,
) -> Result<SnowflakeStream> {
    let bridge = SnowflakeBridge::with_config(config);
    bridge.connect().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::portable_test;

    #[portable_test]
    fn test_snowflake_config_defaults() {
        let config = SnowflakeConfig::new("https://example.com/broker".to_string(), String::new());
        assert_eq!(config.connection_timeout, Duration::from_secs(60));
    }

    #[portable_test]
    fn test_snowflake_config_with_timeout() {
        let config = SnowflakeConfig::new("https://example.com/broker".to_string(), String::new())
            .with_timeout(Duration::from_secs(120));
        assert_eq!(config.connection_timeout, Duration::from_secs(120));
    }

    #[portable_test]
    fn test_snowflake_config_fingerprint() {
        let config = SnowflakeConfig::new("https://example.com/broker".to_string(), "ABCD1234".to_string());
        assert_eq!(config.fingerprint, "ABCD1234");
    }
}
