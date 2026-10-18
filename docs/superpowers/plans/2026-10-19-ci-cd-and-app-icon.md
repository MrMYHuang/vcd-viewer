# CI/CD Release Workflow & App Icon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and publish cross-platform release packages (macOS, Linux, Windows, and Web for x86_64 and arm64) to a GitHub Release draft on tag push (`vx.y.z`), provide local build/packaging scripts for all targets, and create and embed a digital logic waveform app icon.

**Architecture:** 
1. Assets: An SVG digital logic waveform master icon (cyan and amber traces on dark slate squircle) compiled into multi-size PNGs, ICO, and web favicons, wired into `src/main.rs` (native viewport icon) and `index.html` (Trunk favicon).
2. Local scripts: A modular suite in `scripts/` (`build-target.sh`, `build-web.sh`, `build-desktop.sh`, `package-release.sh`) that automates compiling, packaging into `.tar.gz`/`.zip`, and computing SHA-256 checksums locally.
3. CI/CD: A GitHub Actions workflow (`.github/workflows/release.yml`) running a 7-target build matrix on tag push, aggregating artifacts, computing `SHA256SUMS.txt`, and drafting a release via `softprops/action-gh-release@v2`.

**Tech Stack:** Rust, `egui`/`eframe`, `image` crate, WebAssembly (`wasm32-unknown-unknown`), Trunk, GitHub Actions, Bash, Python / SVG rendering tools.

