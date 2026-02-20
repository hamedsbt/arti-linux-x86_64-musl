#!/bin/bash
# Push an encrypted WASM binary to the hash-artifacts branch.
#
# Files are AES-256-GCM encrypted using the WASM's SHA256 hash as the key,
# and stored under SHA256(hash) so the filename doesn't reveal the key.
# This prevents accidental exposure of dev builds with sensitive info.
#
# Usage: scripts/tor-js/push-hash-artifact.sh

set -euo pipefail

cd "$(dirname "$0")/../.."

WASM_FILE="crates/tor-js/pkg/tor_js_bg.wasm"

if [ ! -f "$WASM_FILE" ]; then
  echo "Error: $WASM_FILE not found. Run scripts/tor-js/build.sh first."
  exit 1
fi

# Compute hashes and encrypt using Node.js
ENCRYPT_OUTPUT=$(WASM_INPUT="$WASM_FILE" node --input-type=module <<'SCRIPT'
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash, createCipheriv } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const wasm = readFileSync(process.env.WASM_INPUT);

// hash = SHA256(wasm)
const hashBytes = createHash('sha256').update(wasm).digest();
const hash = hashBytes.toString('hex');

// hashHash = SHA256(hash bytes) — used for filename
const hashHash = createHash('sha256').update(hashBytes).digest('hex');

// Encrypt: AES-256-GCM, key = hashBytes, IV = hashBytes[0:12]
const iv = hashBytes.subarray(0, 12);
const cipher = createCipheriv('aes-256-gcm', hashBytes, iv);
const encrypted = Buffer.concat([cipher.update(wasm), cipher.final(), cipher.getAuthTag()]);

// Write encrypted file to temp location
const outPath = join(tmpdir(), hashHash);
writeFileSync(outPath, encrypted);

// Output values for the shell script
console.log(`HASH=${hash}`);
console.log(`HASH_HASH=${hashHash}`);
console.log(`ENCRYPTED_FILE=${outPath}`);
SCRIPT
)

eval "$ENCRYPT_OUTPUT"

PREFIX="${HASH_HASH:0:2}"
DEST="$PREFIX/$HASH_HASH"

echo "WASM hash:      $HASH"
echo "hash(hash):     $HASH_HASH"
echo "Artifact path:  $DEST"

REMOTE=$(git remote get-url origin)
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR" "$ENCRYPTED_FILE"' EXIT

# Clone just the hash-artifacts branch (or create it as an orphan)
if git ls-remote --exit-code --heads "$REMOTE" hash-artifacts >/dev/null 2>&1; then
  git clone --depth 1 --branch hash-artifacts --single-branch "$REMOTE" "$TMPDIR"
else
  echo "Creating new orphan branch hash-artifacts..."
  git init "$TMPDIR"
  git -C "$TMPDIR" remote add origin "$REMOTE"
  git -C "$TMPDIR" checkout --orphan hash-artifacts
fi

# Check if artifact already exists
if [ -f "$TMPDIR/$DEST" ]; then
  echo "Artifact $DEST already exists on hash-artifacts branch. Nothing to do."
  exit 0
fi

# Copy the encrypted file
mkdir -p "$TMPDIR/$PREFIX"
cp "$ENCRYPTED_FILE" "$TMPDIR/$DEST"

# Commit and push
git -C "$TMPDIR" add "$DEST"
git -C "$TMPDIR" commit -m "Add $DEST"
git -C "$TMPDIR" push origin hash-artifacts

echo "Pushed $DEST to hash-artifacts branch."
