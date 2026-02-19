#!/usr/bin/env node

// Make an HTTP request through Tor with persistent filesystem storage
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch.js [url]
// Example: examples/tor-fetch.js https://check.torproject.org/api/ip
//
// State is persisted to ~/.local/state/tor-js/
// Subsequent runs will load cached state for faster bootstrap.

import { readFile, writeFile, unlink, readdir, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

import { TorClient, Log } from '../crates/tor-js/ts-wrapper/dist/index.js';

// ============================================================================
// FilesystemStorage - TorStorage implementation using Node.js fs
// ============================================================================

class FilesystemStorage {
  constructor(baseDir) {
    this.baseDir = baseDir;
  }

  async init() {
    if (!existsSync(this.baseDir)) {
      await mkdir(this.baseDir, { recursive: true });
      console.log(`Created storage directory: ${this.baseDir}`);
    }
  }

  // Encode key to be filesystem-safe
  keyToPath(key) {
    const encoded = encodeURIComponent(key);
    return join(this.baseDir, encoded);
  }

  async get(key) {
    const path = this.keyToPath(key);
    try {
      return await readFile(path, 'utf-8');
    } catch (err) {
      if (err.code === 'ENOENT') return null;
      throw err;
    }
  }

  async set(key, value) {
    const path = this.keyToPath(key);
    await writeFile(path, value, 'utf-8');
  }

  async delete(key) {
    const path = this.keyToPath(key);
    try {
      await unlink(path);
    } catch (err) {
      if (err.code !== 'ENOENT') throw err;
    }
  }

  async keys(prefix) {
    try {
      const files = await readdir(this.baseDir);
      return files
        .map(f => decodeURIComponent(f))
        .filter(k => k.startsWith(prefix));
    } catch (err) {
      if (err.code === 'ENOENT') return [];
      throw err;
    }
  }

  // FIXME: Stub — use a lock file (e.g. proper-lockfile) for real
  // cross-process locking on the filesystem storage directory.
  async tryLock() { return true; }
  async unlock() {}
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';
  const storageDir = join(homedir(), '.local', 'state', 'tor-js');

  // Initialize filesystem storage
  const fsStorage = new FilesystemStorage(storageDir);
  await fsStorage.init();

  console.log(`\nStorage: ${storageDir}`);
  console.log(`Creating TorClient with persistent storage...\n`);

  const startTime = performance.now();

  const client = new TorClient({
    snowflakeUrl: 'wss://snowflake.pse.dev/',
    fingerprint: '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    log: new Log(),
    storage: fsStorage,
  });

  await client.ready();

  const connectTime = ((performance.now() - startTime) / 1000).toFixed(1);
  console.log(`\nConnected in ${connectTime}s, fetching ${url}...\n`);

  // Make fetch request
  const fetchStart = performance.now();
  const response = await client.fetch(url);
  const fetchTime = ((performance.now() - fetchStart) / 1000).toFixed(1);

  // Cleanup
  client.close();

  // Wait just a little bit so that the last log is our output
  await new Promise(resolve => setTimeout(resolve, 50));

  console.log(`\nStatus: ${response.status}`);
  console.log(`Connect time: ${connectTime}s`);
  console.log(`Fetch time: ${fetchTime}s`);
  console.log('Response:');
  console.log(await response.text());
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
