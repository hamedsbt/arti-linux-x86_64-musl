#!/usr/bin/env node

// Make an HTTP request through Tor from Node.js using arti-client via tor-js
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch-inmemory.js [url]
// Example: examples/tor-fetch-inmemory.js https://check.torproject.org/api/ip
//
// Uses in-memory storage (state is lost when the process exits).
// For persistent storage, see tor-fetch.js.

import { TorClient, Log, storage } from '../crates/tor-js/ts-wrapper/dist/wasm-base64.js';

async function main() {
  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  const log = new Log();

  log.info();
  log.info(`Creating TorClient (in-memory storage)...`);

  const startTime = performance.now();

  const client = new TorClient({
    snowflakeUrl: 'wss://snowflake.pse.dev/',
    fingerprint: '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    log,
    storage: new storage.MemoryStorage(),
  });

  await client.ready();

  const connectTime = ((performance.now() - startTime) / 1000).toFixed(1);
  log.info();
  log.info(`Connected in ${connectTime}s, fetching ${url}...`);

  // Make fetch request
  const fetchStart = performance.now();
  const response = await client.fetch(url);
  const responseText = await response.text();
  const fetchTime = ((performance.now() - fetchStart) / 1000).toFixed(1);

  // Cleanup
  client.close();

  // Wait just a little bit so that the last log is our output
  await new Promise(resolve => setTimeout(resolve, 50));

  log.info();
  log.info(`Status: ${response.status}`);
  log.info(`Connect time: ${connectTime}s`);
  log.info(`Fetch time: ${fetchTime}s`);
  log.info('Response:');
  log.info(responseText);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});