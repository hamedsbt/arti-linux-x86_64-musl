// Entry point: tor-js/wasm-cdn/singleton
// Singleton that downloads WASM from CDN with SHA256 hash verification.

import './index.js'; // side effect: registers CDN WASM source provider

export { tor } from '../../singleton.js';
export type { TorClientOptions, FetchInit, TorStorage } from '../../types.js';
export { Log, type LogLevel } from '../../Log.js';
export * as storage from '../../storage/index.js';
