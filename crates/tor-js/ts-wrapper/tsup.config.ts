import { defineConfig } from 'tsup';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(__dirname, '../pkg');

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
  clean: false, // build.mjs handles cleaning
  splitting: false,
  target: 'es2022',
  external: ['node:fs/promises', 'node:os', 'node:path', 'node:url', 'node:crypto'],
  esbuildPlugins: [{
    name: 'resolve-for-bundling',
    setup(build) {
      build.onResolve({ filter: /^#wasm$/ }, () => ({
        path: resolve(pkgDir, 'tor_js.js'),
      }));
      build.onResolve({ filter: /^#wasm-base64-data$/ }, () => ({
        path: resolve(__dirname, 'src', '_wasm-base64-data.generated.js'),
      }));
    },
  }],
  define: {
    '__WASM_SHA256__': JSON.stringify(process.env.WASM_SHA256 || ''),
    '__PACKAGE_VERSION__': JSON.stringify(process.env.PACKAGE_VERSION || ''),
  },
});