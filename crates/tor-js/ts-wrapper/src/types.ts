import type { Log } from './Log.js';
import type { TorStorage } from '#wasm';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface TorClientOptions {
  /** Snowflake WebSocket bridge URL (e.g., "wss://snowflake.pse.dev/") */
  snowflakeUrl: string;

  /** Bridge fingerprint for verification (40-char hex string). Required. */
  fingerprint: string;

  /**
   * Transport mode: 'websocket' (default) or 'webrtc'.
   * When 'webrtc', snowflakeUrl is ignored and the default broker is used.
   */
  mode?: 'websocket' | 'webrtc';

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
