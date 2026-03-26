# Diff Analysis: `wasm-basic-compat` → `main`

All `.rs` file changes in `crates/`, excluding `crates/tor-js/`,
`crates/tor-time/`, and `crates/tor-async-compat/`.

The `wasm-basic-compat` branch already contains the `tor-time` and
`tor-async-compat` crates and their full codebase migration
(`std::time` → `tor_time`, `async_trait` → `tor_async_compat`,
`coarsetime` → `tor_time::CoarseInstant`, etc.). Those changes are
**not analyzed here** — this document covers only the additional
changes on top of that baseline. Import changes referencing `tor_time`
or `tor_async_compat` that appear in this diff are residual
adjustments, not the primary migration.

---

## arti-client

### New file: `src/storage.rs`
**What:** Adds a `KeyValueStore` trait and `split_storage()` function. The trait is a simple key-value interface (`get`, `set`, `delete`, `keys`, `try_lock`, `can_store`, `unlock`, `wait_for_unlock`). `split_storage` creates two adapters from a single store: a `KvStateAdapter` (implements `StringStore`, prefixes keys with `"state:"`) and a `KvDirAdapter` (implements `CustomDirStore`, passes keys through). Includes unit tests with an in-memory store.

**Why:** WASM support. There is no filesystem or SQLite on WASM, so users need to provide custom storage. This is the public API surface for that.

**Notes:** Well-structured. The `StorageError` type is `Box<dyn Error + Send + Sync>` which is simple but adequate.

### New file: `examples/readme_custom_storage.rs`
**What:** Example demonstrating the `KeyValueStore` trait with a file-backed implementation.

**Why:** Documentation/onboarding for the new storage API.

### `src/builder.rs`
**What:** Adds `statemgr: Option<AnyStateMgr>` and `dirstore: Option<BoxedDirStore>` fields to `TorClientBuilder`. New methods: `state_mgr()`, `dir_store()`, `storage()` (convenience that calls `split_storage`). New `resolve_statemgr()` method that returns the override or constructs from config (with `#[cfg(target_arch = "wasm32")]` error path). The builder now passes `statemgr` and `dirstore` through to `TorClient::create_inner`.

**Why:** WASM support — custom storage must be injectable.

### `src/client.rs`
**What:**
- `statemgr` field changed from `FsStateMgr` to `AnyStateMgr`.
- `pt_mgr` field gated with `not(target_arch = "wasm32")` (WASM has no process-based pluggable transports).
- `create_keymgr()` returns `Ok(None)` on WASM (no filesystem for keystores).
- `create_inner()` now takes `statemgr: AnyStateMgr` and `dirstore: Option<BoxedDirStore>` params; the `FsStateMgr::from_path_and_mistrust` call is removed from this function (moved to builder).
- `DirMgrStore` construction dispatches to `from_custom_store()` or `new()` based on whether a custom store was provided (with WASM error fallback).
- `reconfigure()`: state directory comparison gated behind `not(wasm32)`, uses `self.statemgr.path().is_some_and(...)`.
- `wait_for_stop()`: split into two cfg-gated versions (native uses `Either`, WASM only has Custom variant). Both use `use<'_, R>` lifetime capture.

**Why:** All for WASM compatibility — abstracting away filesystem assumptions.

**Flag:** The `wait_for_stop()` split into two near-identical methods (native vs WASM) is a bit unfortunate. A single method using the `AnyStateMgr::wait_for_unlock()` should work for both since `AnyStateMgr` already handles the dispatch internally.

### `src/lib.rs`
**What:** Adds `pub mod storage;`, re-exports `KeyValueStore`.

**Why:** New storage module for WASM custom storage support.

---

## arti

### `src/proxy.rs`
**What:** Adds `#[cfg_attr(feature = "experimental-api", non_exhaustive)]` to the stub `RpcMgr` enum.

**Why:** Prevents exhaustive matching when `experimental-api` is enabled. Minor cleanup.

### `src/rpc_stub.rs`
**What:** Removes `#[cfg_attr(feature = "experimental-api", visibility::make(pub))]` from `RpcProxySupport`.

