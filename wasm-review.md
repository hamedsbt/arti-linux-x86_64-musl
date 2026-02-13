# Code Review: WASM Arti Client

**Branch:** `wasm-arti-client`
**Stats:** 287 files changed, ~25.8k insertions, ~660 deletions (vs `zydou/main`)

## Overview

This branch adds WebAssembly/browser support to the Arti Tor client. Major new
crates: `tor-js` (WASM bindings), `subtle-tls` (TLS 1.3 for WASM),
`webtor-rs-lite` (Snowflake transport), `tor-wasm-compat` (async trait compat),
`tor-time` (cross-platform time). Extensive modifications to core Arti crates
for WASM compatibility. Traffic routes through Snowflake pluggable transports
(WebRTC or WebSocket), uses a custom pure-Rust TLS 1.3 implementation for relay
connections, and exposes a `fetch()`-like JS API via `wasm-bindgen`.

> **Note on Tor TLS:** Tor relays use self-signed certificates —
> authentication happens via Tor's own CERTS cells (Ed25519/RSA identity
> keys), not WebPKI. `skip_verification: true` is intentional for relay
> connections. The `skip_verification: false` path is used for HTTPS to
> external services (Snowflake broker, tor-js fetch) and *does* need proper
> WebPKI validation.

---

## Critical

### C1. TLS inner-plaintext padding stripping is wrong (RFC 8446 §5.4)

`crates/subtle-tls/src/record.rs:182-191` (and duplicated at ~line 342 and `stream.rs:572`)

The code takes the absolute last byte as the content type. Per RFC 8446, the
content type is the last **non-zero** byte — implementations may append zero
padding for traffic analysis resistance. Must scan backward from the end until
finding a non-zero byte.

### C2. No X.509 BasicConstraints / KeyUsage validation

`crates/subtle-tls/src/cert.rs:71-99`

`verify_chain` checks server name, validity period, and chain signatures, but
does **not** check:
- BasicConstraints (CA:TRUE required for intermediates)
- Key Usage / Extended Key Usage (serverAuth EKU on leaf)
- Path length constraints

A rogue leaf certificate could act as a CA and issue sub-certificates.

### C3. No X25519 all-zero shared secret check

`crates/subtle-tls/src/crypto.rs:200-221`

RFC 7748 §6.1 requires checking that the DH result is not all zeros.
`x25519-dalek::diffie_hellman()` does not perform this check. A malicious
server could send a low-order point, producing a predictable all-zero shared
secret.

### C4. Server cipher suite selection not validated against ClientHello offer

`crates/subtle-tls/src/handshake.rs:350-353`

The server's cipher suite is accepted unconditionally. RFC 8446 §4.1.3 requires
aborting if the server selects a cipher suite not offered. A malicious server
could select AES-256-GCM-SHA384 (0x1302), which requires SHA-384 for the key
schedule, but the code unconditionally uses SHA-256, producing wrong keys.

### C5. RSA-PSS hash algorithm hardcoded to SHA-256

`crates/subtle-tls/src/cert.rs:406-418` (and ~line 534-546)

When verifying RSA-PSS signatures, the hash algorithm is always SHA-256
regardless of the AlgorithmIdentifier parameters. Certificates signed with
RSA-PSS + SHA-384 (increasingly common with 3072+ bit keys) will have incorrect
signature verification.

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

### H3. `export_keying_material` returns zeros instead of error

`crates/webtor-rs-lite/src/snowflake.rs:265-268` and `snowflake_ws.rs:175-178`

Returns `Ok(vec![0u8; len])` for keying material export. If any Tor protocol
component uses this for key derivation, all keys would be zero. Should return
`Err` so callers know the operation is unsupported.

### H4. Header injection in tor-js fetch

`crates/tor-js/src/fetch.rs:65-67`

User-supplied header names/values are inserted into raw HTTP without any CR/LF
validation. A value containing `\r\n` could inject headers or enable HTTP
request smuggling.

### H5. `unsafe impl Send` on TlsStream without cfg guard

`crates/subtle-tls/src/stream.rs:66-69`

`unsafe impl<S: Send> Send for TlsStream<S>` is not gated behind
`#[cfg(target_arch = "wasm32")]`. The struct contains `Rc<ReadySignal>`, `Cell`,
and `RefCell` — none of which are `Send`. On native multi-threaded runtimes this
would be unsound.

### H6. EC curve defaults to P-256 on parse failure

`crates/subtle-tls/src/cert.rs:482-485`

If the EC curve cannot be determined from key parameters, the code defaults to
P-256 and logs a warning. A P-384 certificate whose parameters fail to parse
would be verified with the wrong curve. Should return an error.

### H7. CA bundle fetch has no size limit and no status check

`crates/tor-js/src/lib.rs:468-480`

