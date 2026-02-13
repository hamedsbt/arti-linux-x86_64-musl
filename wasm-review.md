# Code Review: WASM Arti Client

**Branch:** `wasm-arti-client`
**Stats:** 287 files changed, ~25.8k insertions, ~660 deletions (vs `zydou/main`)

## Overview

This branch adds WebAssembly/browser support to the Arti Tor client. Major new
crates: `tor-js` (WASM bindings), `webtor-rs-lite` (Snowflake transport),
`tor-wasm-compat` (async trait compat), `tor-time` (cross-platform time).
Extensive modifications to core Arti crates for WASM compatibility. Traffic
routes through Snowflake pluggable transports (WebRTC or WebSocket), uses
`rustls` + `rustls-rustcrypto` (pure-Rust TLS compiled to WASM) for relay
connections, and exposes a `fetch()`-like JS API via `wasm-bindgen`.

> **Note on Tor TLS:** Tor relays use self-signed certificates —
> authentication happens via Tor's own CERTS cells (Ed25519/RSA identity
> keys), not WebPKI. The relay TLS layer uses a custom `ServerCertVerifier`
> (`TorCertVerifier`) that skips certificate validation but still verifies
> TLS handshake signatures — this is the same pattern used by the native
> rustls provider. HTTPS to external services (tor-js fetch) uses standard
> WebPKI validation via `webpki-roots` (Mozilla CA bundle embedded at
> compile time).

---

## High

### H1. No locking for concurrent browser tabs

`crates/tor-js/src/storage.rs:336-368`

`upgrade_to_readwrite()` always grants the lock locally without coordinating
across browser tabs. Multiple tabs sharing IndexedDB can corrupt each other's
directory cache and guard state. The Web Locks API could solve this.

### H2. No bootstrap timeout

`crates/tor-js/src/lib.rs:381-384`

`tor_client.bootstrap().await` can hang indefinitely if the Snowflake bridge is
unresponsive. In a browser, this blocks the single-threaded event loop. Should
wrap with a timeout.

### H3. Header injection in tor-js fetch

**Status:** DONE

`crates/tor-js/src/fetch.rs:65-67`

User-supplied header names/values are inserted into raw HTTP without any CR/LF
validation. A value containing `\r\n` could inject headers or enable HTTP
request smuggling.

### H4. `state_dir()` called unconditionally in `create_inner`

`crates/arti-client/src/client.rs:897`

`config.state_dir()` is not gated behind `#[cfg(not(target_arch = "wasm32"))]`.
On WASM, filesystem path resolution may fail, preventing client creation even
when custom storage is provided.

### H5. Incremental download path has zero test coverage

`crates/tor-dirmgr/src/bootstrap.rs:554-603`

The production code path (streaming downloads, `#[cfg(not(test))]`) is never
exercised by the test suite, which only tests the batch path.

---

## Medium

### M1. SMUX keepalive interval 500ms (comment says 5 seconds)

`crates/webtor-rs-lite/src/smux.rs:239-240`

The constant is `500` (ms) but the comment says "send NOP every 5 seconds".
The Go Snowflake client uses 10 seconds. This is 20x too aggressive, wasting
bandwidth over Tor.

### M2. SMUX NOP echo creates ping-pong risk

`crates/webtor-rs-lite/src/smux.rs:469-477`

Received NOPs are echoed back. The Go Snowflake implementation does NOT echo
NOPs. If the server also echoes, this creates unbounded network traffic.

### M3. JS storage lock never released on `close()`

`crates/tor-js/src/lib.rs:287-293` and `storage.rs:360-368`

`TorClient::close()` drops the inner client but never calls `unlock()` on JS
storage. The lock acquired in `CachedJsStorage::new()` persists until page
unload.

### M4. `User-Agent: tor-js/0.1.0` enables fingerprinting

`crates/tor-js/src/fetch.rs:55`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity.

### M7. `init()` panics on double-call

**Status:** DONE

`crates/tor-js/src/lib.rs:73-87`

`console_error_panic_hook::set_once()` is safe, but
`tracing_subscriber::registry().init()` is not idempotent. Calling `init()`
twice may panic or silently fail.

### M6. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs:71-75`

`serde_wasm_bindgen::to_value` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M7. `unsafe impl Send/Sync` for JsStorage / CachedJsStorage

`crates/tor-js/src/storage.rs:87-88, 190-191`

Justified for single-threaded WASM but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M8. Debug microdesc batch size in production code

`crates/tor-dirmgr/src/docid.rs:184-190`

`MICRODESC_N` is 20 on WASM (vs 500 native) with a `/// DEBUG:` comment. If
20 is the intended production value, remove the comment; if not, revert it.

### M9. WASM `Instant::now()` panics if `performance` API unavailable

`crates/webtor-rs-lite/src/time.rs:16`

`unwrap()` on `performance` access. In environments without the Performance API
(some Web Workers, non-browser WASM hosts), every `Instant::now()` call panics.

### M10. No timeout on native broker HTTP request

`crates/webtor-rs-lite/src/snowflake_broker.rs:293-362`

