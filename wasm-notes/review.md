# Code Review: WASM Arti Client

**Branch:** `wasm-arti-client`
**Base:** `zydou/main`

## Stats (excluding Cargo.lock)

| Category | Files | Insertions | Deletions |
|----------|------:|----------:|----------:|
| **Overall** | ~250 | ~13,500 | ~700 |
| tor-time + tor-async-compat related | ~189 | ~2,761 | ~645 |
| Core WASM work (excl. time/async-compat) | ~60 | ~10,700 | ~55 |

> Stats are approximate. "tor-time/tor-async-compat related" includes the crate
> sources **plus** all files across the codebase that reference
> `tor_time`/`tor_async_compat` (i.e. migration of `std::time`/`async_trait`
> imports).

**Core WASM work breakdown** (~10.7k insertions):

| Lines | Directory | Description |
|------:|-----------|-------------|
| ~6,100 | `crates/tor-js` | WASM bindings, TS wrapper (ArtiSocketProvider, storage, fetch) |
| ~1,100 | `examples/tor-js` | Browser showcase + Node.js examples |
| ~900 | `crates/tor-rtcompat` | WASM runtime (WasmRuntime, WebSocket/WebRTC relay connections) |
| ~700 | `crates/tor-dirmgr` | Custom storage backend, incremental downloads, bootstrap yielding |
| ~630 | `crates/arti-client` | KeyValueStore trait, builder, WASM cfg guards |
| ~360 | `crates/tor-persist` | StringStore trait, AnyStateMgr |
| ~200 | `scripts/` | Build/check scripts |
| ~700 | other | Review docs, small crate tweaks, zstd-wasm |

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

---

## High

### H1. No locking for concurrent browser tabs

`crates/tor-js/src/storage.rs:400-428`

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

`crates/arti-client/src/client.rs:897`

`config.state_dir()?` is called without a `#[cfg(not(target_arch = "wasm32"))]`
guard. On WASM, filesystem path resolution may fail, preventing client creation
even when custom storage is provided. The result is only used by the
`onion-service-service` feature gate (line 898), but the call itself is
unconditional.

### H4. Incremental download path has zero test coverage

`crates/tor-dirmgr/src/bootstrap.rs:570-604`

The production code path (streaming downloads, `#[cfg(not(test))]`) is never
exercised by the test suite, which only tests the batch path.

### H5. Fast bootstrap skips signature and timeliness verification

`crates/tor-js/src/fast_bootstrap.rs:246-250`

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

`crates/tor-js/src/fetch.rs:307-308`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity. Also, the header check is case-sensitive
(`contains_key("User-Agent")` and `contains_key("user-agent")` checked
separately) -- other casings like `"user-agent"` with mixed case would bypass.

### M3. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs:76-80`

`serde_wasm_bindgen::to_value()` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M4. `unsafe impl Send/Sync` across WASM types

`crates/tor-js/src/storage.rs:93-94, 215-216`

Multiple types use `unsafe impl Send` (and/or `Sync`) with the rationale "WASM
is single-threaded." Justified today but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M5. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs:1306-1309`

`expand_state_dir()` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block at line 1314). May fail
unnecessarily.

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

### M10. Chunk size integer overflow in fetch

`crates/tor-js/src/fetch.rs:228-240`

`usize::from_str_radix()` parses hex chunk sizes without bounds. Extremely
large chunk sizes could cause allocation failures. Should bound to a reasonable
maximum.

### M11. `rustls-rustcrypto` is alpha

`crates/tor-rtcompat/Cargo.toml`

Uses `rustls-rustcrypto = "0.0.2-alpha"` for pure-Rust crypto on WASM. Should
monitor for stability and potential security issues in the alpha release.

### M12. Read timeout changed from total to per-read idle

`crates/tor-dirclient/src/lib.rs`

The original 10-second timeout was a total deadline for the entire read. It's
now a per-read idle timeout (timer resets on each chunk). Same duration (10s)
but different semantics -- a very slow stream that sends 1 byte every 9 seconds
would now succeed where it previously timed out. Comment still references
Snowflake (removed).

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

### L4. `init()` reload handle can desync from active subscriber

`crates/tor-js/src/lib.rs`

`init()` creates a reload handle and stores it globally even when `try_init()`
means the subscriber was not installed (because one already exists). After
repeated `init()` calls, `setLogLevel()` can end up talking to a handle that
is not attached to the active subscriber.

### L5. README out of sync with code