**Spec:** [docs/superpowers/specs/2026-10-19-ci-cd-and-app-icon-design.md](file:///Users/myh/Documents/vcd-viewer/docs/superpowers/specs/2026-10-19-ci-cd-and-app-icon-design.md)

## Global Constraints

- Releases are triggered strictly by git tag pushes matching `v[0-9]+.[0-9]+.[0-9]+*` (e.g. `v1.0.0`).
- Builds must target all 7 architectures: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`, and `wasm32-unknown-unknown`.
- Archives: Linux and macOS produce `.tar.gz`; Windows produces `.zip`; Web produces `.zip` containing static `dist/`.
- The GitHub Release must be created in `draft: true` status for manual maintainer approval.
- App icon visual theme: Crisp neon cyan (`#00e5ff`) and amber (`#ffb300`) square waves on dark slate (`#181a20`) rounded squircle.
- License files (`LICENSE-MIT`, `LICENSE-APACHE`) and `README.md` must be packaged in every desktop archive.

## Review Focus

1. **Linux headless dependency failure in CI**: `eframe` and `rfd` require GTK3 and X11/xcb libraries on Linux; `ubuntu-latest` runner must install `libgtk-3-dev`, `libasound2-dev`, `libudev-dev`, `libx11-dev`, `libxkbcommon-dev`, and xcb packages or `cargo build` fails with linker errors.
2. **Apple Silicon cross-compilation target**: Cross-compiling for `x86_64-apple-darwin` from Apple Silicon (or vice versa) requires the Rust std target installed via `rustup target add <target>`.
3. **Windows AArch64 MSVC compilation**: Compiling `aarch64-pc-windows-msvc` on `windows-latest` must not attempt to execute host tests for the foreign architecture.
4. **Draft Release upload synchronization**: The `create-release` aggregation job must depend on `needs: [build-native, build-web]` so it runs only if all matrix builds succeed, avoiding partial or broken release drafts.
5. **App icon RGBA byte alignment**: Parsing PNG icon bytes in `src/main.rs` must handle error cases gracefully with `Option::None` fallback rather than panicking on startup.

---

### Task 1: Generate App Icon Assets and Wire into Native & Web App

**Files:**
- Create: `assets/icon.svg`
- Create: `assets/icon.png` (512x512)
- Create: `assets/icon-256.png` (256x256)
- Create: `assets/favicon.png` (32x32)
- Create: `assets/favicon.ico`
- Create: `assets/icon.ico`
- Create: `scripts/generate-icons.py`
- Modify: `Cargo.toml` (add `image` dependency with `png` feature)
- Modify: `src/main.rs` (load and configure `viewport.icon`)
- Modify: `index.html` (add favicon links and Trunk copy-file directives)
- Create: `tests/icon_test.rs`

**Interfaces:**
- Consumes: `eframe::NativeOptions`, `egui::ViewportBuilder::with_icon`, `egui::IconData`
- Produces: `assets/` icon bundle and window/web branding

- [ ] **Step 1: Write the failing test for icon assets and loader**

```rust
// tests/icon_test.rs
use std::path::Path;

#[test]
fn test_icon_assets_exist_and_valid() {
    assert!(Path::new("assets/icon-256.png").exists(), "icon-256.png missing");
    assert!(Path::new("assets/favicon.png").exists(), "favicon.png missing");
    assert!(Path::new("assets/icon.ico").exists(), "icon.ico missing");
    assert!(Path::new("assets/favicon.ico").exists(), "favicon.ico missing");

    let png_bytes = std::fs::read("assets/icon-256.png").expect("read icon-256.png");
    let img = image::load_from_memory(&png_bytes).expect("decode icon-256.png");
    assert_eq!(img.width(), 256);
    assert_eq!(img.height(), 256);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test icon_test`
Expected: FAIL (missing dependency `image` or missing files in `assets/`)

- [ ] **Step 3: Add `image` dependency to `Cargo.toml`**

Add to `Cargo.toml` under `[dependencies]`:
```toml
image = { version = "0.25", default-features = false, features = ["png"] }
```

- [ ] **Step 4: Create icon generation script `scripts/generate-icons.py` and generate assets**

Write `scripts/generate-icons.py` to create `assets/icon.svg` featuring cyan (`#00e5ff`) and amber (`#ffb300`) square waves with timing grid on dark slate (`#181a20`) rounded squircle, and rasterize to:
- `assets/icon.png` (512x512)
- `assets/icon-256.png` (256x256)
- `assets/favicon.png` (32x32)
- `assets/icon.ico` and `assets/favicon.ico`
Run: `python3 scripts/generate-icons.py`

- [ ] **Step 5: Wire icon into `src/main.rs` and `index.html`**

In `src/main.rs`:
```rust
#[cfg(not(target_arch = "wasm32"))]
fn load_app_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let icon_bytes = include_bytes!("../assets/icon-256.png");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(std::sync::Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    } else {
        None
    }
}
```
Attach to `egui::ViewportBuilder::default().with_icon(icon)` in `main()`.

In `index.html`:
```html
<link data-trunk rel="copy-file" href="assets/favicon.png"/>
<link data-trunk rel="copy-file" href="assets/favicon.ico"/>
<link rel="icon" type="image/png" href="favicon.png"/>
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --test icon_test`
Expected: PASS

- [ ] **Step 7: Verify native compile and web trunk build**

Run: `cargo check`
Run: `trunk build --release`
Expected: Both pass with 0 errors.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/main.rs index.html assets/ scripts/generate-icons.py tests/icon_test.rs
git commit -m "feat: add digital logic waveform app icons and integrate into desktop and web"
```

---

### Task 2: Create Local Build & Packaging Scripts

**Files:**
- Create: `scripts/build-target.sh`
- Create: `scripts/build-web.sh`
- Create: `scripts/build-desktop.sh`
- Create: `scripts/package-release.sh`
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Test: `tests/scripts_test.sh`

**Interfaces:**
- Consumes: `Cargo.toml` version, `cargo build`, `trunk build`, `zip`, `tar`
- Produces: `dist/vcd-viewer-v<version>-<target>.<ext>`, `dist/SHA256SUMS.txt`

- [ ] **Step 1: Create license files matching `Cargo.toml`**

Create `LICENSE-MIT` and `LICENSE-APACHE` in the project root so packaging includes full legal text.

- [ ] **Step 2: Implement `scripts/build-target.sh`**

Implement `scripts/build-target.sh`:
- Accepts arguments: `./scripts/build-target.sh <target-name> [version]`
  - Supports: `web`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`, `all-local`, and host auto-detection.
- Auto-extracts version from `Cargo.toml` if omitted.
- Runs `rustup target add <target>` if target is missing.
- For `web`: runs `trunk build --release`, zips `dist/` into `dist/vcd-viewer-v${VERSION}-web.zip`.
- For desktop: runs `cargo build --release --target <target>`, creates staging directory with binary, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
- Packs into `.tar.gz` (macOS/Linux) or `.zip` (Windows).
- Updates `dist/SHA256SUMS.txt`.
- Make executable: `chmod +x scripts/build-target.sh`

- [ ] **Step 3: Implement convenience helper scripts**

- `scripts/build-web.sh`: Calls `./scripts/build-target.sh web "$@"`
- `scripts/build-desktop.sh`: Determines host target via `rustc -vV | sed -n 's|host: ||p'` and calls `./scripts/build-target.sh <host> "$@"`
- `scripts/package-release.sh`: Iterates over all `.tar.gz` and `.zip` archives in `dist/` and updates `dist/SHA256SUMS.txt` with formatted hashes.
- Make all executable: `chmod +x scripts/*.sh`

- [ ] **Step 4: Create automated verification script `tests/scripts_test.sh`**

Write a bash test that executes:
```bash
./scripts/build-target.sh web
./scripts/build-desktop.sh
```
and verifies that:
1. `dist/vcd-viewer-*-web.zip` exists and contains `index.html`.
2. `dist/vcd-viewer-*$(rustc -vV | sed -n 's|host: ||p')*` archive exists and contains the binary.
3. `dist/SHA256SUMS.txt` exists and matches `shasum -a 256` or `sha256sum`.

- [ ] **Step 5: Run tests and verify scripts pass**

Run: `bash tests/scripts_test.sh`
Expected: PASS with 0 exit code.

- [ ] **Step 6: Commit**

```bash
git add LICENSE-MIT LICENSE-APACHE scripts/ tests/scripts_test.sh
git commit -m "feat: add local target build, packaging, and checksum verification scripts"
```

---

### Task 3: Implement GitHub Actions Multi-Platform Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `tests/ci_workflow_test.sh`

**Interfaces:**
- Consumes: Git tag `v*.*.*`, `actions/checkout@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4`, `softprops/action-gh-release@v2`
- Produces: GitHub Draft Release with 7 platform archives and `SHA256SUMS.txt`

- [ ] **Step 1: Write `.github/workflows/release.yml`**

Create `.github/workflows/release.yml` with:
1. `on.push.tags: ['v[0-9]+.[0-9]+.[0-9]+*']` and `on.workflow_dispatch` for manual testing.
2. `permissions: contents: write`.
3. Job `build-native`:
   - Matrix:
     - `{ os: ubuntu-latest, target: x86_64-unknown-linux-gnu, archive: tar.gz, cross: false }`
     - `{ os: ubuntu-latest, target: aarch64-unknown-linux-gnu, archive: tar.gz, cross: true }`
     - `{ os: macos-13, target: x86_64-apple-darwin, archive: tar.gz, cross: false }`
     - `{ os: macos-14, target: aarch64-apple-darwin, archive: tar.gz, cross: false }`
     - `{ os: windows-latest, target: x86_64-pc-windows-msvc, archive: zip, cross: false }`
     - `{ os: windows-latest, target: aarch64-pc-windows-msvc, archive: zip, cross: false }`
   - Steps:
     - Checkout code.
     - Install Rust toolchain with target.
     - Linux system dependencies: `sudo apt-get install -y libgtk-3-dev libasound2-dev libudev-dev libx11-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`.
     - For Linux AArch64: install and use `cargo-zigbuild` or `cross` (`cross-rs/cross`).
     - Build release binary.
     - Package into `vcd-viewer-${TAG}-${TARGET}.${ARCHIVE}` with `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
     - Upload artifact using `actions/upload-artifact@v4`.
4. Job `build-web`:
   - Runs on `ubuntu-latest`.
   - Adds `wasm32-unknown-unknown`.
   - Downloads/installs `trunk`.
   - Runs `trunk build --release`.
   - Packages `dist/` into `vcd-viewer-${TAG}-web.zip`.
   - Uploads artifact.
5. Job `create-draft-release`:
   - `needs: [build-native, build-web]`
   - Runs on `ubuntu-latest`.
   - Downloads all artifacts to staging dir.
   - Computes `sha256sum * > SHA256SUMS.txt`.
   - Uses `softprops/action-gh-release@v2` with `draft: true`, `generate_release_notes: true`, attaching `*.tar.gz`, `*.zip`, and `SHA256SUMS.txt`.

- [ ] **Step 2: Write workflow validation test `tests/ci_workflow_test.sh`**

Implement a test script that:
1. Validates YAML syntax of `.github/workflows/release.yml`.
2. Verifies presence of all 7 target matrices (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`, `wasm32-unknown-unknown`).
3. Verifies `draft: true` and `generate_release_notes: true` are configured on `action-gh-release`.
4. Verifies `needs: [build-native, build-web]` dependency to ensure atomic draft creation.

- [ ] **Step 3: Run workflow validation test**

Run: `bash tests/ci_workflow_test.sh`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml tests/ci_workflow_test.sh
git commit -m "ci: add multi-platform release workflow with draft release gate for tag pushes"
```

---

### Task 4: Documentation and Final Verification

**Files:**
- Modify: `README.md` (Add Release & CI/CD section with local script usage)
- Test: Full verification suite (`cargo test`, `trunk build --release`, `bash tests/scripts_test.sh`, `bash tests/ci_workflow_test.sh`)

- [ ] **Step 1: Update `README.md`**

Add a **Building & Releasing** section to `README.md` detailing:
- The local build scripts (`./scripts/build-target.sh <target>`).
- The automated GitHub Actions release workflow on tag push (`git tag v1.0.0 && git push origin v1.0.0`).
- The 7 supported architecture targets and manual draft release approval process.

- [ ] **Step 2: Run complete test suite**

Run:
```bash
cargo test
trunk build --release
bash tests/scripts_test.sh
bash tests/ci_workflow_test.sh
```
Expected: All tests pass with 0 errors.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document release workflow and local target build scripts in README"
```
