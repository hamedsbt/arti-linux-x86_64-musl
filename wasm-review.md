# Code Review: WASM Arti Client

**Branch:** `wasm-arti-client`
**Base:** `zydou/main`

## Stats (excluding Cargo.lock)

| Category | Files | Insertions | Deletions |
|----------|------:|----------:|----------:|
| **Overall** | 308 | 22,052 | 700 |
| tor-time + tor-async-compat related | 189 | 2,761 | 645 |
| Core WASM work (excl. time/async-compat) | 119 | ~19,291 | ~55 |

> "tor-time/tor-async-compat related" includes the crate sources **plus** all
> files across the codebase that reference `tor_time`/`tor_async_compat` (i.e.
> migration of `std::time`/`async_trait` imports). The partition is approximate
> since some files contain both time migration and WASM changes.

**Core WASM work breakdown** (~19.3k insertions):

| Lines | Directory | Description |
|------:|-----------|-------------|
| 6,462 | `crates/tor-snowflake` | Snowflake transport (SMUX, KCP, WebRTC, broker) |
| 6,073 | `crates/tor-js` | WASM bindings, TS wrapper, fetch, storage |
| 2,940 | `tests/webtor` | Integration tests |
| 1,598 | `examples/tor-js-showcase` | Browser demo app |
| 703 | `crates/tor-dirmgr` | Custom storage backend, bootstrap yielding |
| 631 | `crates/arti-client` | KeyValueStore trait, builder, WASM cfg guards |
| 362 | `crates/tor-persist` | StringStore trait, AnyStateMgr |
| 216 | `scripts/` | Build/check scripts |
| 306 | other | Review docs, small crate tweaks |

## Overview

This branch adds WebAssembly/browser support to the Arti Tor client. Major new
crates: `tor-js` (WASM bindings + TS wrapper), `tor-snowflake` (Snowflake
transport), `tor-time` (cross-platform time), `tor-async-compat` (conditional
Send bounds). Extensive modifications to core Arti crates for WASM
compatibility. Traffic routes through Snowflake pluggable transports (WebRTC or
WebSocket), uses `rustls` + `rustls-rustcrypto` (pure-Rust TLS compiled to
WASM) for relay connections, and exposes a `fetch()`-like JS API via
`wasm-bindgen`.

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

`crates/tor-js/src/lib.rs:362-399`

`ready()` polls bootstrap events with a 500ms periodic timeout to check for WS
errors, but there is no hard overall timeout. If the Snowflake bridge is
unresponsive but the WebSocket stays open, `ready()` loops indefinitely.

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

### M1. SMUX keepalive interval 500ms (comment says 5 seconds)

`crates/tor-snowflake/src/smux.rs:239-240`

The constant is `500` (ms) but the comment says "send NOP every 5 seconds".
The Go Snowflake client uses 10 seconds. This is 20x too aggressive, wasting
bandwidth over Tor (~24KB/s minimum overhead).

### M2. SMUX NOP echo creates ping-pong risk

`crates/tor-snowflake/src/smux.rs:469-477`

Received NOPs are echoed back. The Go Snowflake implementation does NOT echo
NOPs. If the server also echoes, this creates unbounded network traffic.
Additionally, no timeout if NOP responses never arrive -- connection can become
asymmetrically stalled.

### M3. JS storage lock never released on `close()`

`crates/tor-js/src/lib.rs:402-410` and `storage.rs:335-347`

`close()` sets `self.inner = None` but doesn't explicitly release the lock.
The Drop impl on `CachedJsStorage` spawns an async `unlock()` via
`spawn_local()` -- so lock release is fire-and-forget and may not complete if
the event loop doesn't run after drop.

### M4. `User-Agent: tor-js/0.1.0` enables fingerprinting

`crates/tor-js/src/fetch.rs:307-308`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity. Also, the header check is case-sensitive
(`contains_key("User-Agent")` and `contains_key("user-agent")` checked
separately) -- other casings like `"user-agent"` with mixed case would bypass.

### M5. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs:76-80`

`serde_wasm_bindgen::to_value()` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M6. `unsafe impl Send/Sync` for JsStorage / CachedJsStorage

`crates/tor-js/src/storage.rs:93-94, 215-216`

