// Annotations for each diff class. Run gen-fragment.mjs with each.
// This file is the single source of truth for all annotations.

export default [
  {
    id: "tor-rtcompat",
    title: "tor-rtcompat",
    stats: "+972 -37",
    annotation: "WASM runtime implementation. New <code>WasmRuntime</code> struct implementing the <code>Runtime</code> trait for WASM: <code>gloo-timers</code> for sleep, <code>wasm-bindgen-futures</code> for spawning, JS callback for socket connections, TLS via <code>rustls</code> + <code>rustls-rustcrypto</code>. Also <code>wasm_compat</code> module providing <code>Send</code>/<code>Sync</code> trait aliases that become no-ops on WASM. <code>SpawnExt</code> bounds relaxed to <code>wasm_compat::Send</code>.",
    files: {
      "Cargo.toml": { annotation: "Dependencies restructured: cross-platform vs native-only vs WASM-only" },
      "src/lib.rs": { annotation: "New wasm/wasm_compat modules, PreferredRuntime gated for WASM" },
      "src/traits.rs": { annotation: "SpawnExt bounds changed to wasm_compat::Send" },
      "src/wasm.rs": { annotation: "868-line WASM runtime: WasmRuntime, JsProxyStream, sleep, spawn", collapsed: true },
      "src/wasm_compat.rs": { annotation: "Send/Sync trait aliases — no-op on WASM" },
      "src/impls.rs": { annotation: "Native modules gated behind not(wasm32)" },
      "src/impls/streamops.rs": { annotation: "Stream ops imports gated for WASM" },
      "src/dyn_time.rs": { annotation: "PreferredRuntime macro gated for WASM" },
      "src/compound.rs": { annotation: "CoarseTimeProvider import from tor_time" },
      "src/general.rs": { annotation: "Import update" },
      "src/opaque.rs": { annotation: "Import update" },
      "src/scheduler.rs": { annotation: "Import update" },
    }
  },
  {
    id: "arti-client",
    title: "Storage Changes: arti-client",
    stats: "+717 -18",
    annotation: "Public API for custom storage. New <code>KeyValueStore</code> trait and <code>split_storage()</code> function. <code>TorClientBuilder</code> gains <code>storage()</code> method for injecting custom backends. <code>TorClient</code> internals refactored from <code>FsStateMgr</code> to <code>AnyStateMgr</code>.",
    files: {
      "Cargo.toml": { annotation: "mmap feature extracted, WASM dev-deps added" },
      "src/storage.rs": { annotation: "New: KeyValueStore trait, split_storage(), adapters", collapsed: true },
      "examples/readme_custom_storage.rs": { annotation: "New: example of file-backed KeyValueStore", collapsed: true },
      "src/builder.rs": { annotation: "Storage injection via state_mgr(), dir_store(), storage()" },
      "src/client.rs": { annotation: "FsStateMgr → AnyStateMgr, custom store dispatch", collapsed: true },
      "src/lib.rs": { annotation: "pub mod storage, re-exports" },
      "src/protostatus.rs": { annotation: "Time compat import" },
      "src/status.rs": { annotation: "Time compat import" },
    }
  },
  {
    id: "tor-persist",
    title: "Storage Changes: tor-persist",
    stats: "+365 -5",
    annotation: "Core storage abstraction. New <code>StringStore</code> trait (object-safe, JSON strings) and <code>AnyStateMgr</code> enum dispatching between filesystem and custom backends.",
    files: {
      "Cargo.toml": { annotation: "New dependency on tor-time" },
      "src/custom.rs": { annotation: "New: StringStore trait, AnyStateMgr, unit tests", collapsed: true },
      "src/err.rs": { annotation: "New Resource::Memory variant, public error constructors" },
      "src/lib.rs": { annotation: "Result type made public, new exports" },
      "src/fs.rs": { annotation: "Import update" },
      "src/state_dir.rs": { annotation: "Import update" },
    }
  },
  {
    id: "tor-dirmgr",
    title: "Storage Changes: tor-dirmgr",
    stats: "+721 -14",
    annotation: "Directory storage adapter. New <code>CustomDirStore</code> trait and <code>BoxedDirStore</code> wrapper implementing the full <code>Store</code> trait over a key-value interface. SQLite gated behind <code>not(wasm32)</code>.",
    files: {
      "Cargo.toml": { annotation: "rusqlite/fslock gated not(wasm32), gloo-timers + zstd-wasm added" },
      "src/storage/custom.rs": { annotation: "New: 677-line CustomDirStore, BoxedDirStore, JSON types", collapsed: true },
      "src/lib.rs": { annotation: "from_custom_store(), DirMgrStore::new() gated" },
      "src/storage.rs": { annotation: "sqlite/filesystem gated, custom module added" },
      "src/config.rs": { annotation: "open_store() gated behind not(wasm32)" },
      "src/err.rs": { annotation: "SqliteError gated behind not(wasm32)" },
      "src/state.rs": { annotation: "Time compat import" },
      "src/docmeta.rs": { annotation: "Import update" },
    }
  },
  {
    id: "tor-proto",
    title: "tor-proto",
    stats: "+51 -36",
    annotation: "Cfg-gated <code>CoarseDuration</code> → <code>Duration</code> returns on WASM (conversion differs). <code>duration_unused()</code> and <code>time_since_last_incoming_traffic()</code> return the value directly on WASM instead of going through <code>Into::into</code>.",
    files: {
      "Cargo.toml": { annotation: "Dependency updates" },
      "src/channel.rs": { annotation: "duration_unused() cfg-gated return" },
      "src/lib.rs": { annotation: "time_since_last_incoming_traffic() cfg-gated return" },
    }
  },
  {
    id: "tor-dirclient",
    title: "tor-dirclient",
    stats: "+80 -1",
    annotation: "Pure-Rust zstd for WASM. New <code>RuzstdDecoder</code> using <code>ruzstd</code> crate, gated behind <code>zstd-wasm</code> feature. Advertises <code>x-zstd</code> encoding when the feature is enabled.",
    files: {
      "Cargo.toml": { annotation: "New zstd-wasm feature, ruzstd dependency" },
      "src/lib.rs": { annotation: "RuzstdDecoder + get_decoder case for zstd-wasm" },
      "src/request.rs": { annotation: "all_encodings() adds x-zstd for zstd-wasm" },
    }
  },
  {
    id: "tor-circmgr",
    title: "tor-circmgr",
    stats: "+25 -3",
    annotation: "<code>double_timeout</code> split into native (spawns background task) and WASM (uses abandon timeout directly, no background task since WASM is single-threaded).",
    files: {
      "Cargo.toml": { annotation: "Dependency updates" },
      "src/build.rs": { annotation: "double_timeout native/WASM split" },
      "src/hspool.rs": { annotation: "Clippy allow for unused_async" },
    }
  },
  {
    id: "tor-basic-utils",
    title: "tor-basic-utils",
    stats: "+19 -15",
    annotation: "<code>IoErrorExt::is_not_a_directory()</code> refactored into three platform-specific implementations (unix, windows, other). Previous inline <code>#[cfg]</code> wouldn't compile on WASM.",
    files: {
      "Cargo.toml": { annotation: "getrandom with wasm_js feature" },
      "src/lib.rs": { annotation: "Platform-specific is_not_a_directory() methods" },
    }
  },
  {
    id: "tor-hsservice",
    title: "tor-hsservice",
    stats: "+21 -10",
    annotation: "<code>PowManager::new()</code> dispatch fix: stub expects plain <code>StatusSender</code>, real impl expects <code>PowManagerStatusSender</code> newtype. Stub methods get doc comments and clippy allows.",
    files: {
      "src/lib.rs": { annotation: "PowManager dispatch based on hs-pow-full feature" },
      "src/pow/v1_stub.rs": { annotation: "Doc comments and clippy attrs on stub" },
    }
  },
  {
    id: "tor-memquota",
    title: "tor-memquota",
    stats: "+14 -5",
    annotation: "32-bit vs 64-bit memory threshold refactored from compile-time <code>#[cfg]</code> on an <code>if</code> condition to a runtime boolean. The <code>#[cfg]</code> approach gated the entire <code>if</code> statement.",
    files: {
      "src/config.rs": { annotation: "cfg → runtime boolean for pointer width check" },
    }
  },
  {
    id: "tor-chanmgr",
    title: "tor-chanmgr",
    stats: "+6 -7 (minor)",
    annotation: "<code>Send + Sync</code> bounds consolidated into <code>TransportImplHelper</code> trait definition, removed from individual impl sites.",
    files: {
      "src/builder.rs": { annotation: "Redundant Send+Sync bounds removed" },
      "src/transport.rs": { annotation: "Send+Sync added to trait definition" },
      "src/transport/proxied.rs": { annotation: "Redundant Send+Sync removed" },
    }
  },
  {
    id: "minor-crates",
    title: "Minor Crate Changes",
    stats: "various",
    annotation: "Small cfg guards, clippy fixes, and cleanup across multiple crates.",
    files: {}
  },
  {
    id: "examples",
    title: "Examples (tor-js)",
    stats: "+1581",
    annotation: "Node.js CLI examples and browser showcase for tor-js.",
    files: {
      "examples/tor-js/tor-fetch.js": { annotation: "Main CLI example with --websocket and --in-memory flags" },
      "examples/tor-js/tor-fetch-abort.js": { annotation: "AbortSignal test scenarios", collapsed: true },
      "examples/tor-js/tor-fetch-streaming.js": { annotation: "Streaming response with match counting", collapsed: true },
      "examples/tor-js/tor-fetch-singleton.js": { annotation: "Minimal singleton API example" },
      "examples/tor-js/showcase/index.html": { annotation: "Browser demo — single-page app", collapsed: true },
      "examples/tor-js/showcase/run.sh": { annotation: "Local HTTP server for demo" },
      "examples/tor-js/showcase/tor.svg": { annotation: "Logo SVG" },
    }
  },
  {
    id: "scripts",
    title: "Scripts",
    stats: "+240",
    annotation: "Build, check, and deployment scripts.",
    files: {
      "scripts/check.sh": { annotation: "Runs cargo check + clippy across native + WASM targets" },
      "scripts/todos.sh": { annotation: "Grep for TODO/FIXME" },
      "scripts/tor-js/build.sh": { annotation: "wasm-pack build + TypeScript wrapper build" },
      "scripts/tor-js/push-hash-artifact.sh": { annotation: "Publish WASM binary to hash-artifacts branch", collapsed: true },
    }
  },
  {
    id: "ci",
    title: "CI (GitHub Actions)",
    stats: "+54",
    annotation: "Showcase deployment workflow.",
    files: {
      ".github/workflows/showcase.yml": { annotation: "Build and deploy tor-js showcase to GitHub Pages" },
    }
  },
  {
    id: "root-config",
    title: "Root Configuration",
    stats: "+8 -3",
    annotation: "Workspace and tooling configuration.",
    files: {
      "Cargo.toml": { annotation: "tor-js added to workspace members" },
      "clippy.toml": { annotation: "allow-invalid for rand path on wasm32" },
      ".gitignore": { annotation: "node_modules added" },
    }
  },
];
