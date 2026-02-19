#!/bin/bash
# Build tor-js WASM package and TypeScript wrapper
#
# Usage: scripts/tor-js/build.sh [--target web|nodejs|bundler] [--release]
#
# Targets:
#   web      - ES modules for browsers and modern runtimes (default)
#   nodejs   - CommonJS for Node.js
#   bundler  - ES modules for bundlers (webpack, etc.)

set -e

cd "$(dirname "$0")/../.."

TARGET="web"
PROFILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --release)
            PROFILE="--release"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "Building tor-js WASM for target '$TARGET'..."
wasm-pack build crates/tor-js --target "$TARGET" $PROFILE

# Copy README to pkg
cp crates/tor-js/README.md crates/tor-js/pkg/

echo "WASM package available at: crates/tor-js/pkg/"

# Build TypeScript wrapper
echo ""
echo "Building TypeScript wrapper..."
cd crates/tor-js/ts-wrapper
npm install --silent
npm run build

echo ""
echo "Done! ts-wrapper available at: crates/tor-js/ts-wrapper/dist/"
