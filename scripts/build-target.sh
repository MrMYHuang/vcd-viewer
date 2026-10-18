#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="${1:-}"
VERSION="${2:-}"

if [ -z "$TARGET" ]; then
    echo "Usage: $0 <target> [version]"
    echo "Targets: web, all-local, or Rust target triple (e.g. x86_64-apple-darwin, aarch64-apple-darwin, etc.)"
    exit 1
fi

if [ -z "$VERSION" ]; then
    VERSION=$(awk -F'"' '/^version *=/ {print $2; exit}' Cargo.toml)
fi

echo "==> Building VCD Viewer v${VERSION} for target: ${TARGET}"
mkdir -p dist

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

update_checksum() {
    local archive_name="$1"
    local checksum
    checksum=$(calculate_sha256 "dist/${archive_name}")
    
    # Remove existing entry if present, then append
    if [ -f dist/SHA256SUMS.txt ]; then
        grep -v "${archive_name}$" dist/SHA256SUMS.txt > dist/SHA256SUMS.tmp || true
        mv dist/SHA256SUMS.tmp dist/SHA256SUMS.txt
    fi
    echo "${checksum}  ${archive_name}" >> dist/SHA256SUMS.txt
    echo "==> Checksum recorded for ${archive_name}: ${checksum}"
}

if [ "$TARGET" = "web" ]; then
    echo "--> Ensuring wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

    if ! command -v trunk >/dev/null 2>&1; then
        echo "Error: trunk is not installed. Please install it (e.g., cargo install trunk)."
        exit 1
    fi

    echo "--> Running trunk build --release..."
    trunk build --release

    ARCHIVE="vcd-viewer-v${VERSION}-web.zip"
    echo "--> Packaging ${ARCHIVE}..."
    (
        cd dist
        # Remove any existing archive before zipping
        rm -f "${ARCHIVE}"
        zip -r -q "${ARCHIVE}" . -x "*.tar.gz" "*.zip" "SHA256SUMS*"
    )
    update_checksum "${ARCHIVE}"
    echo "==> Web build complete: dist/${ARCHIVE}"
    exit 0
fi

if [ "$TARGET" = "all-local" ]; then
    HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p')
    echo "--> Building web..."
    "$0" web "${VERSION}"
    echo "--> Building host native (${HOST_TARGET})..."
    "$0" "${HOST_TARGET}" "${VERSION}"
    echo "==> all-local build complete."
    exit 0
fi

# Desktop target build
echo "--> Ensuring Rust target ${TARGET}..."
rustup target add "${TARGET}" >/dev/null 2>&1 || true

echo "--> Compiling with cargo build --release --target ${TARGET}..."
cargo build --release --target "${TARGET}"

BIN_EXT=""
ARCHIVE_EXT="tar.gz"
case "$TARGET" in
    *windows*)
        BIN_EXT=".exe"
        ARCHIVE_EXT="zip"
        ;;
esac

BIN_PATH="target/${TARGET}/release/vcd-viewer${BIN_EXT}"
if [ ! -f "$BIN_PATH" ]; then
    echo "Error: expected binary not found at ${BIN_PATH}"
    exit 1
fi

STAGE_DIR="target/${TARGET}/release/stage_vcd_viewer"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp "$BIN_PATH" "$STAGE_DIR/"
cp README.md "$STAGE_DIR/"
cp LICENSE-MIT "$STAGE_DIR/"
cp LICENSE-APACHE "$STAGE_DIR/"

ARCHIVE="vcd-viewer-v${VERSION}-${TARGET}.${ARCHIVE_EXT}"
echo "--> Packaging dist/${ARCHIVE}..."

if [ "$ARCHIVE_EXT" = "zip" ]; then
    (
        cd "$(dirname "$STAGE_DIR")"
        zip -r -q "${ROOT_DIR}/dist/${ARCHIVE}" "$(basename "$STAGE_DIR")"
    )
else
    tar -czf "dist/${ARCHIVE}" -C "$(dirname "$STAGE_DIR")" "$(basename "$STAGE_DIR")"
fi

rm -rf "$STAGE_DIR"
update_checksum "${ARCHIVE}"
echo "==> Native build complete: dist/${ARCHIVE}"
