import initWasm, {
  init as wasmInit,
  setLogCallback as wasmSetLogCallback,
  TorClient as WasmTorClient,
  TorClientOptions as WasmTorClientOptions,
} from '#wasm';

export { WasmTorClient, WasmTorClientOptions, wasmSetLogCallback };

type WasmSourceProvider = () => Promise<BufferSource | Uint8Array>;

let initPromise: Promise<void> | null = null;
let customWasmUrl: string | URL | undefined;
let wasmSourceProvider: WasmSourceProvider | undefined;
let configuredLogLevel: string | undefined;

/**
 * Override the WASM binary URL. Must be called before any TorClient is created.
 */
export function setWasmUrl(url: string | URL): void {
  if (initPromise) {
    throw new Error('setWasmUrl() must be called before any TorClient is created');
  }
  customWasmUrl = url;
}

/**
 * Set a custom WASM source provider. Called by entry points to configure
 * how the WASM binary is loaded (e.g. base64 decode, CDN fetch).
 * Must be called before any TorClient is created.
 */
export function setWasmSourceProvider(provider: WasmSourceProvider): void {
  if (initPromise) {
    throw new Error('setWasmSourceProvider() must be called before any TorClient is created');
  }
  wasmSourceProvider = provider;
}

/**
 * Ensures the WASM module is loaded and initialized. Idempotent.
 * The logLevel from the first call wins (subsequent calls are no-ops).
 */
export async function ensureWasmInitialized(logLevel?: string): Promise<void> {
  if (initPromise) return initPromise;
  configuredLogLevel = logLevel;
  initPromise = doInit();
  return initPromise;
}

async function doInit(): Promise<void> {
  if (customWasmUrl) {
    await initWasm({ module_or_path: customWasmUrl });
  } else if (wasmSourceProvider) {
    await initWasm({ module_or_path: await wasmSourceProvider() });
  } else if (typeof process !== 'undefined' && process.versions?.node) {
    // Node.js: read WASM binary from filesystem
    const { readFile } = await import('node:fs/promises');
    const { fileURLToPath } = await import('node:url');
    const wasmPath = fileURLToPath(new URL('./tor_js_bg.wasm', import.meta.url));
    const bytes = await readFile(wasmPath);
    await initWasm({ module_or_path: bytes });
  } else {
    // Browser: use URL relative to this module
    await initWasm({ module_or_path: new URL('./tor_js_bg.wasm', import.meta.url) });
  }
  wasmInit(configuredLogLevel);
}
