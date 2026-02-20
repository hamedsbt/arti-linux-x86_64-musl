import { defineConfig } from 'tsup';

export default defineConfig({
  entry: [
    'src/index.ts',
    'src/wasm-file.ts',
    'src/wasm-cdn.ts',
    'src/wasm-base64.ts',
  ],
  format: ['esm'],
  dts: true,
  sourcemap: true,
  clean: true,
  splitting: false,
  target: 'es2022',
  external: ['#wasm', '#wasm-base64-data', 'node:fs/promises', 'node:os', 'node:path', 'node:url', 'node:crypto'],
  define: {
    '__WASM_SHA256__': JSON.stringify(process.env.WASM_SHA256 || ''),
    '__PACKAGE_VERSION__': JSON.stringify(process.env.PACKAGE_VERSION || ''),
  },
});
