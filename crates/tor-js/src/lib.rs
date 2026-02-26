//! tor-js: WebAssembly bindings for arti-client
//!
//! This crate provides JavaScript bindings for making HTTP requests through Tor
//! using arti-client (the official Tor Project client library).
//!
//! # Example
//!
//! ```javascript
//! import { init, TorClient, TorClientOptions } from 'tor-js';
//!
//! // Initialize the WASM module
//! init();
//!
//! // Create client options with Snowflake transport
//! // The fingerprint is required for bridge verification
//! const options = new TorClientOptions(
//!   'wss://snowflake.pse.dev/',
//!   '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'  // pse.dev bridge fingerprint
//! );
//!
//! // Create the Tor client (async)
//! const client = await TorClient.create(options);
//!
//! // Make a fetch request through Tor
//! const response = await client.fetch('https://check.torproject.org/api/ip');
//! console.log(await response.text());
//!
//! // Clean up
//! await client.close();
//! ```

#![cfg(target_arch = "wasm32")]

mod error;
mod fetch;
mod storage;

pub use storage::{JsStorage, JsStorageInterface, CachedJsStorage};

use error::JsTorError;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arti_client::config::{BridgeConfigBuilder, CfgPath, pt::TransportConfigBuilder};
use arti_client::{TorClient as ArtiTorClient, TorClientConfig};
use serde::Deserialize;
use tor_rtcompat::wasm::WasmRuntime;
use tracing::{debug, info, error};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use wasm_bindgen::prelude::*;
use tor_snowflake::arti_transport::{SnowflakeMode, SnowflakePtMgr};

// Global log callback (WASM is single-threaded, so thread_local is fine)
thread_local! {
    static LOG_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static LOG_LEVEL_HANDLE: RefCell<Option<tracing_subscriber::reload::Handle<
        tracing_subscriber::filter::LevelFilter,
        tracing_subscriber::Registry,
    >>> = const { RefCell::new(None) };
}

/// Parse a log level string into a `LevelFilter`.
fn parse_level(level: &str) -> Result<tracing_subscriber::filter::LevelFilter, JsValue> {
    match level {
        "trace" => Ok(tracing_subscriber::filter::LevelFilter::TRACE),
        "debug" => Ok(tracing_subscriber::filter::LevelFilter::DEBUG),
        "info" => Ok(tracing_subscriber::filter::LevelFilter::INFO),
        "warn" => Ok(tracing_subscriber::filter::LevelFilter::WARN),
        "error" => Ok(tracing_subscriber::filter::LevelFilter::ERROR),
        other => Err(JsValue::from_str(&format!(
            "Invalid log level: {:?}. Must be trace, debug, info, warn, or error.",
            other
        ))),
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the tor-js WASM module
///
/// This must be called before creating any TorClient instances.
/// Sets up panic hooks and logging infrastructure.
///
/// The optional `log_level` parameter sets the initial log level:
/// "trace", "debug", "info", "warn", or "error". Defaults to "debug".
/// The level can be changed later with `setLogLevel()`.
#[wasm_bindgen]
pub fn init(log_level: Option<String>) -> Result<(), JsValue> {
    // Set up panic handler for better error messages
    console_error_panic_hook::set_once();

    let initial_filter = match log_level.as_deref() {
        Some(level) => parse_level(level)?,
        None => tracing_subscriber::filter::LevelFilter::DEBUG,
    };

    // Create a reloadable filter so the level can be changed dynamically
    let (filter, reload_handle) = tracing_subscriber::reload::Layer::new(initial_filter);

    // Use try_init() to avoid panicking if init() is called more than once
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(JsLogLayer)
        .try_init();

    LOG_LEVEL_HANDLE.with(|h| {
        *h.borrow_mut() = Some(reload_handle);
    });

    Ok(())
}

/// Dynamically update the minimum log level.
///
/// Called from JS when the broadest requested level across all clients changes.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: String) -> Result<(), JsValue> {
    let new_filter = parse_level(&level)?;
    LOG_LEVEL_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            handle.modify(|filter| *filter = new_filter)
                .map_err(|e| JsValue::from_str(&format!("Failed to update log level: {}", e)))
        } else {
            Err(JsValue::from_str("init() must be called before setLogLevel()"))
        }
    })
}

/// Set a callback function to receive log messages
///
/// The callback receives three arguments: (level: string, target: string, message: string)
#[wasm_bindgen(js_name = setLogCallback)]
pub fn set_log_callback(callback: js_sys::Function) {
    LOG_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(callback);
    });
}

