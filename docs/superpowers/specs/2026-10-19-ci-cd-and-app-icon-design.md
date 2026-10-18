# CI/CD Release Workflow & App Icon Design

**Date**: 2026-10-19  
**Status**: Draft for Review  
**Target Repository**: `vcd-viewer`  

---

## 1. Intent & Context

`vcd-viewer` is a fast, cross-platform Value Change Dump (VCD) waveform viewer written in Rust using `egui` and `eframe`. Currently, users can compile and run the application locally for native desktop (macOS, Linux, Windows) or web (WebAssembly via Trunk).

To distribute `vcd-viewer` to end users and hardware engineers seamlessly:
1. **Automated CI/CD**: We need a GitHub Actions workflow that automatically builds, packages, and verifies binaries across all supported operating systems (`macOS`, `Linux`, `Windows`, and `Web`) and hardware architectures (`x86_64` and `arm64`), triggered whenever a version tag like `v1.0.0` is pushed.
2. **Draft Release for Human Approval**: The built artifacts must be attached to a **GitHub Release in Draft status** so maintainers can review the release notes, sanity-check the packages, and manually click "Publish".
3. **Application Branding & Icon**: The application currently lacks a visual brand identity. We need a distinctive digital logic waveform icon (cyan and amber traces on dark slate) integrated into both native window titlebars/docks and the web browser favicon.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Automated Multi-Platform Matrix**: Build 7 release targets on every tag push:
  1. `x86_64-unknown-linux-gnu` (Linux 64-bit Intel/AMD)
  2. `aarch64-unknown-linux-gnu` (Linux 64-bit ARM / Raspberry Pi / Graviton)
  3. `x86_64-apple-darwin` (macOS Intel)
  4. `aarch64-apple-darwin` (macOS Apple Silicon M-series)
  5. `x86_64-pc-windows-msvc` (Windows 64-bit Intel/AMD)
  6. `aarch64-pc-windows-msvc` (Windows on ARM)
  7. `wasm32-unknown-unknown` (Web browser bundle compiled via Trunk)
- **Standard Portable Archives**:
  - Linux & macOS: `.tar.gz` containing executable, `README.md`, and license files.
  - Windows: `.zip` containing `vcd-viewer.exe`, `README.md`, and license files.
  - Web: `.zip` containing the full static site `dist/` directory (`index.html`, `.wasm`, `.js`).
- **Integrity Verification**: Generate a single `SHA256SUMS.txt` listing SHA-256 hashes of all 7 published archives.
- **Draft Release Gate**: Use `softprops/action-gh-release@v2` with `draft: true` so no public release is published automatically without human manual trigger.
- **Icon Assets & Integration**:
  - Crisp glowing cyan/amber digital logic waveform traces on a dark slate rounded squircle.
  - Formatted into high-res PNG (`assets/icon-256.png`), Windows ICO (`assets/icon.ico`), and Web Favicon (`assets/favicon.ico` & `assets/favicon.png`).
  - Embedded directly into native `eframe` window options and web `index.html`.

### Non-Goals
- Native OS installers (e.g. `.msi`, `.dmg`, `.deb`) are out of scope for this milestone. Standalone portable archives are preferred.
- Automatic Git tagging from commit messages (releases are strictly driven by manual tag pushes matching `v*.*.*`).
- Auto-publishing without human approval (anti-goal: publishing directly to public without a draft).

### Anti-Goals
- Do not fail the entire release workflow if an optional changelog generation step has warnings.
- Do not publish incomplete release artifacts if any single platform build fails (jobs must coordinate so all artifacts are assembled or the draft status clearly indicates what passed).

---

## 3. App Icon Design & Integration

### 3.1 Visual Theme
- **Motif**: Digital logic square wave traces and bus state transitions.
- **Color Palette**:
  - Background: Dark slate (`#181a20`) rounded squircle with subtle border.
  - High trace: Bright neon cyan (`#00e5ff`).
  - Transition / bus trace: Amber / gold (`#ffb300`).
  - Subtle grid / timing tick accents in muted slate (`#333a46`).
- **Resolution**: 512x512 master SVG/PNG, downscaled to 256x256, 128x128, 64x64, 32x32, 16x16.

### 3.2 File Locations
- Master PNG: `assets/icon.png` (512x512) and `assets/icon-256.png` (256x256).
- Windows Icon: `assets/icon.ico` (multi-res ICO containing 16, 32, 48, 64, 128, 256 px).
- Web Favicon: `assets/favicon.ico` and `assets/favicon.png` (32x32).

