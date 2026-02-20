// Entry point: tor-js/wasm-file
// Loads WASM from a file alongside the JS module (Node.js filesystem or browser URL).
// This is the original loading behavior.

export { TorClient } from './TorClient.js';
export type { TorClientOptions, FetchInit, TorStorage } from './types.js';
export { Log, type LogLevel } from './Log.js';
export * as storage from './storage/index.js';
export { setWasmUrl } from './wasm.js';
