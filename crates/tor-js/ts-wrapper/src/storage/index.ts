export { MemoryStorage } from './memory.js';
export { IndexedDBStorage } from './indexeddb.js';
export { FilesystemStorage } from './filesystem.js';

import type { TorStorage } from '#wasm';
import { IndexedDBStorage } from './indexeddb.js';
import { FilesystemStorage } from './filesystem.js';

export function createAutoStorage(name: string = 'tor-js'): TorStorage {
  if (typeof globalThis !== 'undefined' && typeof globalThis.indexedDB !== 'undefined') {
    return new IndexedDBStorage(name);
  }
  if (typeof process !== 'undefined' && process.versions?.node) {
    return FilesystemStorage.localShare(name);
  }
  throw new Error(
    'No persistent storage available: need IndexedDB (browser) or filesystem (Node.js)',
  );
}