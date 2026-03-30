# Code Review: WASM Arti Client

**Branch:** `main` (includes merged `wasm-arti-client`)
**Base:** `zydou/main` (upstream)
**Last audited:** 2026-03-27

## Overview

This branch adds WebAssembly/browser support to the Arti Tor client. Major new
crates: `tor-js` (WASM bindings + TS wrapper), `tor-time` (cross-platform
time), `tor-async-compat` (conditional Send bounds). Extensive modifications to
core Arti crates for WASM compatibility. Traffic routes through a gateway server
(WebRTC or WebSocket relay proxying) in browsers, or via direct TCP in Node.js/
Deno. Uses `rustls` + `rustls-rustcrypto` (pure-Rust TLS compiled to WASM) for
relay connections, and exposes a `fetch()`-like JS API via `wasm-bindgen`.

> **Note on Tor TLS:** Tor relays use self-signed certificates --
> authentication happens via Tor's own CERTS cells (Ed25519/RSA identity
> keys), not WebPKI. The relay TLS layer uses a custom `ServerCertVerifier`
> (`TorCertVerifier`) that skips certificate validation but still verifies
> TLS handshake signatures -- this is the same pattern used by the native
> rustls provider. HTTPS to external services (tor-js fetch) uses standard
> WebPKI validation via `webpki-roots` (Mozilla CA bundle embedded at
> compile time).

For per-crate change details, see `changes-explained.md`.

---

## High

### H1. No locking for concurrent browser tabs

`crates/tor-js/src/storage.rs`

`try_lock()` / `unlock()` only track local boolean state -- no cross-tab
coordination. The JS-side lock is acquired once in `CachedJsStorage::new()` and
held for the client's lifetime. Multiple tabs sharing IndexedDB can corrupt each
other's directory cache and guard state. The Web Locks API could solve this
(the TS wrapper's `locking.ts` uses it when available, but the Rust layer
doesn't participate in the protocol).

### H2. No bootstrap timeout

`crates/tor-js/src/lib.rs`

`ready()` polls bootstrap events with a 500ms periodic timeout to check for
errors, but there is no hard overall timeout. If the gateway is unresponsive but
the connection stays open, `ready()` loops indefinitely.

### H3. `state_dir()` called unconditionally in `create_inner`

`crates/arti-client/src/client.rs`

`config.state_dir()?` is called without a `#[cfg(not(target_arch = "wasm32"))]`
guard. On WASM, filesystem path resolution may fail, preventing client creation
even when custom storage is provided. The result is only used by the
`onion-service-service` feature gate, but the call itself is unconditional.

### H4. Fast bootstrap skips signature and timeliness verification

`crates/tor-js/src/fast_bootstrap.rs`

Authority certs extracted during fast bootstrap call
`dangerously_assume_wellsigned()` and `dangerously_assume_timely()`. This is
documented with a comment: "Skip signature and time checks for metadata
extraction only. Arti re-verifies everything when loading from storage (see
state.rs add_from_cache impls)." The risk is that if arti's re-verification
invariant ever changes, malicious bootstrap data could inject bad certs
silently. No freshness check on the bootstrap ZIP either (stale data with
expired `valid_until` is accepted).

---

## Medium

### M1. JS storage lock never released on `close()`

`crates/tor-js/src/lib.rs` and `storage.rs`

`close()` sets `self.inner = None` but doesn't explicitly release the lock.
The Drop impl on `CachedJsStorage` spawns an async `unlock()` via
`spawn_local()` -- so lock release is fire-and-forget and may not complete if
the event loop doesn't run after drop.

### M2. `User-Agent: tor-js/0.1.0` enables fingerprinting

`crates/tor-js/src/fetch.rs`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity. Also, the header check is case-sensitive
(`contains_key("User-Agent")` and `contains_key("user-agent")` checked
separately) -- other casings like `"User-agent"` with mixed case would bypass.

### M3. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs`

`serde_wasm_bindgen::to_value()` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M4. `unsafe impl Send/Sync` across WASM types

`crates/tor-js/src/storage.rs`

Multiple types use `unsafe impl Send` (and/or `Sync`) with the rationale "WASM
is single-threaded." Justified today but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M5. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs`

`expand_state_dir()` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block). May fail unnecessarily.

### M6. Abort signal not raced into connect/TLS/header-read awaits

`crates/tor-js/src/fetch.rs`

`AbortSignal` is checked before connect, before sending the request, and between
response-body chunks, but is not raced into the connect / TLS handshake /
header-read awaits themselves. An abort remains "stuck" until the next explicit
checkpoint rather than cancelling promptly.

### M7. Duplicate and collapsed HTTP headers in fetch

`crates/tor-js/src/fetch.rs`

The request builder always writes `Host`, then caller-supplied headers verbatim,
then appends `Content-Length` -- so duplicate `Host` / `Content-Length` headers
are possible if the caller also sets them. On the response side, headers are
collected into a `HashMap<String, String>`, which collapses repeated headers
(e.g. `Set-Cookie`) instead of preserving them as the HTTP spec requires.