`read_to_end` with no size limit. A malicious exit node could serve an
arbitrarily large response. The HTTP status code is also not checked — a 302
redirect or 500 error page could be treated as valid PEM.

### H8. `state_dir()` called unconditionally in `create_inner`

`crates/arti-client/src/client.rs:897`

`config.state_dir()` is not gated behind `#[cfg(not(target_arch = "wasm32"))]`.
On WASM, filesystem path resolution may fail, preventing client creation even
when custom storage is provided.

### H9. `skip_verification` exposed as public struct field

`crates/subtle-tls/src/lib.rs:64-72`

`TlsConfig::skip_verification` is a public `bool` with no feature-gate.
Production TLS libraries typically gate this behind a feature flag like
`danger_accept_invalid_certs`.

### H10. Incremental download path has zero test coverage

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

### M3. Trust store matching by Subject DN string comparison

`crates/subtle-tls/src/trust_store.rs:255-276`

Root CA matching uses `to_string()` comparison of X.500 names, which has
encoding ambiguities (UTF-8 vs PrintableString, whitespace normalization).
Comparing raw DER-encoded issuer/subject bytes would be more robust.

### M4. No secret zeroization

`crates/subtle-tls/src/handshake.rs:68-97`

All TLS secrets (`handshake_secret`, `client_app_secret`,
`server_app_secret`, `exporter_master_secret`) are stored as `Vec<u8>` and not
zeroed on drop. Consider `zeroize::Zeroizing<Vec<u8>>`.

### M5. JS storage lock never released on `close()`

`crates/tor-js/src/lib.rs:287-293` and `storage.rs:360-368`

`TorClient::close()` drops the inner client but never calls `unlock()` on JS
storage. The lock acquired in `CachedJsStorage::new()` persists until page
unload.

### M6. `User-Agent: tor-js/0.1.0` enables fingerprinting

`crates/tor-js/src/fetch.rs:55`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity.

### M7. `init()` panics on double-call

`crates/tor-js/src/lib.rs:73-87`

`console_error_panic_hook::set_once()` is safe, but
`tracing_subscriber::registry().init()` is not idempotent. Calling `init()`
twice may panic or silently fail.

### M8. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs:71-75`

`serde_wasm_bindgen::to_value` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M9. `unsafe impl Send/Sync` for JsStorage / CachedJsStorage

`crates/tor-js/src/storage.rs:87-88, 190-191`

Justified for single-threaded WASM but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M10. Debug microdesc batch size in production code

`crates/tor-dirmgr/src/docid.rs:184-190`

`MICRODESC_N` is 20 on WASM (vs 500 native) with a `/// DEBUG:` comment. If
20 is the intended production value, remove the comment; if not, revert it.

### M11. WASM `Instant::now()` panics if `performance` API unavailable

`crates/webtor-rs-lite/src/time.rs:16`

`unwrap()` on `performance` access. In environments without the Performance API
(some Web Workers, non-browser WASM hosts), every `Instant::now()` call panics.

### M12. No timeout on native broker HTTP request

`crates/webtor-rs-lite/src/snowflake_broker.rs:293-362`

No timeout on `TcpStream::connect` or `tls_stream.read_to_end`. A
non-responsive broker hangs the client indefinitely.

### M13. Verbose info-level logging in crypto module

`crates/subtle-tls/src/crypto.rs` (e.g. lines 460-508)

Seven `info!()` calls inside `AesGcm::decrypt`. These are clearly debug
artifacts and should be demoted to `trace` or `debug`.

### M14. Missing HelloRetryRequest handling

`crates/subtle-tls/src/stream.rs:207-254`

The code does not detect HelloRetryRequest (ServerHello with special
`server_random` value per RFC 8446 §4.1.4). Connections to servers requiring a
different key share group fail with an opaque error.

### M15. ALPN configuration ignored

`crates/subtle-tls/src/lib.rs:69` and `handshake.rs:232`

`TlsConfig::alpn_protocols` is never read. `build_alpn_extension` hardcodes
`["http/1.1"]` regardless of the config.

### M16. Read timeout changed from 10s to 120s for all platforms

`crates/tor-dirclient/src/lib.rs:405-430`

Changed from a fixed 10-second total timeout to a 120-second idle timeout.
This affects native too, not just WASM. 120s of idle time could mask stalled
connections on native.

### M17. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs:1303-1306`

`expand_state_dir` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block). May fail unnecessarily.

### M18. Unsanitized SNI hostname

`crates/subtle-tls/src/handshake.rs:248-268`

No validation that the server name contains valid DNS characters before sending
in the SNI extension. Low real-world risk since hostnames come from internal Tor
config, not user input.

### M19. InMemoryStore inconsistent readonly behavior

`crates/tor-dirmgr/src/storage/custom.rs:605-618`

`store_bridgedesc()` has no readonly check, while other storage methods
potentially should. Inconsistent write behavior.

