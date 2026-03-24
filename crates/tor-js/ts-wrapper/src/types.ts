import type { Log } from './Log.js';
import type { TorStorage } from '#wasm';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/**
 * Options for creating a TorClient.
 *
 * The gateway provides WebSocket relay and optional fast bootstrap.
 * Arti connects to regular Tor relays through the gateway's WebSocket proxy.
 */
export type TorClientOptions = {
  /** Gateway URL (e.g., `"https://tor-js-gateway.voltrevo.com"`). */
  gateway: string;

  /**
   * Optional logger instance.
   * Note: WASM logging is global, so all TorClient instances receive all WASM
   * log events, not just their own. This is because wasm-bindgen generates a
   * single module-level instance (`let wasm;`), so all Rust global state
   * (including the tracing subscriber) is shared.
   */
  log?: Log;

  /** Optional storage for persistent state (implements TorStorage). */
  storage?: TorStorage;

  /**
   * Minimum log level for this client's log listener. Defaults to 'debug'.
   * Can be changed at runtime via `TorClient.setLogLevel()`.
   * The WASM subscriber auto-widens to the broadest level across all clients.
   */
  logLevel?: LogLevel;
};

export type { TorStorage } from '#wasm';

export interface FetchInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string | Uint8Array | ArrayBuffer;
  signal?: AbortSignal;
}
