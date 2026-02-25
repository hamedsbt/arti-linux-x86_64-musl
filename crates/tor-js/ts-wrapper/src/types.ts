import type { Log } from './Log.js';
import type { TorStorage } from '#wasm';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface TorClientOptions {
  /** Snowflake WebSocket bridge URL (e.g., "wss://snowflake.pse.dev/"). Used in 'websocket' mode. */
  snowflakeUrl: string;

  /** Bridge fingerprint for identity verification (40-char hex string). */
  fingerprint: string;

  /**
   * Transport mode: 'websocket' (default) or 'webrtc'.
   * When 'websocket', uses snowflakeUrl to connect directly to the bridge.
   * When 'webrtc', uses brokerUrl (or the default Tor Project broker) for signaling.
   */
  mode?: 'websocket' | 'webrtc';

  /**
   * Snowflake broker URL for WebRTC mode (e.g., "https://snowflake-broker.torproject.net/").
   * Only used when mode is 'webrtc'. Defaults to the Tor Project's broker if omitted.
   */
  brokerUrl?: string;

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

  // Accepted for API compatibility with original tor-js, silently ignored
  // (arti manages these internally):
  connectionTimeout?: number;
  circuitTimeout?: number;
  circuitBuffer?: number;
  maxCircuitLifetime?: number;
}

export type { TorStorage } from '#wasm';

export interface FetchInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string | Uint8Array | ArrayBuffer;
  signal?: AbortSignal;
}