### 3.3 Application Wiring
1. **Native Window (`src/main.rs`)**:
   - Load icon bytes via `include_bytes!("../assets/icon-256.png")`.
   - Parse image into `egui::IconData` via `eframe::icon_data::from_png_bytes` (or raw RGBA).
   - Set `viewport.icon = Some(Arc::new(icon))` on `eframe::NativeOptions`.
2. **Web Page (`index.html`)**:
   - Add `<link rel="icon" type="image/png" href="assets/favicon.png">` and `<link rel="icon" type="image/x-icon" href="assets/favicon.ico">`.
   - Configure `Trunk.toml` or `index.html` copy-file directive (`<link data-trunk rel="copy-file" href="assets/favicon.png">`) to copy assets into `dist/`.

---

## 4. GitHub Actions CI/CD Architecture

### 4.1 Workflow Trigger
```yaml
name: Release

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+*'
```

### 4.2 Build Matrix Strategy

| Target Architecture | OS Runner | Tool / Method | Output Archive |
| :--- | :--- | :--- | :--- |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `cargo build --release` with native Linux deps (`libgtk-3-dev`, etc.) | `vcd-viewer-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` | `cross` (`cross-rs/cross`) or `zigbuild` (`cargo-zigbuild`) | `vcd-viewer-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` | `macos-13` (Intel) or `macos-14` cross | `cargo build --release --target x86_64-apple-darwin` | `vcd-viewer-vX.Y.Z-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | `macos-14` (Apple Silicon) | `cargo build --release --target aarch64-apple-darwin` | `vcd-viewer-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `cargo build --release --target x86_64-pc-windows-msvc` | `vcd-viewer-vX.Y.Z-x86_64-pc-windows-msvc.zip` |
| `aarch64-pc-windows-msvc`| `windows-latest` | `cargo build --release --target aarch64-pc-windows-msvc` | `vcd-viewer-vX.Y.Z-aarch64-pc-windows-msvc.zip` |
| `wasm32-unknown-unknown` | `ubuntu-latest` | `trunk build --release` with `wasm32-unknown-unknown` | `vcd-viewer-vX.Y.Z-web.zip` |

### 4.3 Native Dependencies on Linux
`eframe` and `rfd` (file dialogs) require Linux development libraries:
```bash
sudo apt-get update
sudo apt-get install -y libgtk-3-dev libasound2-dev libudev-dev libx11-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```
For `aarch64-unknown-linux-gnu`, cross-compilation can be executed cleanly using `cargo-zigbuild` with `libgtk-3` glibc headers or `cross` with appropriate sysroot packages.

### 4.4 WebAssembly Build
```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked # or download binary release
trunk build --release
cd dist && zip -r ../vcd-viewer-${TAG}-web.zip .
```

### 4.5 Packaging & Checksums Job
A downstream aggregation job `create-release`:
1. Downloads artifacts uploaded by all matrix jobs via `actions/download-artifact@v4`.
2. Computes SHA-256 for all packages:
   ```bash
   sha256sum * > SHA256SUMS.txt
   ```
3. Creates a **Draft Release** using `softprops/action-gh-release@v2`:
   ```yaml
   - name: Create Draft Release
     uses: softprops/action-gh-release@v2
     with:
       draft: true
       generate_release_notes: true
       files: |
         *.tar.gz
         *.zip
         SHA256SUMS.txt
   ```

---

## 5. What is Left to the Builder

- Exact selection of cross-compilation tool for Linux aarch64 (`cargo-zigbuild` vs `cross` vs apt cross-compiler) based on GitHub Actions reliability.
- SVG generation script or generator for the icon files in `assets/`.
- Optimizing GitHub Actions caching (`Swatinem/rust-cache` or `actions/cache`) to keep build times under 10 minutes.

---

## 6. Verification Plan

1. **Local Icon Verification**:
   - Verify `assets/icon-256.png` and `assets/icon.ico` render properly.
   - Run `cargo run --release` and verify the window icon in the dock/taskbar.
   - Run `trunk serve --release` and verify the browser favicon.
2. **Workflow Syntax & Lint Verification**:
   - Validate `.github/workflows/release.yml` with `actionlint` or strict YAML parse.
   - Test artifact archive generation scripts.
3. **End-to-End Simulation**:
   - Push a test tag (or run a `workflow_dispatch` dry-run if configured) to confirm all 7 targets compile, archives are generated, and a draft release appears in GitHub Releases with attached artifacts and `SHA256SUMS.txt`.
