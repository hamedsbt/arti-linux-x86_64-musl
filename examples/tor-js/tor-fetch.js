#!/usr/bin/env node

// Make an HTTP request through Tor with persistent filesystem storage
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-js/tor-fetch.js [url]
// Example: examples/tor-js/tor-fetch.js https://check.torproject.org/api/ip
//
// State is persisted to ~/.local/share/tor-js/
// Subsequent runs will load cached state for faster bootstrap.

import { TorClient, Log } from '../../crates/tor-js/ts-wrapper/dist/entryPoints/wasm-base64/index.js';

async function main() {
  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  const log = new Log();

  log.info();
  log.info(`Creating TorClient with persistent storage...`);

  const startTime = performance.now();

  const client = new TorClient({
    snowflakeUrl: 'wss://snowflake.pse.dev/',
    fingerprint: '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    log,
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

  // This does not block exit (see .unref), but logs if nodejs stays alive
  // without exiting gracefully. This suggests resource use (presumably in arti)
  // that persists beyond .close().
  await shutdown();
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});

async function shutdown() {
  await softDelay(2000);
  console.log('\nWaiting up to 10s for graceful exit.');
  await softDelay(8000);
  console.warn('WARN: Graceful exit did not occur. Ignoring.\n')
  process.exit(0);
}

async function softDelay(ms) {
  return new Promise(resolve => {
    setTimeout(resolve, ms).unref();
  });
}