No timeout on `TcpStream::connect` or `tls_stream.read_to_end`. A
non-responsive broker hangs the client indefinitely.

### M11. Read timeout changed from 10s to 120s for all platforms

`crates/tor-dirclient/src/lib.rs:405-430`

Changed from a fixed 10-second total timeout to a 120-second idle timeout.
This affects native too, not just WASM. 120s of idle time could mask stalled
connections on native.

### M12. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs:1303-1306`

`expand_state_dir` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block). May fail unnecessarily.

### M13. InMemoryStore inconsistent readonly behavior

`crates/tor-dirmgr/src/storage/custom.rs:605-618`

`store_bridgedesc()` has no readonly check, while other storage methods
potentially should. Inconsistent write behavior.

### M14. Bridge fingerprint not validated

`crates/tor-js/src/lib.rs:300-303`

Fingerprint is required but not validated for format (40 hex chars). Invalid
fingerprints are accepted silently and only fail later when parsing the bridge
line.

---

## Low

### L1. SMUX payload truncation

`crates/webtor-rs-lite/src/smux.rs:125`

Payload length encoded as `u16` — data > 65535 bytes silently truncates. No
runtime guard.

### L2. Unbounded channels in WebSocket/WebRTC

`crates/webtor-rs-lite/src/websocket.rs:40` and `webrtc_stream.rs:92`

`mpsc::unbounded()` for incoming data with no backpressure at this layer.

### L3. Duplicated code across WASM/native file pairs

- `snowflake_ws.rs` / `snowflake_ws_native.rs` are near-duplicates
- `arti_transport.rs` / `arti_transport_native.rs` are near-duplicates

### L4. `create_snowflake_stream` ignores its parameters

`crates/webtor-rs-lite/src/snowflake.rs:326-333`

Both `broker_url` and `connection_timeout` are accepted but ignored. A separate
`create_snowflake_stream_with_config()` exists for full configuration.

### L5. HTTP method matching is case-sensitive

`crates/tor-js/src/lib.rs:521-538`

The Fetch spec normalizes methods to uppercase. `"get"` or `"post"` here
returns `INVALID_METHOD`.

### L6. `#[allow(dead_code)]` on multiple struct fields

- `webtor-rs-lite/src/kcp_stream.rs:108-114`
- `webtor-rs-lite/src/webrtc_stream.rs:57,63-68`

### L7. `Blocking` trait panics on WASM

`crates/tor-rtcompat/src/wasm.rs:178-201`

`spawn_blocking` and `reenter_block_on` panic. Correct for WASM but any library
code reaching these without a cfg guard causes a runtime crash.

### L8. Fire-and-forget `spawn_local` writes in CachedJsStorage

`crates/tor-js/src/storage.rs:265-282`

If JS storage persistence fails (e.g., quota exceeded), the in-memory cache
diverges from persistent storage. Errors are only logged.

### L9. Bridge fingerprint logged at INFO level

`crates/webtor-rs-lite/src/snowflake.rs:125`, `snowflake_ws.rs:102`,
`tor-js/src/lib.rs:321`

In contexts where the bridge is private or unlisted, this leaks which bridge the
user is connecting to.

---

## Testing Gaps

1. **Production streaming download path** (`#[cfg(not(test))]`) never tested
2. **No unit tests for webtor-rs-lite** — entire crate has zero `#[test]`
   functions

---

## Positive Observations

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Clean storage abstraction (`KeyValueStore` → `split_storage`)
- `wasm_compat::Send` pattern for removing `Send` bounds on WASM is elegant
- Good error types in tor-js with retryability info
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- Extensive fuzz testing in webtor-rs-lite
- Clean `tor-time` crate consolidating cross-platform time handling
- `portable_test` / `portable_test_async` macros enable cross-platform test authoring
- TLS handled by well-audited `rustls` library rather than custom implementation

---

## Already Fixed

These items were identified in earlier reviews and have been resolved:

1. **`export_keying_material` returned zeros** — Snowflake streams now
   delegate to rustls, which properly implements RFC 5705 key export
2. **CA bundle fetch had no size limit / status check** — Eliminated entirely;
   `webpki-roots` embeds Mozilla CA bundle at compile time
3. **Duration serde round-trip bug** — Now uses `{:09}` zero-padding
4. **Misleading comment in wallclock()** — Corrected
5. **Fragile error classification via string matching** — tor-js now uses
   structured `ErrorKind` matching
6. **Possible string slicing panic** — Replaced with `trim_matches('"')`
7. **Mutex poisoning panics in KCP** — Now uses
   `.lock().unwrap_or_else(|e| e.into_inner())`
8. **Duplicate trait definitions in tor-persist** — Consolidated into single
   `StringStore` trait via `AnyStateMgr`
9. **RwLock deadlock in BoxedDirStore** — Locks are now explicitly dropped
   before re-acquisition
10. **KCP poll_write double-sends** — Output buffering reworked to avoid
    re-queuing on `Pending`
11. **All subtle-tls issues** — Resolved
    by replacing the custom TLS implementation with `rustls` +
    `rustls-rustcrypto`.
