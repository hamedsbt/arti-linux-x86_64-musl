import {
  ensureWasmInitialized,
  WasmTorClient,
  WasmTorClientOptions,
  addLogListener,
  setListenerLevel,
} from './wasm.js';
import type { TorClientOptions, FetchInit, LogLevel } from './types.js';
import { Log } from './Log.js';
import { createAutoStorage } from './storage/index.js';

export class TorClient {
  private log: Log;
  private clientPromise: Promise<WasmTorClient>;
  private removeLogListener: (() => void) | null = null;
  private wasmCallback: ((level: string, target: string, message: string) => void) | null = null;
  private closed = false;

  constructor(options: TorClientOptions) {
    this.log = (options.log ?? new Log({ rawLog: () => {} }));
    this.clientPromise = this.bootstrap(options);
  }

  private async bootstrap(options: TorClientOptions): Promise<WasmTorClient> {
    await ensureWasmInitialized();

    // Register log listener with per-client level filtering.
    // The WASM subscriber auto-widens to the broadest level across all listeners.
    this.wasmCallback = this.log._makeWasmCallback();
    this.removeLogListener = addLogListener(this.wasmCallback, options.logLevel);

    // Create WASM options
    let wasmOptions: WasmTorClientOptions;
    if (options.mode === 'webrtc') {
      if (!options.brokerUrl) throw new Error('brokerUrl is required for webrtc mode');
      wasmOptions = WasmTorClientOptions.snowflakeWebRtc(options.brokerUrl, options.fingerprint);
    } else {
      wasmOptions = new WasmTorClientOptions(options.snowflakeUrl, options.fingerprint);
    }

    wasmOptions = wasmOptions.withStorage(options.storage ?? createAutoStorage());

    // Create client (WASM constructor returns a Promise)
    this.log.info('Bootstrapping...');
    const client = await WasmTorClient.create(wasmOptions);
    this.log.info('Bootstrap complete');
    return client;
  }

  /**
   * Make an HTTP fetch request through Tor.
   * Returns a standard browser Response object.
   */
  async fetch(url: string, init?: FetchInit): Promise<Response> {
    if (this.closed) throw new Error('TorClient is closed');
    const client = await this.clientPromise;
    this.log.info(`Fetching ${url}`);
    return client.fetch(url, init);
  }

  /**
   * Wait for the Tor client to be ready to make requests (ie finish bootstrapping).
   * (fetch will wait for this automatically)
   */
  async ready(): Promise<void> {
    if (this.closed) throw new Error('TorClient is closed');
    await this.clientPromise;
  }

  /**
   * Change the log level for this client's listener.
   * Also re-syncs the global WASM filter to the broadest level across all clients.
   */
  setLogLevel(level: LogLevel): void {
    if (this.wasmCallback) {
      setListenerLevel(this.wasmCallback, level);
    }
  }

  /**
   * Close the TorClient and release resources.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.removeLogListener?.();
    this.removeLogListener = null;
    this.wasmCallback = null;
    this.clientPromise.then(client => client.close()).catch(() => {});
  }

  [Symbol.dispose](): void {
    this.close();
  }
}
