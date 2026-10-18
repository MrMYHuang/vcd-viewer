#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p')
"$ROOT_DIR/scripts/build-target.sh" "$HOST_TARGET" "$@"
