import type { Log } from './Log.js';
import type { TorStorage } from '#wasm';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface TorClientOptions {
  /** Snowflake WebSocket bridge URL (e.g., "wss://snowflake.pse.dev/"). Used in 'websocket' mode. */
  snowflakeUrl: string;

  /** Bridge fingerprint for verification (40-char hex string), or 'not-pinned' to skip verification. */
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

  /** Optional logger instance */
  log?: Log;

  /** Optional storage for persistent state (implements TorStorage). */
  storage?: TorStorage;

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
}