/// Custom tracing layer that forwards logs to JavaScript
struct JsLogLayer;

impl<S> tracing_subscriber::Layer<S> for JsLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Extract event data
        let level = match *event.metadata().level() {
            tracing::Level::TRACE => "trace",
            tracing::Level::DEBUG => "debug",
            tracing::Level::INFO => "info",
            tracing::Level::WARN => "warn",
            tracing::Level::ERROR => "error",
        };
        let target = event.metadata().target();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        // Try to call the JavaScript callback
        LOG_CALLBACK.with(|cb| {
            if let Some(callback) = cb.borrow().as_ref() {
                let _ = callback.call3(
                    &JsValue::NULL,
                    &JsValue::from_str(level),
                    &JsValue::from_str(target),
                    &JsValue::from_str(&visitor.message),
                );
            } else {
                // Fall back to console.log if no callback set
                web_sys::console::log_1(&format!("[{}] {}: {}", level, target, visitor.message).into());
            }
        });
    }
}

/// Visitor to extract message from tracing events
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // Remove surrounding quotes if present
            self.message = self.message.trim_matches('"').to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {:?}", field.name(), value);
        } else {
            self.message.push_str(&format!(", {} = {:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {}", field.name(), value);
        } else {
            self.message.push_str(&format!(", {} = {}", field.name(), value));
        }
    }
}

// ============================================================================
// TorClientOptions
// ============================================================================

/// Options for creating a TorClient
#[wasm_bindgen]
pub struct TorClientOptions {
    mode: SnowflakeMode,
    /// Custom storage implementation (optional)
    storage: Option<JsStorageInterface>,
}

#[wasm_bindgen]
impl TorClientOptions {
    /// Create options for WebSocket Snowflake transport
    ///
    /// # Arguments
    /// * `snowflake_url` - WebSocket URL for the Snowflake bridge (e.g., "wss://snowflake.pse.dev/")
    /// * `fingerprint` - Bridge fingerprint (40 char hex string).
    #[wasm_bindgen(constructor)]
    pub fn new(snowflake_url: String, fingerprint: String) -> Self {
        Self {
            mode: SnowflakeMode::WebSocket {
                url: snowflake_url,
                fingerprint,
            },
            storage: None,
        }
    }

    /// Create options for WebRTC Snowflake transport (via broker)
    ///
    /// # Arguments
    /// * `broker_url` - Snowflake broker URL (e.g., "https://snowflake-broker.torproject.net/").
    /// * `fingerprint` - Bridge fingerprint (40 char hex string).
    #[wasm_bindgen(js_name = snowflakeWebRtc)]
    pub fn snowflake_webrtc(broker_url: String, fingerprint: String) -> Self {
        Self {
            mode: SnowflakeMode::WebRtc {
                broker_url,
                fingerprint,
            },
            storage: None,
        }
    }

    /// Set a custom storage implementation for persistent state.
    ///
    /// When set, the Tor client will persist guard selection and other state
    /// to this storage, allowing faster reconnection across page reloads.
    ///
    /// If not set, in-memory storage is used (state lost on page reload).
    ///
    /// # Arguments
    /// * `storage` - A JavaScript object implementing the TorStorage interface
    #[wasm_bindgen(js_name = withStorage)]
    pub fn with_storage(mut self, storage: JsStorageInterface) -> Self {
        self.storage = Some(storage);
        self
    }
}

// ============================================================================
// TorClient
// ============================================================================

/// Tor client for making HTTP requests through the Tor network
#[wasm_bindgen]
pub struct TorClient {
    inner: Option<Arc<ArtiTorClient<WasmRuntime>>>,
    tls_config: Arc<futures_rustls::rustls::ClientConfig>,
    /// Set by SnowflakeChannelFactory when WebSocket connection fails/succeeds.
    /// Checked by ready() to fail fast when the bridge is unreachable.
    ws_connect_error: Arc<Mutex<Option<String>>>,
}

#[wasm_bindgen]
impl TorClient {
    /// Create a new TorClient with the given options.
    ///
    /// This is an async operation that returns a Promise.
    /// The client will bootstrap and establish a connection to the Tor network.
    ///
    /// Usage from JS: `const client = await TorClient.create(options);`
    pub fn create(options: TorClientOptions) -> js_sys::Promise {
        wasm_bindgen_futures::future_to_promise(async move {
            let client = create_client(options).await?;
            Ok(JsValue::from(client))
        })
    }

