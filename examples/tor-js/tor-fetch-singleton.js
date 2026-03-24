#!/usr/bin/env node

// Make an HTTP request through Tor using the singleton API
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-js/tor-fetch-singleton.js [url]
// Example: examples/tor-js/tor-fetch-singleton.js https://check.torproject.org/api/ip

import { tor, Log } from '../../crates/tor-js/ts-wrapper/dist/entryPoints/wasm-base64/singleton.js';

const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

console.log(`Fetching ${url} via Tor...`);

tor.configure({
  gateway: 'https://tor-js-gateway.voltrevo.com',
  // log: new Log(),
});

const response = await tor.fetch(url);
const text = await response.text();

console.log(`Status: ${response.status}`);
console.log(text);

tor.close();