Justified for single-threaded WASM but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M7. Debug microdesc batch size in production code

`crates/tor-dirmgr/src/docid.rs:184-190`

`MICRODESC_N` is 20 on WASM (vs 500 native) with a `/// DEBUG:` comment. If
20 is the intended production value, remove the comment; if not, revert it.
Should be configurable rather than hardcoded.

### M8. WASM `Instant::now()` panics if `performance` API unavailable

`crates/tor-snowflake/src/time.rs:9-18`

`get_performance_now_ms()` chains `.ok()`, `.and_then()`, `.map()` then
`.unwrap()`. In environments without the Performance API (some Web Workers,
non-browser WASM hosts), every `Instant::now()` call panics.

### M9. No timeout on native broker HTTP request

`crates/tor-snowflake/src/snowflake_broker.rs:293-338`

No timeout on `TcpStream::connect` or `read_to_end`. A non-responsive broker
hangs the client indefinitely. Also, the HTTP response parser just looks for
`\r\n\r\n` and treats everything after as body -- if the broker response
contains embedded double-newlines, parsing breaks.

### M10. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs:1306-1309`

`expand_state_dir()` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block at line 1314). May fail
unnecessarily.

### M11. Bridge fingerprint not validated

`crates/tor-js/src/lib.rs:238-263`

Fingerprint is required but not validated for format (40 hex chars). Invalid
fingerprints are accepted silently and only fail later when parsing the bridge
line.

### M12. SMUX window update loss risk

`crates/tor-snowflake/src/smux.rs:664-676`

Window updates (UPD) are queued in `pending_upd` but there's no retry if the
network layer drops them. If a UPD write fails, the peer won't know the
receiver has consumed data and may stop sending -- potential deadlock.

### M13. Chunk size integer overflow in fetch

`crates/tor-js/src/fetch.rs:228-240`

`usize::from_str_radix()` parses hex chunk sizes without bounds. Extremely
large chunk sizes could cause allocation failures. Should bound to a reasonable
maximum.

### M14. Read timeout changed from 10s to 120s for all platforms

`crates/tor-dirclient/src/lib.rs:409-412`

Changed from a fixed 10-second total timeout to a 120-second idle timeout.
This affects native too, not just WASM. 120s of idle time could mask stalled
connections on native.

### M15. `rustls-rustcrypto` is alpha

`crates/tor-rtcompat/Cargo.toml`

Uses `rustls-rustcrypto = "0.0.2-alpha"` for pure-Rust crypto on WASM. Should
monitor for stability and potential security issues in the alpha release.

---

## Low

### L1. SMUX payload truncation

`crates/tor-snowflake/src/smux.rs:125`

Payload length encoded as `u16` -- data > 65535 bytes silently truncates. No
runtime guard.

### L2. Unbounded channels in WebSocket/WebRTC

`crates/tor-snowflake/src/websocket.rs:40` and `webrtc_stream.rs:92`

`mpsc::unbounded()` for incoming data with no backpressure at this layer.

### L3. Duplicated code across WASM/native file pairs

- `snowflake_ws.rs` / `snowflake_ws_native.rs` are near-duplicates
- `arti_transport.rs` / `arti_transport_native.rs` are near-duplicates

### L4. HTTP method matching is case-sensitive

`crates/tor-js/src/lib.rs:581-597`

The Fetch spec normalizes methods to uppercase. `"get"` or `"post"` here
returns `INVALID_METHOD`.

### L5. `#[allow(dead_code)]` on multiple struct fields

- `tor-snowflake/src/kcp_stream.rs:108-114`
- `tor-snowflake/src/webrtc_stream.rs:57,63-68`

### L6. `Blocking` trait panics on WASM

`crates/tor-rtcompat/src/wasm.rs:187-202`

`spawn_blocking` and `reenter_block_on` panic. Correct for WASM but any library
code reaching these without a cfg guard causes a runtime crash.

### L7. Fire-and-forget `spawn_local` writes in CachedJsStorage

`crates/tor-js/src/storage.rs:290-330`

If JS storage persistence fails (e.g., quota exceeded), the in-memory cache
diverges from persistent storage. Errors are only logged.

