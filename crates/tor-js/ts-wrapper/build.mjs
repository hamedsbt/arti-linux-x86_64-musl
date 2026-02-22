import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(__dirname, '../pkg');
const distDir = resolve(__dirname, 'dist');

// 1. Compute SHA256 of WASM binary
const wasmPath = resolve(pkgDir, 'tor_js_bg.wasm');
const wasmBytes = readFileSync(wasmPath);
const wasmHash = createHash('sha256').update(wasmBytes).digest('hex');
console.log(`WASM SHA256: ${wasmHash}`);

// 2. Read package version
const packageJson = JSON.parse(readFileSync(resolve(__dirname, 'package.json'), 'utf-8'));
const version = packageJson.version;
console.log(`Package version: ${version}`);

// 3. Run tsup with build-time constants
execSync('npx tsup', {
  stdio: 'inherit',
  cwd: __dirname,
  env: {
    ...process.env,
    WASM_SHA256: wasmHash,
    PACKAGE_VERSION: version,
  },
});

// 4. Generate base64 data file
const base64Data = wasmBytes.toString('base64');
writeFileSync(
  resolve(distDir, 'wasm-base64-data.js'),
  `export const wasmBase64 = "${base64Data}";\n`
);
writeFileSync(
  resolve(distDir, 'wasm-base64-data.d.ts'),
  `export declare const wasmBase64: string;\n`
);
console.log(`Generated wasm-base64-data.js (${(base64Data.length / 1024 / 1024).toFixed(1)} MB)`);

// 5. Run tsc to generate declaration maps (.d.ts.map files)
// This overwrites tsup's .d.ts files with versions that include
// //# sourceMappingURL= comments, enabling "jump to definition" in editors.
execSync('npx tsc --emitDeclarationOnly --declarationMap', {
  stdio: 'inherit',
  cwd: __dirname,
});
console.log('Generated declaration maps (.d.ts.map files)');

// 6. Copy WASM runtime files to dist/
const distWasm = resolve(distDir, 'wasm-pkg');
mkdirSync(distWasm, { recursive: true });
copyFileSync(resolve(pkgDir, 'tor_js.js'), resolve(distWasm, 'tor_js.js'));
copyFileSync(resolve(pkgDir, 'tor_js.d.ts'), resolve(distWasm, 'tor_js.d.ts'));
copyFileSync(resolve(pkgDir, 'tor_js_bg.wasm'), resolve(distDir, 'tor_js_bg.wasm'));
console.log('Copied WASM runtime files to dist/');