### M8. 1xx interim responses not handled

`crates/tor-js/src/fetch.rs`

The parser classifies `1xx`, `204`, `304` as bodyless, but does not continue
past an interim `1xx` to read the final response. A server sending `100
Continue` before the real response would cause the fetch to return early with
a 1xx status.

### M9. Chunked decoder silently accepts truncated bodies

`crates/tor-js/src/fetch.rs`

If EOF occurs during chunked body reading and some data was already read, the
decoder returns `Ok(None)` rather than an error. This silently accepts truncated
responses instead of surfacing the incomplete transfer.

### M10. `rustls-rustcrypto` is alpha

`crates/tor-rtcompat/Cargo.toml`

Uses `rustls-rustcrypto = "0.0.2-alpha"` for pure-Rust crypto on WASM. Should
monitor for stability and potential security issues in the alpha release.

---

## Low

### L1. HTTP method matching is case-sensitive

`crates/tor-js/src/lib.rs`

The Fetch spec normalizes methods to uppercase. `"get"` or `"post"` here
returns `INVALID_METHOD`.

### L2. `Blocking` trait panics on WASM

`crates/tor-rtcompat/src/wasm.rs`

`spawn_blocking` and `reenter_block_on` panic. Correct for WASM but any library
code reaching these without a cfg guard causes a runtime crash.

### L3. Fire-and-forget `spawn_local` writes in CachedJsStorage

`crates/tor-js/src/storage.rs`

If JS storage persistence fails (e.g., quota exceeded), the in-memory cache
diverges from persistent storage. Errors are only logged.

### L4. README out of sync with code

`crates/tor-js/README.md`

Documents a `JsHttpResponse` API and shows `response.text()` used
synchronously, while the implementation now builds a real `web_sys::Response`.
Still references `tor-snowflake` and `subtle-tls` which no longer exist.

---

## Testing

- **Custom storage**: Unit tests for `AnyStateMgr` and `BoxedDirStore` with
  in-memory `KeyValueStore`. Missing: concurrent access patterns, error cases
  (write failures, lock failures).
- **No WASM integration tests** visible in the diff (tests run on native via
  `wasm-bindgen-test` but no browser-level end-to-end test).
- **Smoke test**: `tor-fetch.js --websocket` successfully connects through Tor
  and returns `{"IsTor":true}`.

---

## Positive Observations

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Clean storage abstraction: single `KeyValueStore` trait, no intermediate layers
- `tor_async_compat::async_trait` elegantly removes `Send` bounds on WASM
- `tor-time` creates solid foundation for cross-platform time handling
- Good error types in tor-js with retryability info and structured error codes
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- TLS handled by well-audited `rustls` rather than custom implementation
- Batch persistence optimization (`set_many()`) acquires lock once
- Fast-failure path: connection errors surfaced to `ready()` for quick feedback
- Hardware-accelerated SHA-256 via `crypto.subtle.digest()` in fast bootstrap
- CR/LF validation in fetch headers prevents HTTP header injection
- `wait_for_unlock()` future enables async lock coordination
- `ArtiSocketProvider` cleanly abstracts over direct TCP / WebRTC / WebSocket
  with auto-detection and fallback
- `ruzstd` (pure-Rust zstd) enables x-zstd directory compression on WASM

---

## Already Fixed

These items were identified in earlier reviews and have been resolved:

1. **Header injection in tor-js fetch** -- CR/LF validation added
2. **`init()` panics on double-call** -- now uses `try_init()` which is
   idempotent
3. **CA bundle fetch had no size limit / status check** -- Eliminated entirely;
   `webpki-roots` embeds Mozilla CA bundle at compile time
4. **Duration serde round-trip bug** -- Now uses `{:09}` zero-padding
5. **Misleading comment in wallclock()** -- Corrected
6. **Fragile error classification via string matching** -- tor-js now uses
   structured `ErrorKind` matching
7. **Possible string slicing panic** -- Replaced with `trim_matches('"')`
8. **Duplicate trait definitions in tor-persist** -- Consolidated: single
   `KeyValueStore` trait used directly by `AnyStateMgr` and `BoxedDirStore`
9. **RwLock deadlock in BoxedDirStore** -- `BoxedDirStore` now wraps
   `Arc<dyn KeyValueStore>` directly, no internal RwLock
10. **All subtle-tls issues** -- Resolved by replacing the custom TLS
    implementation with `rustls` + `rustls-rustcrypto`
11. **`MICRODESC_N` debug batch size** -- Reverted to standard batch size (500)
12. **Read timeout idle→total** -- Reverted to upstream's total 10s timeout
13. **Commented-out debug tracing in tor-proto** -- Removed
14. **`init()` reload handle desync** -- `init()` now uses idempotent
    `try_init()`, reload handle properly managed
15. **Streaming bootstrap untested** -- Reverted to upstream's batch approach
    (see `potential-improvements.md`)
16. **wasm.ts double-init** -- Idempotent pattern; rejected promise cached
    permanently, which is correct behavior (no silent retry of broken init)