### L8. Bridge fingerprint logged at INFO level

`crates/tor-snowflake/src/snowflake.rs:98`,
`crates/tor-snowflake/src/snowflake_ws.rs:80`

In contexts where the bridge is private or unlisted, this leaks which bridge the
user is connecting to.

### L9. KCP congestion control explicitly disabled

`crates/tor-snowflake/src/kcp_stream.rs:84-101`

KCP configured with `nc: true` (no congestion control) and `snd_wnd/rcv_wnd:
65535` (maximum window). Congestion control is delegated to the SMUX layer
above. The rationale matches Snowflake's MaxStreamBuffer but is undocumented.

### L10. WASM cancellation token uses polling

`crates/tor-snowflake/src/retry.rs:83-85`

WASM version polls every 50ms to check cancellation via `Arc<AtomicBool>`.
Wastes CPU cycles; could use Promise-based signaling instead.

### L11. wasm.ts double-init failure is permanent

`crates/tor-js/ts-wrapper/src/wasm.ts:106-110`

If `doInit()` rejects, `initPromise` is set to the rejected promise forever.
Subsequent calls get the same rejection with no retry path.

### L12. Guard deletion workaround on every client creation

`crates/tor-js/ts-wrapper/src/TorClient.ts:56`

Deletes `state:guards` from storage on every client creation as a workaround
for a bridge-switching bug. Causes unnecessary reconnection latency.

### L13. Partial writes treated as fatal in SMUX

`crates/tor-snowflake/src/smux.rs:765-796`

`poll_write()` treats partial writes as `WriteZero` (fatal). TCP can
legitimately write less than the full frame size under backpressure.

---

## Testing

- **tor-snowflake**: 68 tests across the crate (SMUX, KCP, Turbo, broker,
  transport). Good fuzz testing for segment encode/decode and Turbo framing.
  Missing: integration tests with mocked Snowflake server, concurrent stream
  tests, network degradation tests.
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
- `SnowflakeConfig` builder pattern cleaner than old function parameter approach
- Batch persistence optimization (`set_many()`) acquires lock once
- Fast-failure path: WebSocket errors surfaced to `ready()` for quick feedback
- Hardware-accelerated SHA-256 via `crypto.subtle.digest()` in fast bootstrap
- Incremental bootstrap yields control via `sleep(Duration::ZERO)` to prevent
  UI blocking on WASM
- CR/LF validation in fetch headers prevents HTTP header injection
- Structured retry logic with exponential backoff in tor-snowflake
- `wait_for_unlock()` future enables async lock coordination

---

## Already Fixed

These items were identified in earlier reviews and have been resolved:

1. **Header injection in tor-js fetch** -- CR/LF validation added
   (`fetch.rs:317-329`)
2. **`init()` panics on double-call** -- now uses `try_init()` which is
   idempotent (`lib.rs:106-110`)
3. **`create_snowflake_stream` ignores params** -- API refactored to single
   `create_snowflake_stream_with_config()` with `SnowflakeConfig` builder
4. **`export_keying_material` returned zeros** -- Snowflake streams now
   delegate to rustls, which properly implements RFC 5705 key export
5. **CA bundle fetch had no size limit / status check** -- Eliminated entirely;
   `webpki-roots` embeds Mozilla CA bundle at compile time
6. **Duration serde round-trip bug** -- Now uses `{:09}` zero-padding
7. **Misleading comment in wallclock()** -- Corrected
8. **Fragile error classification via string matching** -- tor-js now uses
   structured `ErrorKind` matching
9. **Possible string slicing panic** -- Replaced with `trim_matches('"')`
10. **Mutex poisoning panics in KCP** -- Now uses
    `.lock().unwrap_or_else(|e| e.into_inner())`
11. **Duplicate trait definitions in tor-persist** -- Consolidated into single
    `StringStore` trait via `AnyStateMgr`
12. **RwLock deadlock in BoxedDirStore** -- Locks are now explicitly dropped
    before re-acquisition
13. **KCP poll_write double-sends** -- Output buffering reworked to avoid
    re-queuing on `Pending`
14. **All subtle-tls issues** -- Resolved by replacing the custom TLS
    implementation with `rustls` + `rustls-rustcrypto`
