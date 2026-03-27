# Diff Analysis: `wasm-basic-compat` → `main` (at `2c7f788eb`)

Changes in `crates/`, excluding `crates/tor-js/`, `crates/tor-time/`,
and `crates/tor-async-compat/`. For non-crate changes (scripts,
examples, CI, root config), see `non-crate-changes.md`.

The `wasm-basic-compat` branch already contains the `tor-time` and
`tor-async-compat` crates and their full codebase migration
(`std::time` → `tor_time`, `async_trait` → `tor_async_compat`,
`coarsetime` → `tor_time::CoarseInstant`, etc.). Those changes are
**not analyzed here** — this document covers only the additional
changes on top of that baseline. Import changes referencing `tor_time`
or `tor_async_compat` that appear in this diff are residual
adjustments, not the primary migration.

---

## tor-js (+6,100 lines Rust, not analyzed here)

New crate (~6,100 lines Rust + TypeScript wrapper). WASM bindings for
arti-client exposing a `fetch()`-like API to JavaScript. Includes:

- **Rust** (`src/`): `TorClient` exposed via `wasm-bindgen`, HTTP/1.1
  fetch over Tor circuits with rustls TLS, `CachedJsStorage` bridging
  async JS storage to sync Rust reads, fast bootstrap from pre-packaged
  consensus ZIP, structured error types.
- **TypeScript** (`ts-wrapper/`): `TorClient`, `ArtiSocketProvider`
  (direct TCP / WebRTC / WebSocket strategies with fallback), storage
  adapters (IndexedDB, filesystem, memory, locking), logging, WASM
  loader with CDN/base64/file entry points.

See `wasm-notes/review.md` for a detailed code review of this crate.

---

## tor-rtcompat (+972 -37)

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

## Storage Changes

Changes across these three crates replace hard-coded filesystem/SQLite
storage with an injectable `KeyValueStore` trait. Users implement one
trait (get/set/delete strings by key), pass it to the builder, done.

- `KeyValueStore` lives in `tor-persist`
- `AnyStateMgr` (tor-persist) and `BoxedDirStore` (tor-dirmgr) both
  take `Arc<dyn KeyValueStore>` directly — no intermediate adapter traits
- `split_storage()` in arti-client passes the same Arc to both

### arti-client (+499 -14)

#### `src/storage.rs`
**What:** Re-exports `KeyValueStore` and `StorageError` from `tor-persist`. Provides `split_storage()` which wraps a `KeyValueStore` in an Arc and passes it to both `AnyStateMgr::from_custom()` and `BoxedDirStore::new()`. Includes unit tests with an in-memory store.

**Why:** Convenience API so users call `builder.storage(my_store)` and both state + directory subsystems use it.

#### New file: `examples/readme_custom_storage.rs`
**What:** Example demonstrating the `KeyValueStore` trait with a file-backed implementation.

#### `src/builder.rs`
**What:** Adds `statemgr: Option<AnyStateMgr>` and `dirstore: Option<BoxedDirStore>` fields to `TorClientBuilder`. New methods: `state_mgr()`, `dir_store()`, `storage()` (convenience that calls `split_storage`). New `resolve_statemgr()` method that returns the override or constructs from config (with `#[cfg(target_arch = "wasm32")]` error path). The builder now passes `statemgr` and `dirstore` through to `TorClient::create_inner`.

**Why:** WASM support — custom storage must be injectable.

#### `src/client.rs`
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

### tor-persist (+391 -2)

#### New file: `src/custom.rs`
**What:** Defines `KeyValueStore` trait (the single user-facing storage interface) and `AnyStateMgr` enum:
- `KeyValueStore`: object-safe trait with `get`, `set`, `delete`, `keys`, `try_lock`, `can_store`, `unlock`, `wait_for_unlock`
- `AnyStateMgr`: dispatches between `Fs(FsStateMgr)` (native) and `Custom(Arc<dyn KeyValueStore>)`
- Custom variant prefixes keys with `"state:"`, handles JSON serde
- Unit tests with in-memory store

