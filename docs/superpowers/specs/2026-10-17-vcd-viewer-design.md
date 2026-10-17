# Rust Cross-Platform & Web VCD Waveform Viewer — Design Specification

## 1. Intent & Why

Existing VCD (Value Change Dump) waveform viewers present painful trade-offs:
- **GTKWave** is licensed under GPL, restricting permissive commercial/embedded distribution and customization.
- **Surfer** and other tools often require WSL2 on Windows for full functionality or have complex setup chains.
- Most lightweight or browser-based VCD viewers load the entire file into memory, crashing on realistic **10 GB SoC simulation dumps** (especially in WebAssembly, which has a hard 4 GB address space limit).

This project builds a fast, permissively licensed (**MIT OR Apache-2.0**), standalone VCD waveform viewer in **Rust** that runs natively on **Windows (without WSL2), macOS, Linux, and Web (WASM)**, capable of browsing and rendering multi-gigabyte SoC traces smoothly at 60 FPS.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Permissive Licensing:** Entire codebase and dependency tree must be compatible with `MIT OR Apache-2.0` (no GPL/LGPL dependencies).
- **True Cross-Platform + Web:** Single Rust codebase compiling natively to Windows (`.exe`), macOS, Linux, and Web (`wasm32-unknown-unknown` via `eframe`/`egui` + `trunk` or `wasm-bindgen`).
- **10 GB SoC VCD Scalability:** Open and browse the design hierarchy of a 10 GB VCD almost immediately via header parsing + streaming/chunked background indexing, loading full signal transition histories **only on-demand** for signals added to the active waveform view.
- **Clean, Focused Main Window + Modal Hierarchy Browser:** Keep 100% of the main window dedicated to the active Signal List, Value Column, and Waveform Canvas; provide a dedicated floating/modal **Signal Hierarchy Browser** window to expand/collapse SoC module trees, filter/search signals, and batch-add them to the waveform view.
- **Complete Core + Pro Debugging Workflow:** Support all requested basic functions (load VCD, load/save signal list, zoom in/out/full, drag-range zoom, time unit selection, forward/backward transition search, right-click menu with copy/delete/radix/signedness/IEEE-754 float, dark/light themes) plus essential SoC debugging features (named markers & delta time/frequency measurement, value search, bit-slice expansion, collapsible signal groups & dividers, analog step/interpolated rendering, and custom trace colors).

### Non-Goals (for Initial Scope)
- FST, FSDB, or GHW waveform formats (architecture will abstract the waveform data source so FST can be added later, but only IEEE 1364-1995 / 2005 VCD is required now).
- RTL source code annotation / schematic cross-probing.
- Waveform editing or stimulus generation (viewer is strictly read-only for VCD files).
- Complex multi-signal boolean expression synthesis (e.g., `sig_a & sig_b` compiler) — left out of initial scope to keep the core viewer lean.

### Anti-Goals (What Would Make This a Failure)
- **Any GPL/AGPL dependency** in the Cargo dependency tree.
- **Requiring WSL2, GTK, or C/C++ toolchains on Windows** (must compile with standard `cargo build --release` on native Windows MSVC/GNU).
- **Reading the entire VCD file into RAM upfront** (`fs::read_to_string` or storing transitions for all 100,000 unselected SoC signals in memory), which will OOM on 10 GB files on both desktop and WASM.
- **Freezing the UI thread** during file loading or zooming out over millions of clock edges.

---

## 3. User Interface & Feature Specification

