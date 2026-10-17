# Cross-Platform & Web VCD Waveform Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a permissively licensed (`MIT OR Apache-2.0`) Rust VCD waveform viewer supporting Windows, macOS, Linux, and Web (WASM) capable of streaming 10 GB SoC VCD traces with on-demand signal loading, IEEE 754 floating-point formatting, and interactive waveform debugging.

**Architecture:** Single Rust crate (`vcd-viewer`) with a clean separation between a UI-independent core library (`src/lib.rs`: chunked byte source, two-phase VCD header/body parser, columnar waveform database with LOD pyramid, number/IEEE-754 formatter, and workspace state/persistence) and an `eframe`/`egui` GUI layer (`src/ui/`). Desktop reads chunks via `std::fs::File` seek; Web reads chunks via `Blob.slice` (`rfd::FileHandle`), keeping memory bounded on 10 GB traces.

**Tech Stack:** Rust (2021 edition), `eframe` / `egui` `0.29`, `rfd` `0.15`, `serde` / `serde_json` `1.0`, `wasm-bindgen` / `wasm-bindgen-futures` / `web-sys` (for `wasm32-unknown-unknown`).

**Spec:** `docs/superpowers/specs/2026-10-17-vcd-viewer-design.md`

## Global Constraints

- **License:** Entire crate and all dependencies must be `MIT OR Apache-2.0` (zero GPL/LGPL dependencies).
- **Platforms:** Must compile natively on Windows (without WSL2 or GTK), macOS, Linux, and `wasm32-unknown-unknown`.
- **Memory Bound on 10 GB VCDs:** Never call `fs::read_to_string` or load all signals' transitions into RAM; parse the header first, index checkpoints in chunks (8 MB default), and load transitions strictly for watched `VcdIdCode`s.
- **UI Layout:** Main window contains only Top Toolbar + Measurement Bar, 3-column workspace (Signal List, Value Column, Waveform Canvas), and Status Bar. The **Signal Hierarchy Browser** is a floating modal window (`egui::Window`).
- **Themes:** Dark theme is the default; Light theme is selectable via toolbar toggle and persisted in the signal list file.

## Review Focus

1. **Chunk-boundary token splitting in 10 GB streaming parser:** A timestamp (`#12345000`) or vector value (`b10101100 !`) split across two 8 MB chunk reads must be reassembled seamlessly without dropping or corrupting the transition.
2. **VCD vector left-extension rules (IEEE 1364):** When a multi-bit vector (e.g., 8-bit) is given a shorter value in VCD (`b1`, `bx`, `bz`), `0` and `1` must zero-extend on the left (`00000001`), whereas `x` and `z` must replicate (`xxxxxxxx`, `zzzzzzzz`).
3. **VCD alias ID codes:** Multiple signals in different modules (e.g., `top.clk` and `top.u_cpu.clk`) often share the exact same `VcdIdCode` in VCD files; querying or loading by `SignalId` must resolve through its `VcdIdCode` so aliased signals all receive transitions.
4. **IEEE 754 float edge cases & `X`/`Z` masking:** Formatting a bus containing any `X` or `Z` bit in `FP16`/`BF16`/`FP32`/`FP64` mode must return `"X"` or `"Z"` rather than reinterpreting garbage mask bits as a float, and `NaN`, `+Inf`, `-Inf`, and `-0.0` must format cleanly.
5. **Degenerate zoom & zero-duration drags:** Dragging fewer than 5 pixels horizontally on the waveform canvas must act as a single click (setting the cursor) rather than zooming into a 0-ps window that causes division-by-zero in coordinate transforms.

---

## File Structure

- `Cargo.toml` — Crate manifest (`MIT OR Apache-2.0`, native + `wasm32` target dependencies).
- `index.html` & `Trunk.toml` — Web (WASM) build configuration for `trunk`.
- `.gitignore` — Ignores `/target`, `/.superpowers`, `dist/`.
- `src/lib.rs` — Exports core modules (`byte_source`, `vcd_header`, `waveform_db`, `vcd_loader`, `format`, `project`).
- `src/main.rs` — Native and WASM `eframe` entrypoint.
- `src/byte_source.rs` — `VcdByteSource` trait with chunked implementations for `FileByteSource` (native random-access `std::fs::File`), `MemByteSource` (in-memory slice for tests), and `WebBlobByteSource` (`wasm32` `web_sys::Blob::slice`).
- `src/vcd_header.rs` — Streaming VCD header parser up to `$enddefinitions $end`, producing `HierarchyTree`, `ScopeNode`, `SignalMeta`, `VcdIdCode`, and `Timescale`.
- `src/waveform_db.rs` — Columnar transition storage (`SignalWave`, `LogicState`, `VectorValue`, bit-slice extraction, `partition_point` cursor/edge queries, and multi-resolution LOD buckets).
- `src/vcd_loader.rs` — Chunked VCD body streamer that builds `TimeCheckpoint`s and extracts transitions only for watched `VcdIdCode`s across chunk boundaries.
- `src/format.rs` — Value formatting (`Radix`: Bin, Oct, Hex, Dec, Ascii, IEEE 754 `FloatFormat`: FP16, BF16, FP32, FP64, Auto; `Signedness`: Unsigned, Signed2s) and value-search pattern matcher.
- `src/project.rs` — Workspace data model (`DisplayedRow`: Signal, Group, Divider; `SignalConfig`, `Marker`, `TimeUnit`, `ThemeMode`, `Viewport`) and JSON `.vcdview.json` save/load.
- `src/ui/mod.rs` — `VcdViewerApp` root state, background worker channel polling, keyboard shortcuts, and theme application (`Dark` default / `Light`).
- `src/ui/toolbar.rs` — Top toolbar (file actions, zoom, time unit selector, prev/next edge, find value, add marker, theme toggle) and delta measurement bar (`Cursor`, `Marker`, `ΔT`, frequency).
- `src/ui/hierarchy_modal.rs` — Floating `Signal Hierarchy Browser` window (`egui::Window`) with expandable scope tree, search/filter, and batch add to waveform.
- `src/ui/signal_list.rs` — Synchronized Signal List pane + Value Column + right-click context menu (copy/paste/delete, expand/collapse bits, radix, signedness, IEEE 754 float, analog/digital style, color picker).
- `src/ui/waveform_canvas.rs` — Custom 2D `egui::Painter` time ruler, cursor/marker flags, LOD-accelerated digital/bus/analog trace rendering, horizontal drag-range zoom, and wheel zoom/pan.