    /// Make an HTTP fetch request through Tor
    ///
    /// Returns a Promise that resolves to a standard browser `Response` object
    /// as soon as response headers are received. The body is a `ReadableStream`
    /// that reads from the Tor circuit on demand.
    #[wasm_bindgen(js_name = fetch, skip_typescript)]
    pub fn fetch(&self, url: String, init: JsValue) -> js_sys::Promise {
        let client = match &self.inner {
            Some(c) => Arc::clone(c),
            None => {
                return wasm_bindgen_futures::future_to_promise(async {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };

        let tls_config = Arc::clone(&self.tls_config);
        wasm_bindgen_futures::future_to_promise(async move {
            fetch_impl(&client, &url, init, tls_config).await
        })
    }

    /// Wait until the client is ready for traffic (connection usable + valid directory).
    ///
    /// Fails fast if the WebSocket connection to the bridge has failed. The error
    /// is consumed so the next call will wait again (allowing retry).
    #[wasm_bindgen(js_name = ready)]
    pub fn ready(&self) -> js_sys::Promise {
        let client = match &self.inner {
            Some(c) => Arc::clone(c),
            None => {
                return wasm_bindgen_futures::future_to_promise(async {
                    Err(JsTorError::not_initialized().into_js_value())
                });
            }
        };
        let ws_error = Arc::clone(&self.ws_connect_error);

        wasm_bindgen_futures::future_to_promise(async move {
            use futures::future::{select, Either};
            use futures::StreamExt;
            use std::pin::pin;

            // Fast path: already ready
            if client.bootstrap_status().ready_for_traffic() {
                return Ok(JsValue::undefined());
            }

            // Poll bootstrap events until ready, checking for WS errors periodically
            let mut events = client.bootstrap_events();
            loop {
                // Check for WS connection failure (consume the error)
                if let Some(msg) = ws_error.lock().unwrap().take() {
                    return Err(JsTorError::connection(
                        format!("WebSocket connection failed: {}", msg)
                    ).into_js_value());
                }

                // Wait for next bootstrap event, with periodic timeout to recheck WS error
                let next = pin!(events.next());
                let timeout = pin!(gloo_timers::future::TimeoutFuture::new(500));
                match select(next, timeout).await {
                    Either::Left((Some(status), _)) => {
                        if status.ready_for_traffic() {
                            return Ok(JsValue::undefined());
                        }
                    }
                    Either::Left((None, _)) => {
                        return Err(JsTorError::bootstrap(
                            "Client failed to become ready for traffic"
                        ).into_js_value());
                    }
                    Either::Right(_) => continue, // timeout → loop back to check WS error
                }
            }
        })
    }

    /// Close the TorClient and release resources
    #[wasm_bindgen(js_name = close)]
    pub fn close(&mut self) -> js_sys::Promise {
        self.inner = None;
        wasm_bindgen_futures::future_to_promise(async {
            info!("TorClient closed");
            Ok(JsValue::undefined())
        })
    }
}

/// Create a TorClient with the given options
async fn create_client(options: TorClientOptions) -> Result<TorClient, JsValue> {
    debug!("tor-js {} (git: {})", env!("TOR_JS_VERSION"), env!("TOR_JS_GIT_INFO"));
    info!("Creating TorClient with arti-client...");

    // 1. Create Snowflake PT manager from tor-snowflake
    // Extract fingerprint before moving mode into SnowflakePtMgr.
    let bridge_fingerprint = match &options.mode {
        SnowflakeMode::WebSocket { fingerprint, .. } => fingerprint.clone(),
        SnowflakeMode::WebRtc { fingerprint, .. } => fingerprint.clone(),
    };
    let ws_connect_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let snowflake_mgr = SnowflakePtMgr::new(options.mode, Arc::clone(&ws_connect_error));
    info!("Created Snowflake PT manager");

    // 2. Configure arti-client with Snowflake bridge
    let mut config_builder = TorClientConfig::builder();

    // Storage paths (required by config validation, but not used on WASM)
    config_builder
        .storage()
        .cache_dir(CfgPath::new("/wasm/cache".to_owned()))
        .state_dir(CfgPath::new("/wasm/state".to_owned()));

    // Configure the Snowflake bridge
    let bridge_line = format!("snowflake 0.0.2.0:1 {}", bridge_fingerprint);
    debug!("Using bridge line: {}", bridge_line);

    let bridge: BridgeConfigBuilder = bridge_line
        .parse()
        .map_err(|e| JsTorError::config(format!("Failed to parse bridge line: {}", e)).into_js_value())?;
    config_builder.bridges().bridges().push(bridge);

    // Add transport config for "snowflake"
    let mut transport = TransportConfigBuilder::default();
    transport
        .protocols(vec!["snowflake"
            .parse()
            .map_err(|e| JsTorError::config(format!("Failed to parse protocol: {}", e)).into_js_value())?])
        .proxy_addr(
            "127.0.0.1:1"
                .parse()
                .map_err(|e| JsTorError::config(format!("Failed to parse proxy addr: {}", e)).into_js_value())?,
        );
    config_builder.bridges().transports().push(transport);

    let config = config_builder
        .build()
        .map_err(|e| JsTorError::config(format!("Failed to build config: {}", e)).into_js_value())?;
    info!("Configuration built with Snowflake bridge");

    // 3. Create TorClient with WASM runtime
    let runtime = WasmRuntime::default();

    // Build the client with optional custom storage
    let mut builder = ArtiTorClient::with_runtime(runtime).config(config);

    // Set up custom storage if provided
    if let Some(js_storage_interface) = options.storage {
        info!("Initializing JS storage...");
        let js_storage = JsStorage::new(js_storage_interface);

        let cached_storage = CachedJsStorage::new(js_storage)
            .await
            .map_err(|e| {
                JsTorError::internal(format!("Failed to initialize storage: {:?}", e)).into_js_value()
            })?;

        builder = builder.storage(cached_storage);
        info!("Storage configured (state + directory cache)");
    } else {
        error!("Storage not configured");
    }

    let tor_client = builder
        .create_unbootstrapped()
        .map_err(|e| JsTorError::internal(format!("Failed to create client: {}", e)).into_js_value())?;

    info!("TorClient created (unbootstrapped)");

    // 4. Inject PT manager (requires experimental-api feature)
    tor_client.chanmgr().set_pt_mgr(Arc::new(snowflake_mgr));
    info!("Snowflake PT manager injected into ChanMgr");

    // 5. Bootstrap the client
    info!("Bootstrapping Tor client via Snowflake...");
    tor_client
        .bootstrap()
        .await
        .map_err(|e| JsTorError::bootstrap(format!("Bootstrap failed: {}", e)).into_js_value())?;
    info!("Bootstrap complete!");

    Ok(TorClient {
        inner: Some(Arc::new(tor_client)),
        tls_config: make_tls_config(),
        ws_connect_error,
    })
}

/// Build a rustls ClientConfig with the Mozilla CA bundle (compiled in via webpki-roots)
/// and the pure-Rust crypto provider (rustls-rustcrypto) for WASM compatibility.
fn make_tls_config() -> Arc<futures_rustls::rustls::ClientConfig> {
    use futures_rustls::rustls;

    let provider = rustls_rustcrypto::provider();
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

// ============================================================================
// Fetch Implementation
// ============================================================================

/// Fetch init options from JavaScript
#[derive(Debug, Default, Deserialize)]
struct FetchInit {
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    #[serde(skip)]
    body: Option<Vec<u8>>,
}

/// Perform a fetch request, returning a real browser `Response` object.
///
/// The Response is created as soon as headers arrive. Its body is a
/// `ReadableStream` that reads decoded body bytes from the Tor circuit
/// on demand (handling Content-Length, chunked, and EOF framing).
async fn fetch_impl(
    client: &ArtiTorClient<WasmRuntime>,
    url_str: &str,
    init: JsValue,
    tls_config: Arc<futures_rustls::rustls::ClientConfig>,
) -> Result<JsValue, JsValue> {
    // Parse URL
    let url = url::Url::parse(url_str)
        .map_err(|e| JsTorError::new("INVALID_URL", "validation", e.to_string(), false).into_js_value())?;

    // Parse fetch options
    let mut fetch_init: FetchInit = if init.is_undefined() || init.is_null() {
        FetchInit::default()
    } else {
        serde_wasm_bindgen::from_value(init.clone())
            .map_err(|e| JsTorError::new("INVALID_OPTIONS", "validation", e.to_string(), false).into_js_value())?
    };

    // Extract body and signal separately (JS objects; not handled by serde)
    if !init.is_undefined() && !init.is_null() {
        fetch_init.body = extract_body_from_js(&init)?;
    }
    let signal = extract_signal_from_js(&init)?;

    // Parse method
    let method = match fetch_init.method.as_deref() {
        Some("GET") | None => http::Method::GET,
        Some("POST") => http::Method::POST,
        Some("PUT") => http::Method::PUT,
        Some("DELETE") => http::Method::DELETE,
        Some("HEAD") => http::Method::HEAD,
        Some("OPTIONS") => http::Method::OPTIONS,
        Some("PATCH") => http::Method::PATCH,
        Some(other) => {
            return Err(JsTorError::new(
                "INVALID_METHOD",
                "validation",
                format!("Unsupported HTTP method: {}", other),
                false,
            )
            .into_js_value());
        }
    };

    let headers = fetch_init.headers.unwrap_or_default();
    let body = fetch_init.body;

    // Get host and port
    let host = url
        .host_str()
        .ok_or_else(|| JsTorError::new("INVALID_URL", "validation", "No host in URL", false).into_js_value())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| JsTorError::new("INVALID_URL", "validation", "No port in URL", false).into_js_value())?;
    let is_https = url.scheme() == "https";

    info!("Fetching {} via Tor ({}:{})", url, host, port);

    // Check abort before connecting (circuit building can take several seconds)
    check_aborted(signal.as_ref())?;

    // Connect through Tor
    debug!("Connecting to {}:{}...", host, port);
    let stream = client
        .connect((host, port))
        .await
        .map_err(|e| JsTorError::connection(format!("Failed to connect: {}", e)).into_js_value())?;

    // Check abort before sending the HTTP request
    check_aborted(signal.as_ref())?;

    debug!("Connected, making HTTP request...");

    // Perform the HTTP request — resolves as soon as headers arrive
    let result = fetch::fetch_headers(stream, &url, method, headers, body, is_https, host, Some(tls_config))
        .await
        .map_err(|e| e.into_js_value())?;

    // Build web_sys::Headers from the parsed headers
    let js_headers = web_sys::Headers::new()
        .map_err(|e| JsTorError::internal(format!("Failed to create Headers: {:?}", e)).into_js_value())?;
    for (key, value) in &result.headers {
        js_headers.append(key, value)
            .map_err(|e| JsTorError::internal(format!("Failed to set header {}: {:?}", key, e)).into_js_value())?;
    }

    // Convert BodyReader → futures::Stream → JS ReadableStream
    // The signal is threaded through the unfold state so abort is checked between chunks.
    let body_stream = futures::stream::unfold((result.body_reader, signal), |(mut reader, sig)| async move {
        if let Some(s) = &sig {
            if s.aborted() {
                return Some((Err(JsTorError::aborted().into_js_value()), (reader, sig)));
            }
        }
        match reader.read_chunk().await {
            Ok(Some(chunk)) => {
                let arr = js_sys::Uint8Array::from(&chunk[..]);
                Some((Ok(arr.into()), (reader, sig)))
            }
            Ok(None) => None,
            Err(e) => Some((Err(e.into_js_value()), (reader, sig))),
        }
    });
    let readable = wasm_streams::ReadableStream::from_stream(body_stream);
    let raw_readable: web_sys::ReadableStream = readable.into_raw();

    // Create ResponseInit with status + headers
    let response_init = web_sys::ResponseInit::new();
    response_init.set_status(result.status);
    response_init.set_headers(&js_headers.into());

    // Create the real browser Response
    let response = web_sys::Response::new_with_opt_readable_stream_and_init(
        Some(&raw_readable),
        &response_init,
    )
    .map_err(|e| JsTorError::internal(format!("Failed to create Response: {:?}", e)).into_js_value())?;

    Ok(response.into())
}

/// Extract body from JavaScript FetchInit object
fn extract_body_from_js(init: &JsValue) -> Result<Option<Vec<u8>>, JsValue> {
    let body = js_sys::Reflect::get(init, &JsValue::from_str("body"))
        .map_err(|e| JsTorError::new("INVALID_OPTIONS", "validation", format!("Failed to get body: {:?}", e), false).into_js_value())?;

    if body.is_undefined() || body.is_null() {
        return Ok(None);
    }

    // Handle string body
    if let Some(s) = body.as_string() {
        return Ok(Some(s.into_bytes()));
    }

    // Handle Uint8Array
    if let Ok(arr) = body.clone().dyn_into::<js_sys::Uint8Array>() {
        return Ok(Some(arr.to_vec()));
    }

    // Handle ArrayBuffer
    if let Ok(buf) = body.clone().dyn_into::<js_sys::ArrayBuffer>() {
        let arr = js_sys::Uint8Array::new(&buf);
        return Ok(Some(arr.to_vec()));
    }

    Err(JsTorError::new(
        "INVALID_BODY",
        "validation",
        "Body must be a string, Uint8Array, or ArrayBuffer",
        false,
    )
    .into_js_value())
}

/// Extract an AbortSignal from a JavaScript FetchInit object.
fn extract_signal_from_js(init: &JsValue) -> Result<Option<web_sys::AbortSignal>, JsValue> {
    if init.is_undefined() || init.is_null() {
        return Ok(None);
    }
    let signal = js_sys::Reflect::get(init, &JsValue::from_str("signal"))
        .map_err(|e| JsTorError::new("INVALID_OPTIONS", "validation", format!("Failed to get signal: {:?}", e), false).into_js_value())?;
    if signal.is_undefined() || signal.is_null() {
        return Ok(None);
    }
    signal
        .dyn_into::<web_sys::AbortSignal>()
        .map(Some)
        .map_err(|_| JsTorError::new("INVALID_OPTIONS", "validation", "signal must be an AbortSignal", false).into_js_value())
}

/// Return an abort error if the signal has already been triggered.
fn check_aborted(signal: Option<&web_sys::AbortSignal>) -> Result<(), JsValue> {
    if let Some(s) = signal {
        if s.aborted() {
            return Err(JsTorError::aborted().into_js_value());
        }
    }
    Ok(())
}

// ============================================================================
// TypeScript definitions
// ============================================================================

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = r#"
/**
 * Storage interface for persisting Tor client state.
 *
 * Implement this interface to provide custom storage (IndexedDB, filesystem, etc.).
 * All methods must return Promises.
 *
 * When storage is provided, the Tor client will persist guard selection and other
 * state, allowing faster reconnection across page reloads.
 *
 * @example
 * ```typescript
 * class IndexedDBStorage implements TorStorage {
 *   async get(key: string): Promise<string | null> {
 *     // Load from IndexedDB
 *   }
 *   async set(key: string, value: string): Promise<void> {
 *     // Save to IndexedDB
 *   }
 *   async delete(key: string): Promise<void> {
 *     // Delete from IndexedDB
 *   }
 *   async keys(prefix: string): Promise<string[]> {
 *     // List keys matching prefix
 *   }
 *   async tryLock(): Promise<boolean> {
 *     // addLocking is available in tor-js to solve locking with in-memory
 *     // overlay
 *     // true:   newly acquired
 *     // false:  already held
 *     // reject: couldn't lock
 *   }
 *   async unlock(): Promise<void> {
 *   }
 * }
 *
 * const options = new TorClientOptions(url, fingerprint)
 *   .withStorage(new IndexedDBStorage());
 * const client = await TorClient.create(options);
 * ```
 */
export interface TorStorage {
    /**
     * Get a value by key.
     * @param key - The storage key
     * @returns The stored value as a string, or null if not found
     */
    get(key: string): Promise<string | null>;

    /**
     * Get all key-value pairs matching a prefix.
     * @param prefix - The key prefix to match
     * @returns Array of [key, value] pairs
     */
    getAll(prefix: string): Promise<[string, string][]>;

    /**
     * Set a value by key.
     * @param key - The storage key
     * @param value - The value to store (JSON string)
     */
    set(key: string, value: string): Promise<void>;

    /**
     * Delete a value by key.
     * @param key - The storage key
     */
    delete(key: string): Promise<void>;

    /**
     * List all keys with a given prefix.
     * @param prefix - The key prefix to match
     * @returns Array of matching keys
     */
    keys(prefix: string): Promise<string[]>;

    /**
     * Try to acquire an exclusive write lock.
     * @returns true if newly acquired, false if already held.
     * Implement using Web Locks API (browser) or lock files (Node.js).
     */
    tryLock(): Promise<boolean>;

    /**
     * Release the write lock.
     */
    unlock(): Promise<void>;
}

export interface FetchInit {
    method?: string;
    headers?: Record<string, string>;
    body?: string | Uint8Array | ArrayBuffer;
    signal?: AbortSignal;
}

export interface TorClient {
    /** Make an HTTP fetch request through Tor. Returns a standard Response. */
    fetch(url: string, init?: FetchInit): Promise<Response>;
    close(): Promise<void>;
}

export interface TorClientOptions {
    /**
     * Set a custom storage implementation for persistent state.
     * If not provided, in-memory storage is used (state lost on page reload).
     */
    withStorage(storage: TorStorage): TorClientOptions;
}
"#;
