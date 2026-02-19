import { copyFileSync, mkdirSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(__dirname, '../pkg');

// 1. Run tsup (types resolved via tsconfig paths → ../pkg/)
execSync('npx tsup', { stdio: 'inherit', cwd: __dirname });

// 2. Copy WASM runtime files to dist/
const distDir = resolve(__dirname, 'dist');
const distWasm = resolve(distDir, 'wasm-pkg');
mkdirSync(distWasm, { recursive: true });
copyFileSync(resolve(pkgDir, 'tor_js.js'), resolve(distWasm, 'tor_js.js'));
copyFileSync(resolve(pkgDir, 'tor_js.d.ts'), resolve(distWasm, 'tor_js.d.ts'));
copyFileSync(resolve(pkgDir, 'tor_js_bg.wasm'), resolve(distDir, 'tor_js_bg.wasm'));
console.log('Copied WASM runtime files to dist/');