**Why:** Cleanup — this type doesn't need to be public even with experimental-api.

---

## fs-mistrust

### `src/file_access.rs`
**What:** Adds `#[cfg_attr(target_arch = "wasm32", expect(clippy::drop_non_drop))]` on a `drop(tmp_file)` call.

**Why:** On WASM, the file type might not have a meaningful Drop, triggering a clippy warning.

---

## hashx

### `bench/benches/hashx_bench.rs`
**What:** `tor_time::Instant` → `std::time::Instant`.

**Why:** Benchmark code — uses native `Instant` directly since benchmarks only run on native.

**FLAG:** This is a reversion from `tor_time::Instant` back to `std::time::Instant`. This is correct for benchmark code that only runs on native, but it's inconsistent with the migration direction.

### `src/register.rs`
**What:** `#[cfg(feature = "compiler")]` → `#[cfg(all(feature = "compiler", not(target_arch = "wasm32")))]` on `RegisterId::as_u8()`.

**Why:** The hashx compiler feature generates native code, which doesn't work on WASM.

---

## retry-error

### `src/lib.rs`
**What:** Doc example changed from `tor_time::{Instant, SystemTime}` to `std::time::{Instant, SystemTime}`.

**Why:** The doc example shows user-facing code. Since `retry-error` is a standalone crate, using `std::time` in docs is correct.

**FLAG:** The doctest explicitly uses `std::time::{Instant, SystemTime}`. If retry-error is used in WASM contexts, this doctest would fail to compile since `std::time::SystemTime` is not available on WASM. However, retry-error itself doesn't use these types internally (they're only in the example), so this is likely fine for now.

---

## tor-basic-utils

### `src/lib.rs`
**What:** `IoErrorExt::is_not_a_directory()` refactored from a single method with inline `#[cfg]` attributes around each error constant to three separate platform-specific method implementations (`unix`, `windows`, `not(any(unix, windows))`).

**Why:** WASM (and other non-unix/non-windows) platforms don't have `ENOTDIR` or `ERROR_DIRECTORY`. The previous code using inline `#[cfg]` inside a single method body wouldn't compile on WASM because neither branch would exist.

---

## tor-cert

### `tests/invalid_certs.rs`
**What:** Changes commented-out import from `use tor_time::SystemTime;` to `//use std::time::{Duration, SystemTime};`.

**Why:** Cleanup of dead code in tests.

**FLAG:** This is cosmetic — the import is commented out in both versions. No functional impact.

---

## tor-chanmgr

### `src/builder.rs`
**What:** Removes redundant `Send + Sync` bounds from `ChannelFactory` and `IncomingChannelFactory` impls for `ChanBuilder<R, H>`. The bounds are already implied by the trait bounds on the types.

**Why:** Cleanup. The `TransportImplHelper` trait now has `Send + Sync` supertraits (see `transport.rs`), making the explicit bounds redundant.

### `src/factory.rs`
**What:**
- `BootstrapReporter` gets four new public methods: `record_attempt()`, `record_tcp_success()`, `record_tls_finished()`, `record_handshake_done()`. These expose the previously private `ChanMgrEventSender` methods.
- Updates doc comments to be more descriptive.
- Removes redundant `Sync` bound from `AbstractChannelFactory` impl.