`crates/tor-js/README.md`

Documents a `JsHttpResponse` API and shows `response.text()` used
synchronously, while the implementation now builds a real `web_sys::Response`.

### L6. wasm.ts double-init failure is permanent

`crates/tor-js/ts-wrapper/src/wasm.ts`

If `doInit()` rejects, `initPromise` is set to the rejected promise forever.
Subsequent calls get the same rejection with no retry path.

---

## Testing

- **Production streaming download path** (`#[cfg(not(test))]` in
  `tor-dirmgr/src/bootstrap.rs`) is never exercised by tests.
- **Custom storage**: Module-level tests for `KvStateAdapter`, `KvDirAdapter`,
  shared lock state. Missing: concurrent access patterns, error cases (write
  failures, lock failures).
- **No WASM integration tests** visible in the diff (tests run on native via
  `wasm-bindgen-test` but no browser-level end-to-end test).

---

## Positive Observations

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Clean storage abstraction (`KeyValueStore` -> `split_storage` -> adapters)
- `tor_async_compat::async_trait` elegantly removes `Send` bounds on WASM
- `tor-time` creates solid foundation for cross-platform time handling
- Good error types in tor-js with retryability info and structured error codes
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- `portable_test` / `portable_test_async` macros enable cross-platform test authoring
- TLS handled by well-audited `rustls` rather than custom implementation
- Batch persistence optimization (`set_many()`) acquires lock once
- Fast-failure path: connection errors surfaced to `ready()` for quick feedback
- Hardware-accelerated SHA-256 via `crypto.subtle.digest()` in fast bootstrap
- Incremental bootstrap yields control via `sleep(Duration::ZERO)` to prevent
  UI blocking on WASM
- CR/LF validation in fetch headers prevents HTTP header injection
- `wait_for_unlock()` future enables async lock coordination
- `ArtiSocketProvider` cleanly abstracts over direct TCP / WebRTC / WebSocket
  with auto-detection and fallback
- `ruzstd` (pure-Rust zstd) enables x-zstd directory compression on WASM

---

## Removed (no longer applicable)

The following items from the original review applied to `crates/tor-snowflake`,
which has been removed from the branch. They are retained here for reference
in case Snowflake support is re-added.

- **M1 (old).** SMUX keepalive interval 500ms (comment says 5 seconds)
- **M2 (old).** SMUX NOP echo creates ping-pong risk
- **M6 (old).** `unsafe impl Send/Sync` on SnowflakeStream
- **M7 (old).** Debug microdesc batch size (`MICRODESC_N = 20`)
- **M8 (old).** WASM `Instant::now()` panics if `performance` API unavailable
- **M9 (old).** No timeout on native broker HTTP request
- **M11 (old).** Bridge fingerprint not validated
- **M12 (old).** SMUX window update loss risk
- **M18 (old).** `connection_timeout` config field not enforced
- **M19 (old).** WebSocket vs WebRTC naming/docs contradictory
- **M20 (old).** Retry logic based on substring matching
- **M22 (old).** `rustls-rustcrypto` is alpha — *moved to M11 (still applies)*
- **L1 (old).** SMUX payload truncation
- **L2 (old).** Unbounded channels in WebSocket/WebRTC
- **L3 (old).** Duplicated code across WASM/native file pairs
- **L5 (old).** `#[allow(dead_code)]` on multiple struct fields
- **L8 (old).** Bridge fingerprint logged at INFO level
- **L9 (old).** KCP congestion control explicitly disabled
- **L10 (old).** WASM cancellation token uses polling
- **L12 (old).** Guard deletion workaround on every client creation
- **L13 (old).** Partial writes treated as fatal in SMUX
- **L14 (old).** `Vec::drain` O(n) in hot paths
- **L17 (old).** Unused DataChannel error closure

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
8. **Duplicate trait definitions in tor-persist** -- Consolidated into single
   `StringStore` trait via `AnyStateMgr`
9. **RwLock deadlock in BoxedDirStore** -- Locks are now explicitly dropped
   before re-acquisition
10. **All subtle-tls issues** -- Resolved by replacing the custom TLS
    implementation with `rustls` + `rustls-rustcrypto`
11. **`MICRODESC_N` debug batch size** -- Reverted to standard batch size (500)
12. **Read timeout 10s→120s for all platforms** -- Reverted to 10s (now per-read
    idle rather than total; see M12)
13. **Commented-out debug tracing in tor-proto** -- Removed