---

### Task 1: Project Scaffolding & Chunked `VcdByteSource`

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `index.html`
- Create: `Trunk.toml`
- Create: `src/lib.rs`
- Create: `src/byte_source.rs`
- Create: `src/main.rs`

**Interfaces:**
- Produces (`src/byte_source.rs`):
  - `pub trait VcdByteSource: Send + Sync { fn total_len(&self) -> u64; fn read_chunk(&self, offset: u64, max_len: usize) -> std::io::Result<Vec<u8>>; }`
  - `pub struct MemByteSource { ... }` with `pub fn new(data: Vec<u8>) -> Self`
  - `pub struct FileByteSource { ... }` (native) with `pub fn open(path: &std::path::Path) -> std::io::Result<Self>`
  - `pub struct LineChunkReader<'a> { ... }` with `pub fn new(source: &'a dyn VcdByteSource, start_offset: u64, chunk_size: usize) -> Self` and `pub fn next_token(&mut self) -> std::io::Result<Option<(u64, Vec<u8>)>>` (returns `(byte_offset, whitespace_delimited_token)` seamlessly across chunk boundaries).

- [ ] **Step 1: Create `Cargo.toml`, `.gitignore`, `index.html`, `Trunk.toml`, `src/lib.rs`, and minimal `src/main.rs`**

Configure `Cargo.toml` with `license = "MIT OR Apache-2.0"`, dependencies `egui = "0.29"`, `eframe = { version = "0.29", default-features = false, features = ["default_fonts", "glow", "persistence"] }`, `rfd = "0.15"`, `serde = { version = "1.0", features = ["derive"] }`, `serde_json = "1.0"`, and target-specific dependencies for native vs `wasm32`.

- [ ] **Step 2: Write the failing test in `src/byte_source.rs` for chunk-boundary token reading (Review Focus #1)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_chunk_reader_across_tiny_chunks() {
        let raw = b"#12345678\nb10101100 !\n#99999999\n1\"\n".to_vec();
        let src = MemByteSource::new(raw);
        // Use a 5-byte chunk size so every token splits across chunk boundaries
        let mut reader = LineChunkReader::new(&src, 0, 5);
        let mut tokens = Vec::new();
        while let Some((off, tok)) = reader.next_token().unwrap() {
            tokens.push((off, String::from_utf8(tok).unwrap()));
        }
        assert_eq!(
            tokens,
            vec![
                (0, "#12345678".to_string()),
                (10, "b10101100".to_string()),
                (20, "!".to_string()),
                (22, "#99999999".to_string()),
                (32, "1\"".to_string()),
            ]
        );
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test byte_source::tests::test_line_chunk_reader_across_tiny_chunks`
Expected: FAIL (unimplemented `LineChunkReader::next_token`).

- [ ] **Step 4: Implement `VcdByteSource`, `MemByteSource`, `FileByteSource`, and `LineChunkReader` in `src/byte_source.rs`**

Maintain an internal `carryover: Vec<u8>` buffer in `LineChunkReader` so partial tokens at the end of a `read_chunk(offset, chunk_size)` call are prepended to the next chunk read, preserving exact starting byte offsets.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test byte_source`
Expected: PASS

- [ ] **Step 6: Initialize git repository and commit**

```bash
git init
git add .gitignore Cargo.toml Cargo.lock index.html Trunk.toml src/ docs/ spec.md
git commit -m "feat: scaffold crate and add chunked VcdByteSource with boundary-safe token reader"
```

---

### Task 2: VCD Header Parser & Hierarchy Model (`src/vcd_header.rs`)

**Files:**
- Create: `src/vcd_header.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `VcdByteSource`, `LineChunkReader` from `crate::byte_source`.
- Produces (`src/vcd_header.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] pub struct ScopeId(pub usize);`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] pub struct SignalId(pub usize);`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] pub struct VcdIdCode(pub u32);`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum TimeUnit { Fs, Ps, Ns, Us, Ms, S }` with `pub fn femtoseconds(&self) -> f64` and `pub fn label(&self) -> &'static str`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub struct Timescale { pub factor: u32, pub unit: TimeUnit }`
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] pub enum VarType { Wire, Reg, Logic, Real, Integer, Parameter, Event, Other(String) }`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct SignalMeta { pub id: SignalId, pub name: String, pub full_path: String, pub vcd_id: VcdIdCode, pub var_type: VarType, pub width: u32, pub msb: Option<i32>, pub lsb: Option<i32> }`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct ScopeNode { pub id: ScopeId, pub name: String, pub full_path: String, pub parent: Option<ScopeId>, pub children: Vec<ScopeId>, pub signals: Vec<SignalId> }`
  - `#[derive(Debug, Clone, Default, Serialize, Deserialize)] pub struct VcdHeader { pub timescale: Timescale, pub scopes: Vec<ScopeNode>, pub root_scopes: Vec<ScopeId>, pub signals: Vec<SignalMeta>, pub path_to_signal: std::collections::HashMap<String, SignalId>, pub vcd_id_to_signals: std::collections::HashMap<VcdIdCode, Vec<SignalId>>, pub body_start_offset: u64 }`
  - `pub fn parse_vcd_header(source: &dyn VcdByteSource) -> Result<VcdHeader, String>`

