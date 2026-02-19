#!/usr/bin/env node

// Make an HTTP request through Tor with persistent filesystem storage
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch.js [url]
// Example: examples/tor-fetch.js https://check.torproject.org/api/ip
//
// State is persisted to ~/.local/share/tor-js/
// Subsequent runs will load cached state for faster bootstrap.

import { TorClient, Log, storage } from '../crates/tor-js/ts-wrapper/dist/index.js';

async function main() {
  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  console.log(`\nCreating TorClient with persistent storage...\n`);

  const startTime = performance.now();

  const client = new TorClient({
    snowflakeUrl: 'wss://snowflake.pse.dev/',
    fingerprint: '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    log: new Log(),
    storage: new storage.FilesystemStorage(),
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