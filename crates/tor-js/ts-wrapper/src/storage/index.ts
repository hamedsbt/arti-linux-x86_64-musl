export { MemoryStorage } from './memory.js';
export { IndexedDBStorage } from './indexeddb.js';

import type { TorStorage } from '#wasm';
import { IndexedDBStorage } from './indexeddb.js';
import { MemoryStorage } from './memory.js';

export function createAutoStorage(name: string = 'tor-js'): TorStorage {
  if (typeof globalThis !== 'undefined' && typeof globalThis.indexedDB !== 'undefined') {
    return new IndexedDBStorage(name);
  }
  return new MemoryStorage();
}