**Why:** Core storage abstraction — the one trait users implement.

#### `src/err.rs`
**What:** New `Resource::Memory` variant. New public error constructors: `load_error()`, `store_error()`, `lock_error()`, `unlock_error()`.

**Why:** External `KeyValueStore` implementations need to construct errors.

#### `src/lib.rs`
**What:** `Result<T>` type alias changed from `pub(crate)` to `pub`. Exports `AnyStateMgr`, `KeyValueStore`, `StorageError` from `custom` module.

**Why:** External implementations need the `Result` type.

### tor-dirmgr (+679 -3)

#### `src/lib.rs`
**What:**
- `DirMgrStore::new()` gated behind `not(wasm32)`.
- New `DirMgrStore::from_custom_store()` method.
- Re-exports `BoxedDirStore`.

**Why:** WASM support — custom storage backend.

#### New file: `src/storage/custom.rs`
**What:** `BoxedDirStore` wrapping `Arc<dyn KeyValueStore>` directly, implementing the full `Store` trait:
- JSON-serializable types: `StoredConsensus`, `StoredAuthcert`, `StoredMicrodesc`, `StoredRouterdesc`, `StoredBridgedesc`, `StoredProtocols`
- Helper functions for key building (`dir:consensus:`, `dir:authcert:`, etc.), time conversion, hex encoding
- Full implementation of all `Store` methods (consensus, authcerts, microdescs, router descs, bridge descs, protocol recommendations, expiration)

**Why:** Directory storage adapter for non-SQLite backends (e.g., IndexedDB on WASM).

---

## tor-proto (+51 -36)

### `src/channel.rs`
**What:** `duration_unused()` method gets cfg-gated return: on WASM, returns `duration` directly (already `Option<Duration>`); on native, maps through `Into::into` (converting from `CoarseDuration`).

**Why:** `CoarseDuration` → `Duration` conversion differs on WASM.

### `src/lib.rs`
**What:** `time_since_last_incoming_traffic()` gets cfg-gated return (same pattern as `duration_unused()`).

**Why:** `CoarseDuration` → `Duration` conversion differs on WASM.

---

## tor-dirclient (+80 -1)

### `src/lib.rs`
**What:**
1. **New `RuzstdDecoder`** — a pure-Rust zstd decoder using `ruzstd` for WASM targets (where C `zstd-sys` is unavailable). Gated behind `#[cfg(all(feature = "zstd-wasm", not(feature = "zstd")))]`. Buffers the entire compressed stream then decompresses synchronously.
2. The `get_decoder` macro adds a case for `zstd-wasm`.

**Why:** WASM where C libraries can't be linked.

### `src/request.rs`
**What:** `all_encodings()` adds `x-zstd` when `zstd-wasm` feature is enabled.

**Why:** WASM uses pure-Rust zstd (`ruzstd`) and needs to advertise support.

---

## tor-circmgr (+25 -3)

