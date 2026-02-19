import type { TorStorage } from '#wasm';

export class MemoryStorage implements TorStorage {
  private data = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.data.get(key) ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    this.data.set(key, value);
  }

  async delete(key: string): Promise<void> {
    this.data.delete(key);
  }

  async keys(prefix: string): Promise<string[]> {
    return [...this.data.keys()].filter(k => k.startsWith(prefix)).sort();
  }

  async tryLock(): Promise<boolean> {
    return true;
  }

  async unlock(): Promise<void> {}
}