### 3.1 Main Window Layout
The main window consists of three vertical zones:
1. **Top Toolbar & Measurement Bar:**
   - **File Actions:** `Load VCD`, `Select Signals...` (opens Hierarchy Browser modal), `Load Signal List`, `Save Signal List`.
   - **Zoom Controls:** `Zoom In`, `Zoom Out`, `Zoom Full` (fit `[0, max_timestamp]`).
   - **Time Unit Selector:** Dropdown to set display time unit (`fs`, `ps`, `ns`, `us`, `ms`, `s`). All ruler ticks, cursor readouts, and delta measurements automatically scale to the selected unit.
   - **Transition & Value Search:**
     - `Prev Edge (⏮)` / `Next Edge (⏭)`: Moves primary cursor to the previous or next transition of the currently selected signal.
     - `Find Value`: Text box + Prev/Next buttons to jump to the previous or next timestamp where the selected signal equals the entered value (evaluated using the signal's current display format/radix, e.g., `0xdeadbeef`, `1024`, `3.14159`, or binary patterns with `x`/`z` wildcards).
   - **Marker Controls:** `Add Marker` button (drops a named marker `M1`, `M2`, etc. at the current cursor time) and a **Delta Measurement Bar** showing `Primary Cursor Time`, active `Marker Time(s)`, `ΔT = Marker - Cursor`, and equivalent frequency `1 / |ΔT|` (in Hz/kHz/MHz/GHz).
   - **Theme Toggle:** Switch between **Dark Theme (default)** and **Light Theme**.
2. **Center Split Workspace (3 synchronized horizontal columns sharing vertical scroll):**
   - **Column 1: Signal List Pane (Left):**
     - Displays active rows: Signals, expanded Bit Slices (`sig[3]`, `sig[2]`...), collapsible **Signal Groups** (folders), and labeled **Dividers/Separators**.
     - Supports click selection, multi-selection (`Ctrl/Cmd+Click`, `Shift+Click`), drag-and-drop vertical reordering, and keyboard shortcuts (`Delete`/`Backspace`, `Ctrl+C`/`Ctrl+V`).
     - Toolbar header buttons to `+ Group` and `+ Divider`.
   - **Column 2: Value Column (Middle):**
     - Shows the formatted value of each displayed signal (and expanded bit slice) at the **Primary Cursor Time**.
     - Resizable column width via draggable vertical splitter.
   - **Column 3: Waveform Display Area (Right):**
     - **Top Time Ruler:** Shows major/minor time ticks formatted in the active Time Unit, primary cursor flag, and named marker flags (`M1`, `M2`...). Clicking/dragging on the ruler moves the primary cursor; clicking/dragging a marker flag moves or deletes that marker.
     - **Waveform Canvas:** Renders synchronized rows aligned 1:1 with the Signal List and Value Column.
       - **1-bit signals:** High (`1`), Low (`0`), Unknown (`X` — shaded/hatched red), High-Z (`Z` — mid-line orange).
       - **Multi-bit buses (Digital Mode):** Hexagonal/bus transitions (`═╪═`) with formatted value text centered inside each stable interval when wide enough; red fill when any bit is `X`, orange when `Z`.
       - **Multi-bit buses / `real` variables (Analog Mode):** Step or linear-interpolated continuous trace scaled vertically to the signal's min/max range within the row height.
     - **Mouse Interactions on Waveform Canvas:**
       - **Left-click:** Sets primary cursor time (with optional snap-to-nearest-edge within a few pixels).
       - **Left-drag horizontally:** Draws a highlighted zoom-selection box (`[t_start, t_end]`) with live time range readout; releasing the mouse zooms the selected horizontal range to fill the waveform display area (if drag distance > 5 px; otherwise treats as a single click to move cursor).
       - **Mouse Wheel / Trackpad:** `Scroll` scrolls vertically through signals; `Ctrl + Scroll` (or pinch) zooms in/out centered on mouse X position; `Shift + Scroll` (or middle-mouse drag) pans horizontally in time.
3. **Bottom Status Bar:**
   - Shows VCD file name, timescale, total time range, background indexing/loading progress bar (e.g., `Indexing VCD: 4.2 GB / 10.0 GB (42%)`), and memory usage statistics.

### 3.2 Floating / Pop-up Signal Hierarchy Browser Window (Option B)
- Opened via the `Select Signals...` button (and automatically opened when a new VCD file finishes reading its header).
- **Left Pane — Design Hierarchy Tree:** Expandable/collapsible tree of VCD `$scope` modules (`module`, `task`, `function`, `begin`, `fork`).
- **Right/Bottom Pane — Signals in Scope:** Lists signals belonging to the selected scope(s), showing name, bit width (e.g., `[31:0]`), and VCD type (`wire`, `reg`, `logic`, `real`, `integer`, `parameter`).
- **Search & Filter:** Real-time substring or glob/regex filter across module paths and signal names.
- **Selection & Batch Actions:** Checkboxes or multi-select list with `Add Selected to Waveform` and `Add All in Scope` buttons. Adding a signal triggers on-demand loading of its transition data if not already cached.

### 3.3 Right-Click Signal Context Menu
Right-clicking any signal row in the Signal List, Value Column, or Waveform Canvas opens a context menu with:
- **Basic Edit Functions:**
  - `Copy Signal` / `Paste Signal` (duplicates signal row below selection)
  - `Rename Display Alias` (optional custom label while preserving original VCD path tooltip)
  - `Delete Signal`
  - `Expand Bits` / `Collapse Bits` (for multi-bit vectors, expands/collapses individual bit sub-rows `[MSB]..[LSB]`)
- **Radix / Number Format Selection:**
  - `Binary (Bin)` (`0b...`)
  - `Octal (Oct)` (`0o...`)
  - `Hexadecimal (Hex)` (`0x...`, default for vectors > 1 bit)
  - `Decimal (Dec)`
  - `ASCII` (interprets 8-bit chunks as printable ASCII characters)
  - **IEEE 754 Floating-Point (`Float`):**
    - `IEEE 754 FP16 (Half, 16-bit: 1 sign, 5 exp, 10 mantissa)`
    - `IEEE 754 BF16 (BFloat16, 16-bit: 1 sign, 8 exp, 7 mantissa)`
    - `IEEE 754 FP32 (Single, 32-bit: 1 sign, 8 exp, 23 mantissa)`
    - `IEEE 754 FP64 (Double, 64-bit: 1 sign, 11 exp, 52 mantissa)`
    - `Auto Float` (automatically picks FP16/FP32/FP64 based on signal bit width, or native `f64` for VCD `real` variables).
    - Properly formats `NaN`, `+Inf`, `-Inf`, `-0.0`, subnormals, and scientific notation for very large/small magnitudes. If any bit in the vector is `X` or `Z`, displays `X` or `Z` instead of a misleading float.
- **Signedness Selection (applies to Integer Decimal & Analog rendering):**
  - `Unsigned`
  - `Signed (2's Complement)` (interprets MSB according to the signal's declared bit width)
- **Waveform Render Style:**
  - `Digital Bus / Logic`
  - `Analog (Step)`
  - `Analog (Linear Interpolated)`
- **Trace Color:**
  - Preset palette picker (Default Green, Blue, Cyan, Yellow, Orange, Purple, Pink, White/Black) + custom RGB picker.

### 3.4 Save & Load Signal List File (`.vcdview.json`)
- Human-readable, versioned JSON file saved/loaded via native file dialog (or browser download/upload in WASM).
- Persists:
  - Displayed item hierarchy (groups, dividers, signals with full hierarchical path `top.u_cpu.clk`, expanded bit state).
  - Per-signal display settings: radix/IEEE-754 format, signedness, render style (digital/analog), custom trace color, and alias.
  - Display time unit, active theme, primary cursor time, zoom viewport `[view_start, view_end]`, and named markers (`M1`, `M2`...).
- When loaded against a VCD file, matches signals by hierarchical path, queues on-demand loading for those VCD ID codes, and warns gracefully (without failing) if any saved signal path is missing in the current VCD.

---

## 4. Technical Architecture & 10 GB VCD Engine (Designer's Calls)

To guarantee native Windows/macOS/Linux + Web (WASM) support while handling 10 GB VCD files, the project will use the following architecture (marked as **Designer's Call**):

### 4.1 Technology Stack (Designer's Call)
- **Language & License:** Rust (2021/2024 edition), licensed under `MIT OR Apache-2.0`.
- **GUI Framework:** `egui` + `eframe` (`0.29+` / `0.30+`).
  - *Why:* Pure Rust, immediate-mode GUI with custom 2D `Painter` primitives ideal for high-density waveform rendering, zero C/GTK dependencies on Windows (uses `winit` + `wgpu`/`glow`), and first-class WebAssembly (`wasm32-unknown-unknown`) support out of the box.
- **File Dialogs:** `rfd` (Rusty File Dialog) — supports native Windows/macOS/Linux file dialogs and Web `<input type="file">` / Blob downloads asynchronously.
- **Serialization:** `serde` + `serde_json` for saving/loading signal list files.

### 4.2 Random-Access Chunked Reader Abstraction (`VcdByteSource`) (Designer's Call)
Why a 10 GB file works on both Desktop and Web:
- **On Native Desktop (Windows/macOS/Linux):** Uses random-access `std::fs::File` (seek + read chunks of 4–16 MB) or memory-mapped windows. Never loads the full 10 GB file into heap memory.
- **On Web (WASM):** A browser `<input type="file">` returns a `web_sys::File` (which inherits from `Blob`). In JavaScript/WASM, a `Blob` is a handle to the disk file—calling `file.slice(start_offset, end_offset)` and reading only a 8 MB chunk into WASM linear memory allows streaming through a **10 GB local VCD file inside a browser tab** while keeping WASM heap usage under a few hundred megabytes!

### 4.3 Two-Phase VCD Parser & On-Demand Signal Loader (Designer's Call)
A 10 GB SoC VCD typically has 50,000–500,000 signals in the header, while a user views 10–100 signals at a time.
1. **Phase 1 — Instant Header Parse (< 1 second):**
   - Reads from offset `0` until `$enddefinitions $end`.
   - Builds the compact `HierarchyTree` (scopes, signal names, widths, and compact `VcdIdCode` mapping).
   - Immediately unlocks the UI and opens the **Signal Hierarchy Browser** so the user can start picking signals right away.
2. **Phase 2 — Background Checkpoint Indexing & Targeted Extraction:**
   - A background worker (native thread on desktop, chunked async task on WASM) scans the body after `$enddefinitions $end` in 8 MB chunks:
     - Records periodic **Time Checkpoints** (every ~16–64 MB of file offset, storing `(file_byte_offset, timestamp)` and optionally a snapshot of active signal states) so the viewer knows `max_timestamp` and can seek directly to any time window `[t_start, t_end]` in a 10 GB file without re-reading from byte 0.
     - Simultaneously extracts transitions for all **currently active/watched `VcdIdCode`s** into compact columnar buffers.
3. **On-Demand Signal Loading:**
   - When the user adds new signals from the Hierarchy Browser, the loader scans the VCD stream (filtering strictly by the requested `VcdIdCode`s—ignoring the other 99.9% of signals in the file using a fast byte-lookup table/bitset) to populate their waveform traces, prioritizing the currently visible time window first if checkpoints are available, then filling the full trace in the background.

### 4.4 Compact Columnar Waveform Storage & Multi-Resolution LOD (Designer's Call)
- For each loaded signal, transitions are stored in a cache-friendly columnar structure:
  - `timestamps: Vec<u64>` (monotonically increasing, enabling $O(\log N)$ binary search via `partition_point` for cursor value lookup, edge jumping, and viewport slicing).
  - **1-bit signals:** Packed 2-bit states (`0`, `1`, `X`, `Z`) — 4 transitions per byte.
  - **Multi-bit signals ≤ 64 bits:** Packed `(val_bits: u64, xz_mask: u64)` or arena-indexed bitvectors for > 64 bits.
  - **Level-of-Detail (LOD) Pyramid / Min-Max Decimation:** When zoomed out so that 1,000,000 transitions map to a single horizontal screen pixel, the renderer uses pre-bucketed LOD summaries (or adaptive stride sampling when pixel density exceeds 2 transitions/pixel) to draw a clean min/max envelope or high-density toggle block in $O(\text{screen\_width})$ draw calls, guaranteeing 60 FPS even on 100M+ edge clock signals.

---

## 5. What Is Left to the Builder

The builder has full autonomy over:
- Internal Rust module organization (e.g., workspace vs. single crate with `vcd_core`, `waveform_db`, `ui` modules).
- Exact visual styling constants (padding, font sizes, exact Catppuccin/Dracula-inspired dark theme and clean light theme color hex codes).
- Exact chunk size (e.g., 4 MB vs. 16 MB) and checkpoint interval for the streaming VCD indexer.
- Keyboard shortcut bindings (e.g., `+`/`-` for zoom, `F` for zoom full, `N`/`P` for next/prev edge, `Ctrl+O` for open).
- Automated unit and integration test suites (generating synthetic VCD files with 1-bit, multi-bit, `real`, `X`/`Z`, signed, and IEEE-754 float test vectors, plus benchmark/stress tests for streaming large VCD data).