### `src/build.rs`
**What:** `double_timeout` function split into two versions: native (spawns background task for the soft timeout) and WASM (simplified, just uses `abandon` timeout directly since WASM is single-threaded and can't spawn background tasks for the soft timeout pattern).

**Why:** The native version uses `spawn_obj` which requires `Send` — not available on WASM's single-threaded model.

---

## tor-basic-utils (+19 -15)

### `src/lib.rs`
**What:** `IoErrorExt::is_not_a_directory()` refactored from a single method with inline `#[cfg]` attributes around each error constant to three separate platform-specific method implementations (`unix`, `windows`, `not(any(unix, windows))`).

**Why:** WASM (and other non-unix/non-windows) platforms don't have `ENOTDIR` or `ERROR_DIRECTORY`. The previous code using inline `#[cfg]` inside a single method body wouldn't compile on WASM because neither branch would exist.

---

## tor-hsservice (+21 -10)

### `src/lib.rs`
**What:** The `PowManager::new()` call now wraps `status_tx.clone()` differently based on `hs-pow-full` feature:
- With `hs-pow-full`: wraps in `PowManagerStatusSender::from(status_tx.clone())`
- Without: passes `status_tx.clone()` directly

**Why:** The stub PowManager expects a plain `StatusSender` while the real one expects a `PowManagerStatusSender` newtype. This was likely a compile error fix.

---

## tor-memquota (+14 -5)

### `src/config.rs`
**What:** The 32-bit vs 64-bit memory threshold check is refactored from `#[cfg(target_pointer_width = "64")]` to a runtime boolean `is_64bit`.

**Why:** The `#[cfg]` attribute on an `if` condition doesn't work well (it gates the entire `if` statement, not just the condition). The refactoring makes the logic compile on all platforms.

---

## Minor Changes

Small cfg guards, clippy fixes, and cleanup that don't introduce new logic.

**WASM cfg guards (exclude incompatible code):**
- **fs-mistrust** `file_access.rs` — `#[cfg_attr(target_arch = "wasm32", expect(clippy::drop_non_drop))]` on `drop(tmp_file)`
- **hashx** `register.rs` — `#[cfg(feature = "compiler")]` → `#[cfg(all(feature = "compiler", not(target_arch = "wasm32")))]` on `RegisterId::as_u8()`
- **tor-dirmgr** `config.rs` — `open_store()` gated behind `not(wasm32)` (SQLite unavailable)
- **tor-dirmgr** `err.rs` — `SqliteError` variant gated behind `not(wasm32)`
- **tor-dirmgr** `storage.rs` — `File`, `IoResult`, `sqlite` module, `InputString::load()` gated behind `not(wasm32)`; `router_descs` field gets dead_code allowance on WASM
- **tor-rtcompat** `dyn_time.rs` — `PreferredRuntime` existence check gains `not(wasm32)` guard
- **tor-rtcompat** `impls.rs` — native module declarations, `tcp_listen()`, `impl_unix_non_provider` gated behind `not(wasm32)`
- **tor-rtcompat** `impls/streamops.rs` — `io` import and `UnsupportedStreamOp` gated behind `not(wasm32)`

**Clippy / lint fixes:**
- **tor-circmgr** `hspool.rs` — `#[allow(clippy::unused_async)]` on `maybe_extend_stem_circuit`
- **tor-memquota** `config.rs` — `#[expect(clippy::identity_op)]` on `1 * GIB`
- **tor-hsservice** `pow/v1_stub.rs` — doc comments and `#[allow(clippy::...)]` attrs on stub methods

**Cargo.toml changes (WASM platform gating):**
- **arti-client** — `mmap` feature extracted (was hardcoded in tor-dirmgr dep); WASM dev-deps added (`wasm-bindgen`, `wasm-bindgen-futures`, `console_error_panic_hook`, `tracing-wasm`)
- **tor-basic-utils** — `getrandom` with `wasm_js` feature for WASM random number generation
- **tor-dirmgr** — `rusqlite`/`fslock` moved behind `cfg(not(wasm32))`; `gloo-timers` added for WASM; `zstd-wasm` feature forwarded from `tor-dirclient`

**Cleanup:**
- **arti** `proxy.rs` — `#[cfg_attr(feature = "experimental-api", non_exhaustive)]` on stub `RpcMgr`
- **arti** `rpc_stub.rs` — removes unnecessary `visibility::make(pub)` attr
- **tor-chanmgr** `builder.rs` / `transport.rs` / `transport/proxied.rs` — `Send + Sync` bounds consolidated into `TransportImplHelper` trait definition
- **tor-proto** `maybenot_padding.rs` — `type Instant` reverted to `std::time::Instant` (native-only padding code)
- **arti-client** `lib.rs` — `pub mod storage;` re-export

See also `wasm-notes/trivial-changes.md` for mechanical fixes (clippy, cfg_attr).
