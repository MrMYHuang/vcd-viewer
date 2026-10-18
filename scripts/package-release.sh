#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DIST_DIR="dist"
mkdir -p "$DIST_DIR"

calculate_sha256() {
    local file="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    else
        openssl dgst -sha256 "$file" | awk '{print $NF}'
    fi
}

echo "==> Regenerating ${DIST_DIR}/SHA256SUMS.txt for all release packages..."
SUMS_FILE="${DIST_DIR}/SHA256SUMS.txt"
TMP_FILE="${DIST_DIR}/SHA256SUMS.tmp"
rm -f "$TMP_FILE"

COUNT=0
for archive in "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.zip; do
    if [ -f "$archive" ]; then
        BASE_NAME=$(basename "$archive")
        CHECKSUM=$(calculate_sha256 "$archive")
        echo "${CHECKSUM}  ${BASE_NAME}" >> "$TMP_FILE"
        COUNT=$((COUNT + 1))
        echo "  - ${BASE_NAME}: ${CHECKSUM}"
    fi
done

if [ -f "$TMP_FILE" ]; then
    sort -k2 "$TMP_FILE" > "$SUMS_FILE"
    rm -f "$TMP_FILE"
    echo "==> Updated ${SUMS_FILE} (${COUNT} archives)."
else
    echo "Warning: No .tar.gz or .zip archives found in ${DIST_DIR}."
fi
