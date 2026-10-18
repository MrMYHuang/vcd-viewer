#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "=== Testing Local Target Build & Packaging Scripts ==="

# Clean test output dist directory
rm -rf dist/vcd-viewer-*.tar.gz dist/vcd-viewer-*.zip dist/SHA256SUMS.txt

# 1. Check scripts exist and are executable
test -x scripts/build-target.sh
test -x scripts/build-web.sh
test -x scripts/build-desktop.sh
test -x scripts/package-release.sh

# 2. Check License files exist
test -f LICENSE-MIT
test -f LICENSE-APACHE

# 3. Test build-web.sh
./scripts/build-web.sh

web_zips=(dist/vcd-viewer-*-web.zip)
WEB_ZIP="${web_zips[0]}"
test -f "$WEB_ZIP"

# Verify web zip contents
unzip -l "$WEB_ZIP" | grep "index.html" >/dev/null
unzip -l "$WEB_ZIP" | grep "favicon.png" >/dev/null

# 4. Test build-desktop.sh
./scripts/build-desktop.sh

HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p')
desktop_archives=(dist/vcd-viewer-*${HOST_TARGET}*)
DESKTOP_ARCHIVE="${desktop_archives[0]}"
test -f "$DESKTOP_ARCHIVE"

# 5. Test package-release.sh
./scripts/package-release.sh

test -f dist/SHA256SUMS.txt
grep "web.zip" dist/SHA256SUMS.txt >/dev/null
grep "${HOST_TARGET}" dist/SHA256SUMS.txt >/dev/null

# Verify checksums match
if command -v shasum >/dev/null 2>&1; then
    (cd dist && shasum -a 256 -c SHA256SUMS.txt)
elif command -v sha256sum >/dev/null 2>&1; then
    (cd dist && sha256sum -c SHA256SUMS.txt)
fi

echo "=== All script tests PASSED ==="