### M20. Bridge fingerprint not validated

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
- `hmac_sha256` duplicated in `handshake.rs` and `crypto.rs`

### L4. `create_snowflake_stream` ignores its parameters

`crates/webtor-rs-lite/src/snowflake.rs:326-333`

Both `broker_url` and `connection_timeout` are accepted but ignored. A separate
`create_snowflake_stream_with_config()` exists for full configuration.

### L5. HTTP method matching is case-sensitive

`crates/tor-js/src/lib.rs:521-538`

The Fetch spec normalizes methods to uppercase. `"get"` or `"post"` here
returns `INVALID_METHOD`.

### L6. `TlsVersion` enum has one variant, `version` field unused

`crates/subtle-tls/src/lib.rs:56-61`

Dead abstraction.

### L7. Dead random bytes in `X25519KeyPair::generate()`

`crates/subtle-tls/src/crypto.rs:185-188`

32 bytes allocated and generated via `getrandom` then immediately discarded.
The actual key generation uses `OsRng` separately.

### L8. `#[allow(dead_code)]` on multiple struct fields

- `subtle-tls/src/crypto.rs:262-263, 358-359` (`key_size`)
- `webtor-rs-lite/src/kcp_stream.rs:108-114`
- `webtor-rs-lite/src/webrtc_stream.rs:57,63-68`

### L9. `Blocking` trait panics on WASM

`crates/tor-rtcompat/src/wasm.rs:178-201`

`spawn_blocking` and `reenter_block_on` panic. Correct for WASM but any library
code reaching these without a cfg guard causes a runtime crash.

### L10. Fire-and-forget `spawn_local` writes in CachedJsStorage

`crates/tor-js/src/storage.rs:265-282`

If JS storage persistence fails (e.g., quota exceeded), the in-memory cache
diverges from persistent storage. Errors are only logged.

### L11. Bridge fingerprint logged at INFO level

`crates/webtor-rs-lite/src/snowflake.rs:125`, `snowflake_ws.rs:102`,
`tor-js/src/lib.rs:321`

In contexts where the bridge is private or unlisted, this leaks which bridge the
user is connecting to.

---

## Testing Gaps

1. **No end-to-end TLS handshake test** in subtle-tls (no loopback or
   known-good test vector validation)
2. **No certificate chain validation test** — only `test_skip_verification`
3. **No test for `poll_read`/`poll_write`** `AsyncRead`/`AsyncWrite` impls on
   `TlsStream`
4. **No test for hostname verification edge cases** (IP SAN, null byte
   injection, IDN)
5. **No test for record fragmentation/reassembly**
6. **Production streaming download path** (`#[cfg(not(test))]`) never tested
7. **No unit tests for webtor-rs-lite** — entire crate has zero `#[test]`
   functions

---

## Positive Observations

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Clean storage abstraction (`KeyValueStore` → `split_storage`)
- `wasm_compat::Send` pattern for removing `Send` bounds on WASM is elegant
- Good error types in tor-js with retryability info
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- Extensive fuzz testing (subtle-tls, webtor-rs-lite)
- Kani verification proofs for ECDSA DER conversion
- Clean `tor-time` crate consolidating cross-platform time handling
- `portable_test` / `portable_test_async` macros enable cross-platform test authoring

---

## Already Fixed

These items were identified in earlier reviews and have been resolved:

1. **Certificate chain validation not enforced** — Untrusted chains now return
   `Err(TlsError::certificate(...))`
2. **Incomplete intermediate CA verification** — `find_issuing_trusted_root`
   now returns root DER bytes; caller cryptographically verifies signature
3. **Timing side-channel in TLS Finished verification** — Now uses
   `subtle::ConstantTimeEq::ct_eq()`
4. **Panic on oversized Turbo frame** — `encode()` now returns `Result<Vec<u8>>`
5. **Nonce reuse risk in record layer** — `increment_sequence()` now returns
   `Result<()>` and rejects at `u64::MAX`
6. **skip_verification disables CertificateVerify** — CertificateVerify is now
   always verified when server sends a Certificate
7. **Duration serde round-trip bug** — Now uses `{:09}` zero-padding
8. **Misleading comment in wallclock()** — Corrected
9. **Fragile error classification via string matching** — tor-js now uses
   structured `ErrorKind` matching
10. **Possible string slicing panic** — Replaced with `trim_matches('"')`
11. **Mutex poisoning panics in KCP** — Now uses
    `.lock().unwrap_or_else(|e| e.into_inner())`
12. **Duplicate trait definitions in tor-persist** — Consolidated into single
    `StringStore` trait via `AnyStateMgr`
13. **RwLock deadlock in BoxedDirStore** — Locks are now explicitly dropped
    before re-acquisition
14. **KCP poll_write double-sends** — Output buffering reworked to avoid
    re-queuing on `Pending`
