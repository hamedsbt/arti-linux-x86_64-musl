#!/usr/bin/env node

// Stream a large file through Tor, counting text matches in real-time
//
// Demonstrates the streaming Response API: fetch() resolves on headers,
// then body chunks are processed incrementally — nothing is buffered.
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch-streaming.js [url] [pattern]
// Example: examples/tor-fetch-streaming.js https://norvig.com/big.txt the

import { TorClient, Log, storage } from '../crates/tor-js/ts-wrapper/dist/index.js';

async function main() {
  const url = process.argv[2] ?? 'https://www.gutenberg.org/cache/epub/100/pg100.txt';
  const pattern = process.argv[3] ?? 'the';
  const regex = new RegExp(pattern, 'gi');

  console.log(`\nCreating TorClient...\n`);
  const startTime = performance.now();

  const client = new TorClient({
    snowflakeUrl: 'wss://snowflake.pse.dev/',
    fingerprint: '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    log: new Log(),
    storage: new storage.FilesystemStorage(),
  });

  await client.ready();

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

  client.close();
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});