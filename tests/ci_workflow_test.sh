#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKFLOW=".github/workflows/release.yml"

echo "=== Testing GitHub Actions Release Workflow ==="

# 1. Verify file exists
test -f "$WORKFLOW"

# 2. Check YAML syntax using ruby's built-in YAML parser
ruby -ryaml -e '
data = YAML.load_file(".github/workflows/release.yml")
raise "Empty workflow" if data.nil?
raise "No jobs in workflow" unless data.key?("jobs")
puts "YAML syntax is valid."
'

# 3. Verify trigger
grep -q "tags:" "$WORKFLOW"
grep -q "v\[0-9\]\+" "$WORKFLOW"

# 4. Verify all 7 matrix targets are defined
TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "x86_64-pc-windows-msvc"
    "aarch64-pc-windows-msvc"
    "wasm32-unknown-unknown"
)

for target in "${TARGETS[@]}"; do
    if ! grep -q "$target" "$WORKFLOW"; then
        echo "Error: target $target not found in $WORKFLOW"
        exit 1
    fi
    echo "  - Found target: $target"
done

# 5. Verify draft release configuration
grep -q "softprops/action-gh-release" "$WORKFLOW"
grep -q "draft: true" "$WORKFLOW"
grep -q "generate_release_notes: true" "$WORKFLOW"

# 6. Verify job dependency for atomic release creation
ruby -ryaml -e '
data = YAML.load_file(".github/workflows/release.yml")
jobs = data["jobs"]
release_job = jobs["create-draft-release"]
raise "No create-draft-release job found" if release_job.nil?
needs = Array(release_job["needs"])
raise "Release job must depend on build-native" unless needs.include?("build-native")
raise "Release job must depend on build-web" unless needs.include?("build-web")
puts "Dependency check passed: draft release requires #{needs.join(", ")}."
'

echo "=== Workflow tests PASSED ==="