**Why:** Custom `ChannelFactory` implementations (e.g., WASM's WebSocket transport) need to report bootstrap progress.

### `src/lib.rs`
**What:** `#[allow(unused)]` → `#[cfg_attr(not(feature = "relay"), expect(dead_code))]` on the `runtime` field.

**Why:** Cleanup — more precise dead_code suppression.

### `src/transport.rs`
**What:** `TransportImplHelper` trait gets `Send + Sync` supertraits added.

**Why:** The bounds were previously specified at each impl site. Moving them to the trait definition is cleaner and ensures all implementations satisfy the bounds.

### `src/transport/proxied.rs`
**What:** Removes redundant `Send + Sync` bound from `ExternalProxyPlugin` impl.

**Why:** Now implied by `TransportImplHelper: Send + Sync`.

---

## tor-circmgr

### `src/build.rs`
**What:** `double_timeout` function split into two versions: native (spawns background task for the soft timeout) and WASM (simplified, just uses `abandon` timeout directly since WASM is single-threaded and can't spawn background tasks for the soft timeout pattern).

**Why:** The native version uses `spawn_obj` which requires `Send` — not available on WASM's single-threaded model.

### `src/hspool.rs`
**What:** Adds `#[allow(clippy::unused_async)]` to `maybe_extend_stem_circuit`.

**Why:** The method is async only when certain features (`vanguards` + `hs-common`) are enabled. Without them, clippy warns about unused async.

### `src/preemptive.rs`
**What:** In test code: `tor_time::Instant` → `std::time::Instant` (combined with Duration import).

**Why:** Test code only runs on native.

**FLAG:** Reversion from `tor_time::Instant` to `std::time::Instant` in test code. Acceptable since tests only run on native, but inconsistent with the migration pattern in non-test code.

---

## tor-config-path

### `src/addr.rs`
**What:** Changes `return Err(...)` to `Err(...)` (removes unnecessary `return` keyword).

**Why:** Clippy cleanup — the `return` was inside a `#[cfg(not(unix))]` block that is the last expression in the arm.

---

## tor-dirclient

### `src/lib.rs`
**What:**
1. **New `truncate_middle()` helper** — truncates long strings by keeping head/tail with "..." in the middle.
2. **Added debug logging** for HTTP responses — logs request path (truncated), encoding, bytes read, and success status.
3. **New `RuzstdDecoder`** — a pure-Rust zstd decoder using `ruzstd` for WASM targets (where C `zstd-sys` is unavailable). Gated behind `#[cfg(all(feature = "zstd-wasm", not(feature = "zstd")))]`. Buffers the entire compressed stream then decompresses synchronously.
4. The `get_decoder` macro adds a case for `zstd-wasm`.

**Why:** Items 1-2 are debugging improvements. Items 3-4 are for WASM where C libraries can't be linked.

Read timeout change (total→idle) was reverted to upstream's original total timeout.

### `src/request.rs`
**What:**
1. `max_response_len()` for `MicrodescRequest` gets a WASM-specific variant with 16KB per microdesc (vs 8KB on native) and a 128KB minimum.
2. `all_encodings()` adds `x-zstd` when `zstd-wasm` feature is enabled.

**Why:** WASM uses pure-Rust decompression which has different overhead characteristics. The larger buffer prevents truncation issues with zstd-wasm.

---

## tor-dircommon

### `src/retry.rs`
**What:** Default parallelism for `DownloadSchedule` changed to 2 on WASM (vs 4 on native).

**Why:** WASM is single-threaded; lower parallelism reduces memory pressure and avoids overwhelming the single-threaded event loop.

---

## tor-dirmgr

### `src/config.rs`
**What:** `open_store()` gated behind `#[cfg(not(target_arch = "wasm32"))]`. Imports of `Result` and `DynStore` similarly gated.

**Why:** SQLite-based store is not available on WASM.

### `src/err.rs`
**What:** `SqliteError` variant and all related match arms gated behind `#[cfg(not(target_arch = "wasm32"))]`. `from_lockfile` gets `#[cfg_attr(target_arch = "wasm32", expect(dead_code))]`.

**Why:** SQLite is not available on WASM.

### `src/lib.rs`
**What:**
- `DirMgrStore::new()` gated behind `not(wasm32)`.
- New `DirMgrStore::from_custom_store()` method.
- Re-exports `BoxedDirStore`, `CustomDirStore`.

**Why:** WASM support — custom storage backend.

### `src/state.rs`
**What:** Removes commented-out `.get_mut()` call.

**Why:** Dead comment cleanup.

### `src/storage.rs`
**What:** `File`, `IoResult`, `sqlite` module gated behind `not(wasm32)`. New `custom` module imported unconditionally. `InputString::load()` gated behind `not(wasm32)`. `ExpirationConfig::router_descs` gets dead_code allowance on WASM.

**Why:** Separating filesystem-dependent code from cross-platform code.

### New file: `src/storage/custom.rs`
**What:** 677-line new file implementing:
- `CustomDirStore` trait (object-safe: `load`, `store`, `delete`, `keys`, `is_readonly`, `upgrade_to_readwrite`)
- JSON-serializable types: `StoredConsensus`, `StoredAuthcert`, `StoredMicrodesc`, `StoredRouterdesc`, `StoredBridgedesc`, `StoredProtocols`
- Helper functions for key building, time conversion, hex encoding
- `BoxedDirStore` wrapper implementing the full `Store` trait
- Full implementation of all `Store` methods (consensus, authcerts, microdescs, router descs, bridge descs, protocol recommendations, expiration)

**Why:** This is the directory storage adapter for custom backends. Allows non-SQLite storage (e.g., IndexedDB on WASM, or any key-value store).

*(`str_to_flavor` dead code was identified and removed.)*

---

## tor-dirserver

### `src/mirror/operation.rs`
**What:** In test code: `tor_time::SystemTime` → `std::time::SystemTime`.

**Why:** Test-only code that runs on native.

**FLAG:** Reversion in test code. Same pattern as other test reversions.

---

## tor-error

Time/async-compat migration only.

---

## tor-guardmgr

### `src/sample.rs`
**What:** Significant logic change in `select_guards_for_descriptor_purposes()`:
- Primary guards now bypass the `reachable() != Unreachable` filter.
- Non-primary guards still get reachability checks.
- Comment explains: discarding a primary bridge's descriptor creates a chicken-and-egg problem where the guard becomes "unsuitable to purpose" until re-fetched, but the descriptor is needed to test reachability.

**Why:** Bug fix for bridge descriptor management. Without this, a temporarily-unreachable primary bridge would lose its descriptor, making it permanently unavailable.

**This is a real bug fix, not just WASM-related.** Worth noting for review.

---

## tor-hsclient

Time/async-compat migration only.

---

## tor-hsservice

### `src/lib.rs`
**What:** The `PowManager::new()` call now wraps `status_tx.clone()` differently based on `hs-pow-full` feature:
- With `hs-pow-full`: wraps in `PowManagerStatusSender::from(status_tx.clone())`
- Without: passes `status_tx.clone()` directly

**Why:** The stub PowManager expects a plain `StatusSender` while the real one expects a `PowManagerStatusSender` newtype. This was likely a compile error fix.

### `src/pow/v1_stub.rs`
**What:** Adds doc comments and `#[allow(clippy::...)]` attributes to the stub PowManager methods to match the real implementation's signature.

**Why:** Clippy cleanup for the stub that must match the real impl's API.

### `src/rend_handshake.rs`
**What:** `RendCircConnector::now()` return type changed from `tor_time::Instant` to `std::time::Instant`.

**Why:** Consistent with the hsservice's use of native `Instant` for timeout tracking.

---

## tor-key-forge

Time/async-compat migration only.

---

## tor-memquota

### `src/config.rs`
**What:**
1. `1 * GIB` gets `#[expect(clippy::identity_op)]` with a reason comment.
2. The 32-bit vs 64-bit memory threshold check is refactored from `#[cfg(target_pointer_width = "64")]` to a runtime boolean `is_64bit`.

**Why:**
1. Clippy warns about `1 *` being a no-op multiplication; the expect says it's for consistency with `8 * GIB`.
2. The `#[cfg]` attribute on an `if` condition doesn't work well (it gates the entire `if` statement, not just the condition). The refactoring makes the logic compile on all platforms.

---

## tor-netdir

### `src/testnet.rs`
**What:** `tor_time::SystemTime` → `std::time::SystemTime`.

**Why:** Test code that only runs on native.

### `src/testprovider.rs`
**What:** `tor_time::SystemTime` → `std::time::SystemTime` in `protocol_statuses()` return type.

**FLAG:** This is in a public method signature of `TestNetDirProvider`. This reverts `SystemTime` to `std::time::SystemTime` in the test provider. Since this is test infrastructure, it's less concerning, but it's inconsistent with the migration direction. If `NetDirProvider::protocol_statuses()` uses `tor_time::SystemTime` in its trait definition, this impl should match.

---

## tor-netdoc

Time/async-compat migration only.

---

## tor-persist

### New file: `src/custom.rs`
**What:** 325-line new file implementing:
- `StringStore` trait (object-safe: `load_str`, `store_str`, `can_store`, `try_lock`, `unlock`, `wait_for_unlock`)
- `AnyStateMgr` enum dispatching between `Fs(FsStateMgr)` (native only) and `Custom(Arc<dyn StringStore>)`
- `StateMgr` impl for `AnyStateMgr` that delegates to either variant
- `path()` method that returns `None` for custom backends
- `wait_for_unlock()` with cfg-gated implementations
- Unit tests with in-memory store

**Why:** Core abstraction for custom storage backends.

### `src/err.rs`
**What:** New `Resource::Memory` variant. New public error constructors: `load_error()`, `store_error()`, `lock_error()`, `unlock_error()`.

**Why:** External `StringStore` implementations need to construct errors.

### `src/lib.rs`
**What:** `Result<T>` type alias changed from `pub(crate)` to `pub`. Exports `AnyStateMgr`, `StringStore` from `custom` module.

**Why:** External implementations need the `Result` type.

---

## tor-proto

### `src/channel.rs`
**What:** `duration_unused()` method gets cfg-gated return: on WASM, returns `duration` directly (already `Option<Duration>`); on native, maps through `Into::into` (converting from `CoarseDuration`).

**Why:** `CoarseDuration` → `Duration` conversion differs on WASM.

### `src/client/circuit/padding/maybenot_padding.rs`
**What:** `type Instant = tor_time::Instant` → `type Instant = std::time::Instant`.

**FLAG:** Reversion. The padding code uses `Instant` for high-precision timing of padding injections. This only runs on native (WASM doesn't support circuit padding). The reversion is intentional but should be noted.

### `src/congestion/rtt.rs`
**What:** In test code: `tor_time::Instant` → `std::time::Instant`.

**Why:** Test code on native.

### `src/lib.rs`
**What:** `time_since_last_incoming_traffic()` gets cfg-gated return (same pattern as `duration_unused()`).

**Why:** `CoarseDuration` → `Duration` conversion differs on WASM.

### `src/relay/channel/initiator.rs`, `src/relay/channel/responder.rs`
**What:** `now: Option<tor_time::SystemTime>` → `now: Option<std::time::SystemTime>`.

**FLAG:** These are in the relay-side channel code (initiator/responder verification). Reverting to `std::time::SystemTime` is correct since relay code only runs on native, but it's inconsistent with the client-side channel code that uses `tor_time::SystemTime`. This could be confusing for developers working across both sides.

---

## tor-rtcompat

### `src/dyn_time.rs`
**What:** `PreferredRuntime` existence check gains `not(target_arch = "wasm32")` guard.

**Why:** WASM doesn't have PreferredRuntime.

### `src/impls.rs`
**What:** All feature-gated module declarations gain `not(target_arch = "wasm32")` guards. `LISTEN_BACKLOG` and `tcp_listen()` gated behind `not(wasm32)`. `impl_unix_non_provider` macro gated behind `not(wasm32)`.

**Why:** None of these native-only impls compile on WASM.

### `src/impls/streamops.rs`
**What:** `io` import and `UnsupportedStreamOp` gated behind `not(wasm32)`.

**Why:** Stream operations (TCP socket options) don't exist on WASM.

### `src/lib.rs`
**What:**
- New `pub mod wasm_compat;` and `pub mod wasm;` (WASM-only).
- All PreferredRuntime-related `#[cfg]` blocks gain `not(target_arch = "wasm32")`.
- Various feature gate combinations updated.

**Why:** Core WASM support.

### `src/traits.rs`
**What:**
- `SpawnExt::spawn()`: bound changed from `Send` to `crate::wasm_compat::Send`. On WASM, uses `wasm_bindgen_futures::spawn_local` instead of `spawn_obj`.
- `SpawnExt::spawn_with_handle()`: bounds changed to `crate::wasm_compat::Send`.
- Doc comment added about `unsafe impl Send` on WASM types.

**Why:** WASM support — WASM is single-threaded, so `Send` bounds need to be relaxed.

### New file: `src/wasm.rs`
**What:** 868-line WASM runtime implementation including:
- `WasmRuntime` struct with `SleepProvider`, `CoarseTimeProvider`, `Spawn`, `Blocking` (panics), `NetStreamProvider` (with JS callback support), `UdpProvider` (stub), `TlsProvider` (stub) implementations.
- `WasmSleepFuture` using `gloo_timers`.
- `JsProxyStream` for JS socket objects.
- Stubs for UDP and TLS (WASM uses external TLS).
- `unsafe impl Send for WasmSleepFuture` and `unsafe impl Send/Sync for JsProxyStream`.

**Why:** Core WASM runtime.

**Flags on `unsafe impl Send`:** Both `WasmSleepFuture` and `JsProxyStream` have `unsafe impl Send`. This is necessary because WASM JS types aren't `Send`, but WASM is single-threaded so this is safe. The safety comments are present but brief. These are standard WASM patterns.

### New file: `src/wasm_compat.rs`
**What:** 35-line module providing `Send` and `Sync` trait aliases. On native: re-exports `std::marker::{Send, Sync}`. On WASM: provides empty auto-implemented traits.

**Why:** Allows code to use `wasm_compat::Send` in bounds that become no-ops on WASM.

---

## tor-rtmock

### `tests/rtcompat_timing.rs`
**What:** `tor_time::SystemTime` → `std::time::SystemTime`.

**FLAG:** Test reversion. The test file uses `std::time::{Duration, SystemTime}` instead of `tor_time` equivalents. Since this is a test that only runs on native, it's fine, but inconsistent.

---

## Summary of Flagged Issues

### Resolved

1. **`tor-proto/src/client/reactor/circuit.rs`** — ~~Unnecessary `.collect()` into Vec before `.any()`.~~ **FIXED:** Reverted to upstream's direct `.any()` on iterator. Was leftover from removed debug counters.

2. **`tor-dirclient/src/lib.rs`** — ~~Read timeout changed from total to per-read idle.~~ **FIXED:** Reverted to upstream's total timeout. The idle timeout was for Snowflake compatibility which has been removed.

3. **`tor-dirmgr/src/bootstrap.rs`** — ~~Streaming downloads with `#[cfg(test)]`/`#[cfg(not(test))]` split.~~ **FIXED:** Reverted to upstream's batch approach. The streaming improvement is documented in `wasm-notes/potential-improvements.md` for future work with proper test coverage.

4. **`tor-dirmgr/src/docid.rs`** — ~~Dead `MICRODESC_N` constant.~~ **FIXED:** Reverted to upstream.

5. **`tor-hsclient/src/pow/v1.rs`, `tor-hsservice/src/timeout_track.rs`, `tor-hsservice/src/time_store.rs`** — ~~Missed `std::time::Instant` migrations.~~ **FIXED:** Changed to `tor_time::Instant`.

### Open (Low Priority)

- **`tor-dirmgr/src/storage/custom.rs`** — `str_to_flavor()` is `#[allow(dead_code)]`. Should be removed if unused.

- **`tor-cert/tests/invalid_certs.rs`** — Editing a commented-out import is pure cosmetic noise.

### Remaining `std::time` Direct Usage (Acceptable)

These files use `std::time` types directly. All are server-side, native-only, or test-only code that won't run on WASM:

- **Native-only code:** `hashx/bench`, `tor-proto/padding`, `tor-proto/relay/initiator+responder`, `tor-circmgr/preemptive` (test), `tor-proto/congestion/rtt` (test)
- **Relay/hsservice code:** `tor-hsservice` (multiple files using `std::time::Instant` for timeout tracking)
- **Test code:** `tor-netdir/testnet.rs`, `tor-netdir/testprovider.rs`, `tor-dirserver/mirror/operation.rs` (test), `tor-rtmock/tests`
- **Docs:** `retry-error/src/lib.rs`