- [ ] **Step 1: Write the failing test in `src/vcd_header.rs` covering nested scopes, bit ranges `[31:0]`, and aliased `VcdIdCode`s (Review Focus #3)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_source::MemByteSource;

    #[test]
    fn test_parse_header_scopes_widths_and_aliases() {
        let vcd = b"$date today $end
$timescale 10 ps $end
$scope module top_soc $end
$var wire 1 ! clk $end
$scope module u_cpu $end
$var wire 1 ! clk_alias $end
$var reg 32 \" pc [31:0] $end
$var real 64 # temp_c $end
$upscope $end
$upscope $end
$enddefinitions $end
#0
0!
";
        let src = MemByteSource::new(vcd.to_vec());
        let hdr = parse_vcd_header(&src).unwrap();
        assert_eq!(hdr.timescale, Timescale { factor: 10, unit: TimeUnit::Ps });
        assert_eq!(hdr.root_scopes.len(), 1);
        assert_eq!(hdr.signals.len(), 4);

        let clk_id = hdr.path_to_signal["top_soc.clk"];
        let alias_id = hdr.path_to_signal["top_soc.u_cpu.clk_alias"];
        assert_eq!(hdr.signals[clk_id.0].vcd_id, hdr.signals[alias_id.0].vcd_id);
        assert_eq!(hdr.vcd_id_to_signals[&hdr.signals[clk_id.0].vcd_id].len(), 2);

        let pc = &hdr.signals[hdr.path_to_signal["top_soc.u_cpu.pc"].0];
        assert_eq!(pc.width, 32);
        assert_eq!((pc.msb, pc.lsb), (Some(31), Some(0)));
        assert!(hdr.body_start_offset > 0 && hdr.body_start_offset < vcd.len() as u64);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test vcd_header`
Expected: FAIL

- [ ] **Step 3: Implement `parse_vcd_header` in `src/vcd_header.rs`**

Parse `$timescale`, `$scope`, `$upscope`, `$var`, and stop immediately upon reading `$end` of `$enddefinitions`, recording the exact byte offset after `$enddefinitions $end` as `body_start_offset`. Intern ASCII identifier codes (e.g., `!`, `"`, `a!`) deterministically into `VcdIdCode(u32)` via base-94 encoding (`c - 33`) or an intern table. Handle split reference names like `$var reg 32 " pc [31:0] $end` and unsplit `pc[31:0]`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test vcd_header`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/vcd_header.rs
git commit -m "feat: add streaming VCD header parser with scope tree and alias mapping"
```

---

### Task 3: Columnar Waveform Storage, Bit Extraction & LOD Pyramid (`src/waveform_db.rs`)

**Files:**
- Create: `src/waveform_db.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `VcdIdCode` from `crate::vcd_header`.
- Produces (`src/waveform_db.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum LogicState { Zero, One, X, Z }`
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub enum SignalValue { Scalar(LogicState), Vector(Vec<LogicState>), Real(f64) }` (where `Vector` stores bits MSB-first: index `0` is MSB, index `width - 1` is LSB)
  - `#[derive(Debug, Clone, Copy)] pub struct LodSummary { pub t_start: u64, pub t_end: u64, pub transition_count: usize, pub first_idx: usize, pub last_idx: usize, pub min_analog: f64, pub max_analog: f64, pub has_xz: bool }`
  - `#[derive(Debug, Clone, Default)] pub struct SignalWave { pub width: u32, pub timestamps: Vec<u64>, pub values: Vec<SignalValue> }` with methods:
    - `pub fn new(width: u32) -> Self`
    - `pub fn push_transition(&mut self, timestamp: u64, value: SignalValue)` (if `timestamp` equals last pushed timestamp, overwrites last value so multiple assignments in the same delta cycle keep the final value)
    - `pub fn value_at(&self, timestamp: u64) -> Option<(u64, &SignalValue)>` (binary searches `timestamps` via `partition_point(|&t| t <= timestamp)`)
    - `pub fn bit_value_at(&self, timestamp: u64, bit_offset_from_msb: usize) -> Option<LogicState>`
    - `pub fn prev_transition(&self, cursor: u64) -> Option<u64>` (strictly `< cursor`)
    - `pub fn next_transition(&self, cursor: u64) -> Option<u64>` (strictly `> cursor`)
    - `pub fn query_viewport_lod(&self, t_start: u64, t_end: u64, max_buckets: usize, signed: bool) -> Vec<LodSummary>`
  - `pub fn parse_vcd_vector_bytes(raw_bits: &[u8], width: u32) -> SignalValue` (implements IEEE 1364 left-extension rules: `0`/`1` extend with `0`, `x`/`X` extend with `X`, `z`/`Z` extend with `Z`).

- [ ] **Step 1: Write failing tests in `src/waveform_db.rs` for IEEE 1364 left-extension (Review Focus #2), binary-search cursor/edge queries, and LOD bucketing**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ieee1364_vector_left_extension() {
        // 'b1' on 4-bit bus -> 0001
        assert_eq!(
            parse_vcd_vector_bytes(b"1", 4),
            SignalValue::Vector(vec![LogicState::Zero, LogicState::Zero, LogicState::Zero, LogicState::One])
        );
        // 'bx' on 4-bit bus -> xxxx
        assert_eq!(
            parse_vcd_vector_bytes(b"x", 4),
            SignalValue::Vector(vec![LogicState::X; 4])
        );
        // 'bz0' on 4-bit bus -> zzz0
        assert_eq!(
            parse_vcd_vector_bytes(b"z0", 4),
            SignalValue::Vector(vec![LogicState::Z, LogicState::Z, LogicState::Z, LogicState::Zero])
        );
    }

    #[test]
    fn test_signal_wave_queries_and_lod() {
        let mut wave = SignalWave::new(1);
        for i in 0..1000u64 {
            let st = if i % 2 == 0 { LogicState::Zero } else { LogicState::One };
            wave.push_transition(i * 10, SignalValue::Scalar(st));
        }
        assert_eq!(wave.value_at(25).unwrap().1, &SignalValue::Scalar(LogicState::Zero));
        assert_eq!(wave.value_at(35).unwrap().1, &SignalValue::Scalar(LogicState::One));
        assert_eq!(wave.prev_transition(35), Some(30));
        assert_eq!(wave.prev_transition(30), Some(20));
        assert_eq!(wave.next_transition(30), Some(40));

        let buckets = wave.query_viewport_lod(0, 10_000, 50, false);
        assert!(buckets.len() <= 50 && !buckets.is_empty());
        assert!(buckets[0].transition_count > 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test waveform_db`
Expected: FAIL

- [ ] **Step 3: Implement `SignalWave`, `parse_vcd_vector_bytes`, edge navigation, and `query_viewport_lod` in `src/waveform_db.rs`**

In `query_viewport_lod`, locate the index range covering `[t_start, t_end]` via `partition_point`. If the number of transitions in `[t_start, t_end]` is `<= max_buckets`, return 1-transition buckets; otherwise partition the slice into `max_buckets` time bins and compute `transition_count`, `first_idx`, `last_idx`, `min_analog`, `max_analog`, and `has_xz` in $O(\text{max\_buckets})$ by striding or scanning each bin.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test waveform_db`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/waveform_db.rs
git commit -m "feat: add columnar SignalWave storage, IEEE 1364 vector extension, and LOD bucketing"
```

---

### Task 4: Number Formatter, IEEE 754 Float Decoder & Value Search (`src/format.rs`)

**Files:**
- Create: `src/format.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `LogicState`, `SignalValue`, `SignalWave` from `crate::waveform_db`.
- Produces (`src/format.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum Radix { Bin, Oct, Hex, Dec, Ascii, Float(FloatFormat) }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum FloatFormat { Auto, Fp16, Bf16, Fp32, Fp64 }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum Signedness { Unsigned, Signed2s }`
  - `pub fn format_signal_value(val: &SignalValue, radix: Radix, signedness: Signedness) -> String`
  - `pub fn signal_value_to_analog(val: &SignalValue, radix: Radix, signedness: Signedness) -> Option<f64>`
  - `pub fn find_value_transition(wave: &SignalWave, start_time: u64, forward: bool, query: &str, radix: Radix, signedness: Signedness) -> Option<u64>`

- [ ] **Step 1: Write failing tests in `src/format.rs` for Bin/Oct/Hex/Dec/Signed2s/ASCII and IEEE 754 `FP16`/`BF16`/`FP32`/`FP64` + `X`/`Z` safety (Review Focus #4)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform_db::{parse_vcd_vector_bytes, LogicState, SignalValue};

    #[test]
    fn test_radix_and_signed_formatting() {
        // 8-bit 0xFE = 254 unsigned, -2 signed
        let val = parse_vcd_vector_bytes(b"11111110", 8);
        assert_eq!(format_signal_value(&val, Radix::Hex, Signedness::Unsigned), "0xFE");
        assert_eq!(format_signal_value(&val, Radix::Oct, Signedness::Unsigned), "0o376");
        assert_eq!(format_signal_value(&val, Radix::Bin, Signedness::Unsigned), "0b11111110");
        assert_eq!(format_signal_value(&val, Radix::Dec, Signedness::Unsigned), "254");
        assert_eq!(format_signal_value(&val, Radix::Dec, Signedness::Signed2s), "-2");
    }

    #[test]
    fn test_ieee754_float_formatting_and_xz_guard() {
        // FP32: 0x40490FDB = 3.1415927
        let pi_bits = parse_vcd_vector_bytes(b"01000000010010010000111111011011", 32);
        let formatted = format_signal_value(&pi_bits, Radix::Float(FloatFormat::Fp32), Signedness::Unsigned);
        assert!(formatted.starts_with("3.14159"));

        // BF16: 0x3F80 = 1.0
        let bf16_one = parse_vcd_vector_bytes(b"0011111110000000", 16);
        assert_eq!(format_signal_value(&bf16_one, Radix::Float(FloatFormat::Bf16), Signedness::Unsigned), "1.0");

        // FP16: 0x3C00 = 1.0, 0x7C00 = +Inf
        let fp16_one = parse_vcd_vector_bytes(b"0011110000000000", 16);
        assert_eq!(format_signal_value(&fp16_one, Radix::Float(FloatFormat::Fp16), Signedness::Unsigned), "1.0");
        let fp16_inf = parse_vcd_vector_bytes(b"0111110000000000", 16);
        assert_eq!(format_signal_value(&fp16_inf, Radix::Float(FloatFormat::Fp16), Signedness::Unsigned), "+Inf");

        // Vector with X bit in FP32 mode must return "X", not a bogus number
        let xz_bits = parse_vcd_vector_bytes(b"0100x000010010010000111111011011", 32);
        assert_eq!(format_signal_value(&xz_bits, Radix::Float(FloatFormat::Fp32), Signedness::Unsigned), "X");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test format`
Expected: FAIL

- [ ] **Step 3: Implement `format_signal_value`, IEEE 754 decoders (`Fp16`, `Bf16`, `Fp32`, `Fp64`, `Auto`), `signal_value_to_analog`, and `find_value_transition` in `src/format.rs`**

Implement exact IEEE 754 bit unpacking:
- `Fp16` (16-bit): sign `(u >> 15) & 1`, exp `(u >> 10) & 0x1F` (bias 15), mantissa `u & 0x3FF` (including subnormals `2^-14 * (m / 1024.0)`, `±Inf`, `NaN`).
- `Bf16` (16-bit): `f32::from_bits((u as u32) << 16)`.
- `Fp32` (32-bit): `f32::from_bits(u as u32)`.
- `Fp64` (64-bit): `f64::from_bits(u)`.
For `find_value_transition`, match either exact formatted string (case-insensitive, ignoring `_` separators) or numeric equivalence.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test format`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/format.rs
git commit -m "feat: add multi-radix, signed 2s-complement, and IEEE 754 (FP16/BF16/FP32/FP64) formatter"
```

---

### Task 5: Streaming VCD Body Indexer & On-Demand Signal Loader (`src/vcd_loader.rs`)

**Files:**
- Create: `src/vcd_loader.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `VcdByteSource`, `LineChunkReader` (`crate::byte_source`); `VcdHeader`, `VcdIdCode` (`crate::vcd_header`); `SignalWave`, `SignalValue`, `LogicState`, `parse_vcd_vector_bytes` (`crate::waveform_db`).
- Produces (`src/vcd_loader.rs`):
  - `#[derive(Debug, Clone, Copy)] pub struct TimeCheckpoint { pub byte_offset: u64, pub timestamp: u64 }`
  - `#[derive(Debug, Default, Clone)] pub struct VcdIndex { pub checkpoints: Vec<TimeCheckpoint>, pub min_timestamp: u64, pub max_timestamp: u64, pub bytes_indexed: u64, pub is_complete: bool }`
  - `pub struct StreamingBodyScanner<'a> { ... }` with:
    - `pub fn new(source: &'a dyn VcdByteSource, header: &'a VcdHeader, watched_ids: std::collections::HashSet<VcdIdCode>, chunk_size: usize) -> Self`
    - `pub fn step_chunk(&mut self) -> Result<bool, String>` (processes one chunk of tokens, updating `index: VcdIndex` and populating `waves: HashMap<VcdIdCode, SignalWave>` only for `watched_ids`; returns `Ok(true)` while more bytes remain, `Ok(false)` when EOF is reached).

- [ ] **Step 1: Write failing test in `src/vcd_loader.rs` verifying selective on-demand extraction and checkpoint creation**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_source::MemByteSource;
    use crate::vcd_header::parse_vcd_header;
    use std::collections::HashSet;

    #[test]
    fn test_selective_signal_loading_and_checkpoints() {
        let vcd = b"$timescale 1 ns $end
$scope module top $end
$var wire 1 ! clk $end
$var reg 8 \" data [7:0] $end
$var wire 1 # ignored_sig $end
$upscope $end
$enddefinitions $end
#0
$dumpvars
0!
b00001111 \"
1#
$end
#10
1!
b10100101 \"
0#
#20
0!
";
        let src = MemByteSource::new(vcd.to_vec());
        let hdr = parse_vcd_header(&src).unwrap();
        let clk_vcd_id = hdr.signals[hdr.path_to_signal["top.clk"].0].vcd_id;
        let data_vcd_id = hdr.signals[hdr.path_to_signal["top.data"].0].vcd_id;
        let ignored_vcd_id = hdr.signals[hdr.path_to_signal["top.ignored_sig"].0].vcd_id;

        let watched: HashSet<_> = [clk_vcd_id, data_vcd_id].into_iter().collect();
        let mut scanner = StreamingBodyScanner::new(&src, &hdr, watched, 32);
        while scanner.step_chunk().unwrap() {}

        assert_eq!(scanner.index.max_timestamp, 20);
        assert!(scanner.waves.contains_key(&clk_vcd_id));
        assert!(scanner.waves.contains_key(&data_vcd_id));
        assert!(!scanner.waves.contains_key(&ignored_vcd_id));
        assert_eq!(scanner.waves[&clk_vcd_id].timestamps, vec![0, 10, 20]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test vcd_loader`
Expected: FAIL

- [ ] **Step 3: Implement `StreamingBodyScanner` in `src/vcd_loader.rs`**

Parse VCD body tokens in chunks:
- `#<u64>`: update `current_timestamp`, update `max_timestamp`, and record a `TimeCheckpoint` every 16 MB of offset progress.
- `$dumpvars` / `$dumpon` / `$dumpoff` / `$ end`: skip directive keywords cleanly; inside `$comment ... $end`, skip until `$end`.
- `0<id>`, `1<id>`, `x<id>`, `z<id>` (scalar value change): decode `VcdIdCode` from `<id>`; only if `watched_ids.contains(&id)`, push `SignalValue::Scalar` to `waves`.
- `b<bits>` / `B<bits>` / `r<float>` / `R<float>` followed by `<id>` token: decode `<id>` first; only if `watched_ids.contains(&id)`, parse the bits/float and push to `waves`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test vcd_loader`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/vcd_loader.rs
git commit -m "feat: add chunked VCD body indexer and on-demand watched signal loader"
```

---

### Task 6: Workspace Model, Viewport Math & Signal List Save/Load (`src/project.rs`)

**Files:**
- Create: `src/project.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `SignalId`, `TimeUnit` (`crate::vcd_header`); `Radix`, `Signedness` (`crate::format`).
- Produces (`src/project.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum ThemeMode { Dark, Light }` (Default: `ThemeMode::Dark`)
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum RenderStyle { Digital, AnalogStep, AnalogLinear }`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct SignalRowConfig { pub signal_ path: String, pub alias: Option<String>, pub radix: Radix, pub signedness: Signedness, pub render_style: RenderStyle, pub color_rgb: [u8; 3], pub expand_bits: bool }`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub enum DisplayedNode { Signal(SignalRowConfig), Group { name: String, collapsed: bool, children: Vec<SignalRowConfig> }, Divider { label: String } }`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct Marker { pub name: String, pub timestamp: u64 }`
  - `#[derive(Debug, Clone, Copy, Serialize, Deserialize)] pub struct Viewport { pub start_time: f64, pub end_time: f64 }` with methods:
    - `pub fn zoom_factor(&mut self, factor: f64, anchor_ratio: f64, max_time: f64)`
    - `pub fn zoom_full(&mut self, min_time: u64, max_time: u64)`
    - `pub fn zoom_drag_range(&mut self, px_start: f32, px_end: f32, canvas_width: f32) -> bool` (returns `false` and does not mutate viewport if `(px_end - px_start).abs() < 5.0` — Review Focus #5)
    - `pub fn time_to_x(&self, time: f64, width: f32) -> f32`
    - `pub fn x_to_time(&self, x: f32, width: f32) -> u64`
  - `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct SignalListDocument { pub version: u32, pub theme: ThemeMode, pub display_unit: TimeUnit, pub cursor_time: u64, pub markers: Vec<Marker>, pub viewport: Viewport, pub rows: Vec<DisplayedNode> }` with `pub fn to_json(&self) -> Result<String, String>` and `pub fn from_json(s: &str) -> Result<Self, String>`.

- [ ] **Step 1: Write failing tests in `src/project.rs` for JSON round-trip and drag-zoom threshold guard (Review Focus #5)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{FloatFormat, Radix, Signedness};
    use crate::vcd_header::TimeUnit;

    #[test]
    fn test_viewport_drag_zoom_ignores_tiny_clicks() {
        let mut vp = Viewport { start_time: 0.0, end_time: 1000.0 };
        // Drag < 5px must return false and leave viewport unchanged
        assert!(!vp.zoom_drag_range(100.0, 103.0, 500.0));
        assert_eq!((vp.start_time, vp.end_time), (0.0, 1000.0));

        // Drag from 25% (125px) to 75% (375px) zooms to [250.0, 750.0]
        assert!(vp.zoom_drag_range(125.0, 375.0, 500.0));
        assert!((vp.start_time - 250.0).abs() < 1e-6);
        assert!((vp.end_time - 750.0).abs() < 1e-6);
    }

    #[test]
    fn test_signal_list_json_roundtrip() {
        let doc = SignalListDocument {
            version: 1,
            theme: ThemeMode::Dark,
            display_unit: TimeUnit::Ns,
            cursor_time: 120,
            markers: vec![Marker { name: "M1".into(), timestamp: 185 }],
            viewport: Viewport { start_time: 0.0, end_time: 500.0 },
            rows: vec![DisplayedNode::Signal(SignalRowConfig::new("top.u_fpu.out", 32))
                .with_radix(Radix::Float(FloatFormat::Fp32))],
        };
        let json = doc.to_json().unwrap();
        let restored = SignalListDocument::from_json(&json).unwrap();
        assert_eq!(restored.theme, ThemeMode::Dark);
        assert_eq!(restored.markers.len(), 1);
        assert_eq!(restored.rows.len(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test project`
Expected: FAIL

- [ ] **Step 3: Implement `Viewport`, `SignalListDocument`, and `DisplayedNode` flattening helpers in `src/project.rs`**

Add a helper `pub fn flatten_visible_rows(&self, header: &VcdHeader) -> Vec<VisibleRow>` on `SignalListDocument` that expands non-collapsed `Group`s and `expand_bits == true` signals into individual bit sub-rows (`BitSlice { signal_id, bit_index_from_msb, bit_label }`) so the Signal List, Value Column, and Waveform Canvas iterate over the exact same `Vec<VisibleRow>`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test project`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/project.rs
git commit -m "feat: add workspace project model, viewport zoom math, and JSON signal list persistence"
```

---

### Task 7: App Shell, Async/Thread Loader Bridge, Top Toolbar & Hierarchy Browser Modal (`src/ui/mod.rs`, `src/ui/toolbar.rs`, `src/ui/hierarchy_modal.rs`)

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/toolbar.rs`
- Create: `src/ui/hierarchy_modal.rs`
- Modify: `src/byte_source.rs` (add `WebBlobByteSource` for `wasm32`)
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `crate::byte_source`, `crate::vcd_header`, `crate::vcd_loader`, `crate::waveform_db`, `crate::format`, `crate::project`.
- Produces:
  - `pub struct VcdViewerApp` implementing `eframe::App` in `src/ui/mod.rs`.
  - `pub fn render_top_toolbar(app: &mut VcdViewerApp, ctx: &egui::Context, ui: &mut egui::Ui)` in `src/ui/toolbar.rs`.
  - `pub fn render_hierarchy_modal(app: &mut VcdViewerApp, ctx: &egui::Context)` in `src/ui/hierarchy_modal.rs`.

- [ ] **Step 1: Write unit test in `src/ui/hierarchy_modal.rs` for hierarchy search/filtering**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_source::MemByteSource;
    use crate::vcd_header::parse_vcd_header;

    #[test]
    fn test_filter_signals_in_scope() {
        let vcd = b"$timescale 1ns $end $scope module top $end $var wire 1 ! axi_aclk $end $var wire 32 \" axi_awaddr $end $var wire 1 # uart_tx $end $upscope $end $enddefinitions $end";
        let hdr = parse_vcd_header(&MemByteSource::new(vcd.to_vec())).unwrap();
        let matched = filter_scope_signals(&hdr, hdr.root_scopes[0], "axi");
        assert_eq!(matched.len(), 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test hierarchy_modal`
Expected: FAIL

- [ ] **Step 3: Implement `src/ui/mod.rs`, `src/ui/toolbar.rs`, and `src/ui/hierarchy_modal.rs`**

- **`src/ui/mod.rs` (`VcdViewerApp`):**
  - Holds `header: Option<Arc<VcdHeader>>`, `waves: HashMap<VcdIdCode, SignalWave>`, `index: VcdIndex`, `doc: SignalListDocument`, `selected_signal: Option<String>`, `show_hierarchy_modal: bool`, `value_search_query: String`, and channel receivers for background VCD loading (`std::sync::mpsc`).
  - Applies `ThemeMode::Dark` (default Catppuccin-inspired dark palette) or `ThemeMode::Light` (`egui::Visuals::light()`) on startup and change.
  - Includes a built-in demo SoC VCD generator (`Load Demo SoC VCD` button in toolbar alongside `Load VCD`) so users can also immediately test all features with one click.
  - When new signals are added via the Hierarchy Browser or `Load Signal List`, spawns/updates the chunked `StreamingBodyScanner` for the active set of watched `VcdIdCode`s and streams progress updates to the status bar.
- **`src/ui/toolbar.rs`:**
  - Renders `Load VCD`, `Select Signals...` (toggles `show_hierarchy_modal`), `Load List`, `Save List`, `Zoom In`, `Zoom Out`, `Zoom Full`, `Prev Edge (⏮)`, `Next Edge (⏭)`, `Find Value` input + prev/next buttons, `Add Marker`, `Time Unit` combo box (`fs`, `ps`, `ns`, `us`, `ms`, `s`), and `Dark / Light` theme button.
  - Renders the **Delta Measurement Bar** displaying `Primary Cursor` time, active `Marker` time(s) (with click-to-jump and `✕` to remove), `ΔT`, and frequency (`Hz`/`kHz`/`MHz`/`GHz`).
- **`src/ui/hierarchy_modal.rs`:**
  - Floating `egui::Window::new("Signal Hierarchy Browser")` (Option B layout).
  - Left side: recursive collapsible `$scope` tree (`egui::CollapsingHeader`).
  - Right side: search filter input, signal list with checkboxes, `[Add Selected to Waveform]` and `[Add All in Scope]` buttons.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/ src/byte_source.rs src/main.rs
git commit -m "feat: add app shell, toolbar, delta measurement bar, and modal signal hierarchy browser"
```

---

### Task 8: Signal List Pane, Value Column, Right-Click Menu & Waveform Canvas (`src/ui/signal_list.rs`, `src/ui/waveform_canvas.rs`)

**Files:**
- Create: `src/ui/signal_list.rs`
- Create: `src/ui/waveform_canvas.rs`
- Modify: `src/ui/mod.rs`

**Interfaces:**
- Consumes: `VcdViewerApp` (`crate::ui`), `VisibleRow`, `Viewport`, `RenderStyle` (`crate::project`), `SignalWave`, `LogicState`, `SignalValue` (`crate::waveform_db`), `format_signal_value`, `signal_value_to_analog` (`crate::format`).
- Produces:
  - `pub fn render_signal_and_value_panes(app: &mut VcdViewerApp, ui: &mut egui::Ui, visible_rows: &[VisibleRow], row_height: f32)` in `src/ui/signal_list.rs`
  - `pub fn render_waveform_canvas(app: &mut VcdViewerApp, ui: &mut egui::Ui, visible_rows: &[VisibleRow], row_height: f32)` in `src/ui/waveform_canvas.rs`

- [ ] **Step 1: Write unit test in `src/ui/waveform_canvas.rs` for time-ruler tick interval selection and unit scaling**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcd_header::{TimeUnit, Timescale};

    #[test]
    fn test_compute_ruler_ticks_and_unit_formatting() {
        let ts = Timescale { factor: 1, unit: TimeUnit::Ns };
        // Viewport from 0 to 1000 ns displayed in Us (microseconds)
        let label = format_timestamp_in_unit(500, ts, TimeUnit::Us);
        assert_eq!(label, "0.500 us");

        let ticks = compute_nice_ticks(0.0, 1000.0, 800.0);
        assert!(ticks.len() >= 4 && ticks.len() <= 20);
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test waveform_canvas`
Expected: FAIL

- [ ] **Step 3: Implement `src/ui/signal_list.rs` and `src/ui/waveform_canvas.rs` and wire them into a shared `egui::ScrollArea::vertical()` in `src/ui/mod.rs`**

- **`src/ui/signal_list.rs`:**
  - Header bar with `+ Group` and `+ Divider` buttons, plus `Value @ <cursor>` header.
  - Iterates `visible_rows` with fixed `row_height` (`26.0` px):
    - Renders Group row (with expand/collapse arrow), Divider row, Signal row, or indented BitSlice row (`[bit]`).
    - Supports up/down/drag reordering of top-level rows.
    - Renders the **Value Column** right-aligned using `format_signal_value` at `app.doc.cursor_time`.
    - Attaches `.context_menu(|ui| ...)` to every Signal row with:
      - `Copy Signal`, `Paste Below`, `Delete Signal`
      - `Expand Bits` / `Collapse Bits` (when `width > 1`)
      - `Radix`: `Hex`, `Bin`, `Oct`, `Dec`, `ASCII`
      - `Signedness`: `Unsigned`, `Signed (2's Complement)`
      - `IEEE 754 Float`: `Auto Float`, `FP16 (16-bit)`, `BF16 (16-bit)`, `FP32 (32-bit)`, `FP64 (64-bit)`
      - `Render Style`: `Digital Bus`, `Analog (Step)`, `Analog (Linear)`
      - `Trace Color`: preset color buttons + `ui.color_edit_button_srgb(&mut row.color_rgb)`
- **`src/ui/waveform_canvas.rs`:**
  - Draws the **Time Ruler** at top (`28.0` px height) with major/minor ticks formatted via `format_timestamp_in_unit`, Primary Cursor flag (red/pink), and named Marker flags (`M1`, `M2`... in gold) with delta arrow overlay between Cursor and nearest/active Marker.
  - Draws each row in `visible_rows` using `query_viewport_lod(t_start, t_end, canvas_width as usize, is_signed)`:
    - When a bucket has `transition_count > 2` in 1–2 pixels, draws a high-density toggle block / min-max envelope.
    - For 1-bit scalar or `BitSlice`: draws logic high/low step lines, vertical edge transitions, red shaded box for `X`, orange mid-line for `Z`.
    - For multi-bit `Digital`: draws bus envelopes (`═╪═`) with transition crossovers and centered value text (`format_signal_value`) when interval pixel width `> 40.0` px.
    - For `AnalogStep` / `AnalogLinear`: maps `[min_val, max_val]` to row vertical bounds and draws step or linear polyline.
  - Handles mouse interactions on the canvas:
    - Left-drag draws semi-transparent zoom selection rectangle with live start/end/ΔT readout; on release, calls `viewport.zoom_drag_range(...)`. If drag `< 5.0` px, sets `cursor_time = viewport.x_to_time(...)`.
    - `Ctrl + Scroll` zooms in/out anchored at mouse X; `Shift + Scroll` or middle-drag pans horizontally.

- [ ] **Step 4: Run all tests and verify both native and `wasm32` builds**

Run:
```bash
cargo test
cargo check --release
```
Expected: All unit tests PASS and zero compilation errors/warnings.

- [ ] **Step 5: Commit**

```bash
git add src/ui/
git commit -m "feat: add signal list pane, value column, IEEE 754 context menu, and LOD waveform canvas"
```
