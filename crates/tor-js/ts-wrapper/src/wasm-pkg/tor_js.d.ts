/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

type ReadableStreamType = "bytes";

/**
 * Storage interface for persisting Tor client state.
 *
 * Implement this interface to provide custom storage (IndexedDB, filesystem, etc.).
 * All methods must return Promises.
 *
 * When storage is provided, the Tor client will persist guard selection and other
 * state, allowing faster reconnection across page reloads.
 *
 * @example
 * ```typescript
 * class IndexedDBStorage implements TorStorage {
 *   async get(key: string): Promise<string | null> {
 *     // Load from IndexedDB
 *   }
 *   async set(key: string, value: string): Promise<void> {
 *     // Save to IndexedDB
 *   }
 *   async delete(key: string): Promise<void> {
 *     // Delete from IndexedDB
 *   }
 *   async keys(prefix: string): Promise<string[]> {
 *     // List keys matching prefix
 *   }
 *   // FIXME: Use the Web Locks API for real cross-tab locking:
 *   //   await navigator.locks.request('tor-storage', {ifAvailable: true}, ...)
 *   // These stubs always succeed, which is fine for single-tab use.
 *   async tryLock(): Promise<boolean> {
 *     return true;
 *   }
 *   async unlock(): Promise<void> {
 *   }
 * }
 *
 * const options = new TorClientOptions(url, fingerprint)
 *   .withStorage(new IndexedDBStorage());
 * const client = await new TorClient(options);
 * ```
 */
export interface TorStorage {
    /**
     * Get a value by key.
     * @param key - The storage key
     * @returns The stored value as a string, or null if not found
     */
    get(key: string): Promise<string | null>;

    /**
     * Set a value by key.
     * @param key - The storage key
     * @param value - The value to store (JSON string)
     */
    set(key: string, value: string): Promise<void>;

    /**
     * Delete a value by key.
     * @param key - The storage key
     */
    delete(key: string): Promise<void>;

    /**
     * List all keys with a given prefix.
     * @param prefix - The key prefix to match
     * @returns Array of matching keys
     */
    keys(prefix: string): Promise<string[]>;

    /**
     * Try to acquire an exclusive write lock.
     * @returns true if newly acquired, false if already held.
     * Implement using Web Locks API (browser) or lock files (Node.js).
     */
    tryLock(): Promise<boolean>;

    /**
     * Release the write lock.
     */
    unlock(): Promise<void>;
}

export interface FetchInit {
    method?: string;
    headers?: Record<string, string>;
    body?: string | Uint8Array | ArrayBuffer;
    // TODO: signal?: AbortSignal;
}

export interface TorClient {
    /** Make an HTTP fetch request through Tor. Returns a standard Response. */
    fetch(url: string, init?: FetchInit): Promise<Response>;
    close(): Promise<void>;
}

export interface TorClientOptions {
    /**
     * Set a custom storage implementation for persistent state.
     * If not provided, in-memory storage is used (state lost on page reload).
     */
    withStorage(storage: TorStorage): TorClientOptions;
}



export class IntoUnderlyingByteSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableByteStreamController): Promise<any>;
    start(controller: ReadableByteStreamController): void;
    readonly autoAllocateChunkSize: number;
    readonly type: ReadableStreamType;
}

export class IntoUnderlyingSink {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    abort(reason: any): Promise<any>;
    close(): Promise<any>;
    write(chunk: any): Promise<any>;
}

export class IntoUnderlyingSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableStreamDefaultController): Promise<any>;
}

/**
 * Tor client for making HTTP requests through the Tor network
 */
export class TorClient {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Close the TorClient and release resources
     */
    close(): Promise<any>;
    /**
     * Create a new TorClient with the given options
     *
     * This is an async operation that returns a Promise.
     * The client will bootstrap and establish a connection to the Tor network.
     */
    constructor(options: TorClientOptions);
}

/**
 * Options for creating a TorClient
 */
export class TorClientOptions {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Create options for WebSocket Snowflake transport
     *
     * # Arguments
     * * `snowflake_url` - WebSocket URL for the Snowflake bridge (e.g., "wss://snowflake.pse.dev/")
     * * `fingerprint` - Bridge fingerprint (40 char hex string). Required for verification.
     */
    constructor(snowflake_url: string, fingerprint: string);
    /**
     * Create options for WebRTC Snowflake transport (via broker)
     *
     * # Arguments
     * * `broker_url` - Snowflake broker URL (e.g., "https://snowflake-broker.torproject.net/").
     *   Pass an empty string to use the default Tor Project broker.
     * * `fingerprint` - Bridge fingerprint (40 char hex string). Required for verification.
     */
    static snowflakeWebRtc(broker_url: string, fingerprint: string): TorClientOptions;
    /**
     * Set a custom storage implementation for persistent state.
     *
     * When set, the Tor client will persist guard selection and other state
     * to this storage, allowing faster reconnection across page reloads.
     *
     * If not set, in-memory storage is used (state lost on page reload).
     *
     * # Arguments
     * * `storage` - A JavaScript object implementing the TorStorage interface
     */
    withStorage(storage: TorStorage): TorClientOptions;
}

/**
 * Initialize the tor-js WASM module
 *
 * This must be called before creating any TorClient instances.
 * Sets up panic hooks and logging infrastructure.
 */
export function init(): void;

/**
 * Set a callback function to receive log messages
 *
 * The callback receives three arguments: (level: string, target: string, message: string)
 */
export function setLogCallback(callback: Function): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_torclient_free: (a: number, b: number) => void;
    readonly __wbg_torclientoptions_free: (a: number, b: number) => void;
    readonly init: () => [number, number];
    readonly torclient_close: (a: number) => any;
    readonly torclient_fetch: (a: number, b: number, c: number, d: any) => any;
    readonly torclient_new: (a: number) => any;
    readonly torclientoptions_new: (a: number, b: number, c: number, d: number) => number;
    readonly torclientoptions_snowflakeWebRtc: (a: number, b: number) => number;
    readonly torclientoptions_withStorage: (a: number, b: any) => number;
    readonly setLogCallback: (a: any) => void;
    readonly __wbg_intounderlyingbytesource_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsink_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsource_free: (a: number, b: number) => void;
    readonly intounderlyingbytesource_autoAllocateChunkSize: (a: number) => number;
    readonly intounderlyingbytesource_cancel: (a: number) => void;
    readonly intounderlyingbytesource_pull: (a: number, b: any) => any;
    readonly intounderlyingbytesource_start: (a: number, b: any) => void;
    readonly intounderlyingbytesource_type: (a: number) => number;
    readonly intounderlyingsink_abort: (a: number, b: any) => any;
    readonly intounderlyingsink_close: (a: number) => any;
    readonly intounderlyingsink_write: (a: number, b: any) => any;
    readonly intounderlyingsource_cancel: (a: number) => void;
    readonly intounderlyingsource_pull: (a: number, b: any) => any;
    readonly wasm_bindgen__closure__destroy__h915bf6b7ffb6c7f3: (a: number, b: number) => void;
    readonly wasm_bindgen__closure__destroy__h0222088af2013b64: (a: number, b: number) => void;
    readonly wasm_bindgen__closure__destroy__hd65abcb002037431: (a: number, b: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h360923ea2bfa4551: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h108123f683c1d79d: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h692dfa7102be6671: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hfd4ce4d6641069e0: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
