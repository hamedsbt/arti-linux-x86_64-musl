#!/usr/bin/env node

// Stream a large file through Tor, counting text matches in real-time
//
// Demonstrates the streaming Response API: fetch() resolves on headers,
// then body chunks are processed incrementally — nothing is buffered.
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch-streaming.js [url] [pattern]
// Example: examples/tor-fetch-streaming.js https://norvig.com/big.txt the

import { readFile, writeFile, unlink, readdir, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

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
  const { TorClient, TorClientOptions, init } = await setup();

  const url = process.argv[2] ?? 'https://www.gutenberg.org/cache/epub/100/pg100.txt';
  const pattern = process.argv[3] ?? 'the';
  const regex = new RegExp(pattern, 'gi');

  const storageDir = join(homedir(), '.local', 'state', 'tor-js');

  // Initialize filesystem storage
  const storage = new FilesystemStorage(storageDir);
  await storage.init();

  console.log(`\nCreating TorClient...\n`);
  const startTime = performance.now();

  const options = new TorClientOptions(
    'wss://snowflake.pse.dev/',
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'
  ).withStorage(storage);

  const client = await TorClient.create(options);
  const connectTime = ((performance.now() - startTime) / 1000).toFixed(1);
  console.log(`\nConnected in ${connectTime}s`);
  console.log(`Streaming ${url}`);
  console.log(`Counting case-insensitive matches of /${pattern}/\n`);

  const fetchStart = performance.now();
  const response = await client.fetch(url);

  const headersTime = ((performance.now() - fetchStart) / 1000).toFixed(1);
  const contentLength = response.headers.get('content-length');
  console.log(`Headers received in ${headersTime}s (status ${response.status})`);
  if (contentLength) {
    console.log(`Content-Length: ${(contentLength / 1024 / 1024).toFixed(1)} MB`);
  }
  console.log('');

  // Stream body chunk-by-chunk, counting matches without buffering
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let totalBytes = 0;
  let matches = 0;
  let chunks = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    chunks++;
    totalBytes += value.byteLength;
    const text = decoder.decode(value, { stream: true });
    matches += (text.match(regex) || []).length;

    const mb = (totalBytes / 1024 / 1024).toFixed(2);
    const pct = contentLength ? ` (${(totalBytes / contentLength * 100).toFixed(0)}%)` : '';
    process.stdout.write(`\r  ${mb} MB${pct} | ${matches} matches | ${chunks} chunks`);
  }

  const totalTime = ((performance.now() - fetchStart) / 1000).toFixed(1);
  const throughput = (totalBytes / 1024 / ((performance.now() - fetchStart) / 1000)).toFixed(1);

  console.log('\n');
  console.log(`Done in ${totalTime}s`);
  console.log(`  ${(totalBytes / 1024 / 1024).toFixed(2)} MB in ${chunks} chunks`);
  console.log(`  ${matches} matches of /${pattern}/`);
  console.log(`  ${throughput} KB/s throughput`);

  await client.close();
}

async function setup() {
  console.log('Loading WASM module...');

  let initWasm, init, TorClient, TorClientOptions;
  try {
    const module = await import('../crates/tor-js/pkg/tor_js.js');
    initWasm = module.default;
    init = module.init;
    TorClient = module.TorClient;
    TorClientOptions = module.TorClientOptions;
  } catch (err) {
    throw new Error(
      'Failed to import tor-js. Run: scripts/tor-js/build.sh',
      { cause: err },
    );
  }

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const wasmPath = join(__dirname, '../crates/tor-js/pkg/tor_js_bg.wasm');
  const wasmBytes = await readFile(wasmPath);
  await initWasm(wasmBytes);

  init();

  return { TorClient, TorClientOptions, init };
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
