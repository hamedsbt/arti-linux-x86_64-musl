#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")"
GIT_ROOT=$(git rev-parse --show-toplevel)

if [ ! -d "$GIT_ROOT/crates/tor-js/ts-wrapper/dist" ]; then
    echo "Error: ts-wrapper dist not found at $GIT_ROOT/crates/tor-js/ts-wrapper/dist"
    echo "Run: scripts/tor-js/build.sh"
    exit 1
fi

rm -rf dist
cp -a "$GIT_ROOT"/crates/tor-js/ts-wrapper/dist dist
echo "*" >dist/.gitignore

python3 -m http.server 8000
