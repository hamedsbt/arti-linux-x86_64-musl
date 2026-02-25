import type { Log } from './Log.js';
import type { TorStorage } from '#wasm';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/**
 * Options for creating a TorClient.
 *
 * Provide exactly one of `bridge` (WebSocket mode) or `broker` (WebRTC mode).
 * The transport mode is inferred from which field is set.
 */
export type TorClientOptions = (
  {
    /** Snowflake WebSocket bridge URL (e.g., `"wss://snowflake.torproject.net/"`). */
    bridge: string;
  } | {
    /** Snowflake broker URL for WebRTC mode (e.g., `"https://snowflake-broker.torproject.net/"`). */
    broker: string;
  }
) & {
  /** Bridge fingerprint for identity verification (40-char hex string, e.g., `"2B280B23E1107BB62ABFC40DDCC8824814F80A72"`). */
  fingerprint: string;

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
