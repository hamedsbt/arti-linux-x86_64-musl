import initWasm, {
  init as wasmInit,
  setLogCallback as wasmSetLogCallback,
  TorClient as WasmTorClient,
  TorClientOptions as WasmTorClientOptions,
} from '#wasm';

export { WasmTorClient, WasmTorClientOptions, wasmSetLogCallback };

let initPromise: Promise<void> | null = null;
let customWasmUrl: string | URL | undefined;

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
 * Ensures the WASM module is loaded and initialized. Idempotent.
 */
export async function ensureWasmInitialized(): Promise<void> {
  if (initPromise) return initPromise;
  initPromise = doInit();
  return initPromise;
}

async function doInit(): Promise<void> {
  if (customWasmUrl) {
    await initWasm(customWasmUrl);
  } else if (typeof process !== 'undefined' && process.versions?.node) {
    // Node.js: read WASM binary from filesystem
    const { readFile } = await import('node:fs/promises');
    const { fileURLToPath } = await import('node:url');
    const wasmPath = fileURLToPath(new URL('./tor_js_bg.wasm', import.meta.url));
    const bytes = await readFile(wasmPath);
    await initWasm(bytes);
  } else {
    // Browser: use URL relative to this module
    await initWasm(new URL('./tor_js_bg.wasm', import.meta.url));
  }
  wasmInit();
}
