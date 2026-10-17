# Synopsys nWave Zoom Keys, Hierarchy Toolbar Button & Shift Range Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Synopsys nWave keyboard shortcuts for waveform zoom (`Z` / `Shift+Z` to zoom in, `z` to zoom out, `f` to zoom full), a clear toolbar button to reveal the Signal Hierarchy window of the loaded VCD (`🌳 Signal Hierarchy`), and `Shift`-based contiguous range selection (`Shift+Click` and `Shift+Up`/`Shift+Down` arrow keys) in both the Signal Hierarchy window signal list and the main viewer signal list.

**Architecture:** We extract pure, unit-tested selection and keyboard action helpers in `src/ui/signal_list.rs`, `src/ui/hierarchy_modal.rs`, and `src/ui/mod.rs`. Both the main signal pane (`SignalLocation`) and the Signal Hierarchy modal (`SignalId`) maintain an anchor item and a focus item so `Shift+Click` and `Shift+Up`/`Shift+Down` arrow keys select contiguous slices of the currently visible signal list. Waveform zoom keyboard dispatch maps `Z` (`Shift+Z`) to Zoom In (`0.5x` span), `z` (unshifted) to Zoom Out (`2.0x` span), and `f` to Zoom Full, matching Synopsys Verdi / nWave.

**Tech Stack:** Rust 2024 (`1.85+`), `eframe` / `egui` `0.29`

**Spec:** `docs/superpowers/specs/2026-10-17-vcd-viewer-design.md`

## Global Constraints

- Dual license `MIT OR Apache-2.0`; no GPL dependencies.
- All file paths in `egui::Stroke::new` must use explicit `_f32` literals (`1.0_f32`) for `rustc 1.99+` compatibility.
- Keyboard shortcuts must be suppressed when any text input has keyboard focus (`ctx.wants_keyboard_input()`).
- Both native (`cargo check --release`) and WebAssembly (`cargo check --target wasm32-unknown-unknown`) targets must compile with zero warnings.

## Review Focus

1. **Synopsys nWave `z` vs `Z` case sensitivity:** In Synopsys nWave, lowercase `z` (without `Shift`) zooms **out**, whereas uppercase `Z` (`Shift + Z`) zooms **in**, and `f` zooms **full**. Reversing `z` and `Z` or failing to distinguish `modifiers.shift` on `Key::Z` would violate nWave muscle memory.
2. **Repeated `Shift+Click` and `Shift+Up/Down` anchor stability:** When extending a range selection with `Shift+Click` or `Shift+Up`/`Shift+Down`, the anchor (`primary_selected` / `hierarchy_anchor_signal`) must stay fixed at the initial selection point while the moving end (`focus_selected` / `hierarchy_focus_signal`) updates, so pressing `Shift+Down` three times or `Shift+Click`ing a different endpoint recomputes the range from the original anchor rather than accumulating stale items outside the new range.
3. **Signal Hierarchy checkbox vs row `Shift+Click`:** Users in the Signal Hierarchy window may `Shift+Click` either directly on the checkbox widget or on the signal label/row; both interactions must trigger contiguous range selection across `visible_sigs` rather than only toggling a single checkbox.
4. **Revealing the Signal Hierarchy window for an already-loaded VCD:** Clicking the toolbar button (`🌳 Signal Hierarchy`) when a VCD is loaded (including the initial Demo SoC VCD or a file loaded earlier whose modal was closed) must reveal the Signal Hierarchy window with a valid `selected_scope` initialized if `None`.
5. **Keyboard focus isolation between Hierarchy Modal and Main Viewer:** When `show_hierarchy_modal` is `true`, `Up`/`Down`/`Shift+Up`/`Shift+Down` and `Cmd/Ctrl+A` must operate on the Signal Hierarchy window's `visible_sigs` rather than mutating the main viewer's signal selection behind the modal.

---

### Task 1: Main Signal List & Hierarchy Modal `Shift+Click` and `Shift+Up/Down` Range Selection (`src/ui/signal_list.rs`, `src/ui/hierarchy_modal.rs`)

**Files:**
- Modify: `src/ui/signal_list.rs`
- Modify: `src/ui/hierarchy_modal.rs`
- Modify: `src/ui/mod.rs`

**Interfaces:**
- Produces (`src/ui/signal_list.rs`):
  - `pub fn apply_row_click_selection(selected: &mut BTreeSet<SignalLocation>, primary: &mut Option<SignalLocation>, focus: &mut Option<SignalLocation>, clicked_loc: SignalLocation, ctrl_or_cmd: bool, shift: bool, ordered_visible_locs: &[SignalLocation])`
  - `pub fn apply_arrow_key_selection(selected: &mut BTreeSet<SignalLocation>, primary: &mut Option<SignalLocation>, focus: &mut Option<SignalLocation>, move_down: bool, shift: bool, ordered_visible_locs: &[SignalLocation])`
- Produces (`src/ui/hierarchy_modal.rs`):
  - `pub fn apply_hierarchy_click_selection(checked: &mut HashSet<SignalId>, anchor: &mut Option<SignalId>, focus: &mut Option<SignalId>, clicked_id: SignalId, is_checkbox_click: bool, ctrl_or_cmd: bool, shift: bool, visible_sigs: &[SignalId])`
  - `pub fn apply_hierarchy_arrow_selection(checked: &mut HashSet<SignalId>, anchor: &mut Option<SignalId>, focus: &mut Option<SignalId>, move_down: bool, shift: bool, visible_sigs: &[SignalId])`
- Produces (`src/ui/mod.rs` fields on `VcdViewerApp`):
  - `pub focus_selected: Option<SignalLocation>`
  - `pub hierarchy_anchor_signal: Option<SignalId>`
  - `pub hierarchy_focus_signal: Option<SignalId>`

- [ ] **Step 1: Write failing unit tests in `src/ui/signal_list.rs` and `src/ui/hierarchy_modal.rs` for `Shift+Click` and `Shift+Up/Down` range selection (Review Focus #2 & #3)**

In `src/ui/signal_list.rs`:
```rust
    #[test]
    fn test_shift_arrow_and_shift_click_range_selection() {
        let locs = vec![
            SignalLocation { node_idx: 0, group_child_idx: None },
            SignalLocation { node_idx: 1, group_child_idx: None },
            SignalLocation { node_idx: 2, group_child_idx: None },
            SignalLocation { node_idx: 3, group_child_idx: None },
        ];
        let mut selected = BTreeSet::new();
        let mut primary = None;
        let mut focus = None;

        // Click index 1 -> anchor=1, focus=1, selected={1}
        apply_row_click_selection(&mut selected, &mut primary, &mut focus, locs[1], false, false, &locs);
        assert_eq!(selected, BTreeSet::from([locs[1]]));

        // Shift+Down twice -> range {1, 2, 3}
        apply_arrow_key_selection(&mut selected, &mut primary, &mut focus, true, true, &locs);
        assert_eq!(selected, BTreeSet::from([locs[1], locs[2]]));
        apply_arrow_key_selection(&mut selected, &mut primary, &mut focus, true, true, &locs);
        assert_eq!(selected, BTreeSet::from([locs[1], locs[2], locs[3]]));

        // Shift+Up once -> shrinks back to {1, 2} because anchor stays at 1
        apply_arrow_key_selection(&mut selected, &mut primary, &mut focus, false, true, &locs);
        assert_eq!(selected, BTreeSet::from([locs[1], locs[2]]));
    }
```

In `src/ui/hierarchy_modal.rs`:
```rust
    #[test]
    fn test_hierarchy_shift_click_and_shift_arrow_range_selection() {
        use std::collections::HashSet;
        let visible = vec![SignalId(0), SignalId(1), SignalId(2), SignalId(3), SignalId(4)];
        let mut checked = HashSet::new();
        let mut anchor = None;
        let mut focus = None;

        // Click checkbox on SignalId(1) -> checks {1}, anchor=1
        apply_hierarchy_click_selection(
            &mut checked,
            &mut anchor,
            &mut focus,
            SignalId(1),
            true,
            false,
            false,
            &visible,
        );
        assert_eq!(checked, HashSet::from([SignalId(1)]));

        // Shift+Click on SignalId(3) -> checks contiguous range {1, 2, 3}
        apply_hierarchy_click_selection(
            &mut checked,
            &mut anchor,
            &mut focus,
            SignalId(3),
            true,
            false,
            true,
            &visible,
        );
        assert_eq!(checked, HashSet::from([SignalId(1), SignalId(2), SignalId(3)]));

        // Shift+Down arrow -> extends range from anchor(1) to focus(4): {1, 2, 3, 4}
        apply_hierarchy_arrow_selection(&mut checked, &mut anchor, &mut focus, true, true, &visible);
        assert_eq!(
            checked,
            HashSet::from([SignalId(1), SignalId(2), SignalId(3), SignalId(4)])
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib`
Expected: FAIL (`apply_arrow_key_selection`, `apply_hierarchy_click_selection`, and `apply_hierarchy_arrow_selection` not defined).

- [ ] **Step 3: Implement range selection helpers and UI bindings in `src/ui/signal_list.rs`, `src/ui/hierarchy_modal.rs`, and `src/ui/mod.rs`**

In `src/ui/hierarchy_modal.rs`:
- Render each signal row in `visible_sigs` with a selection highlight background when `app.checked_signals.contains(&sid)`.
- Detect `ctrl_or_cmd` and `shift` modifiers via `ui.input(|i| (i.modifiers.command || i.modifiers.ctrl, i.modifiers.shift))`.
- When either the checkbox is clicked or the signal label is clicked, call `apply_hierarchy_click_selection(&mut app.checked_signals, &mut app.hierarchy_anchor_signal, &mut app.hierarchy_focus_signal, sid, is_checkbox_click, ctrl_or_cmd, shift, &visible_sigs)`.
- When the hierarchy modal is open and `!ctx.wants_keyboard_input()`, handle `Key::ArrowDown` / `Key::ArrowUp` via `apply_hierarchy_arrow_selection` and `Cmd/Ctrl+A` to select all `visible_sigs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/signal_list.rs src/ui/hierarchy_modal.rs src/ui/mod.rs
git commit -m "feat: support Shift+Click and Shift+Up/Down range selection in hierarchy modal and signal list"
```

---

### Task 2: Synopsys nWave Zoom Shortcuts (`z`, `Z`, `f`, `g`) & Toolbar Signal Hierarchy Button (`src/ui/mod.rs`, `src/ui/toolbar.rs`)

**Files:**
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/toolbar.rs`

**Interfaces:**
- Produces (`src/ui/mod.rs`):
  - `pub fn handle_waveform_key_shortcut(&mut self, key: egui::Key, shift: bool, cmd_or_ctrl: bool) -> bool`
    - `Key::Z` with `!cmd_or_ctrl && shift` (`Z`): Zoom In (`self.doc.viewport.zoom_factor(0.5, 0.5, max_t)`)
    - `Key::Z` with `!cmd_or_ctrl && !shift` (`z`): Zoom Out (`self.doc.viewport.zoom_factor(2.0, 0.5, max_t)`)
    - `Key::F` with `!cmd_or_ctrl` (`f` / `F`): Zoom Full (`self.doc.viewport.zoom_full(self.index.min_timestamp, self.index.max_timestamp)`)
    - `Key::G` with `!cmd_or_ctrl` (`g` / `G`): Reveal Signal Hierarchy window (`self.reveal_hierarchy_window()`)
  - `pub fn reveal_hierarchy_window(&mut self)` (sets `self.show_hierarchy_modal = true` and initializes `self.selected_scope` to the first root scope if `None`)

- [ ] **Step 1: Write failing unit tests in `src/ui/mod.rs` for Synopsys nWave keyboard shortcuts (`z` zoom out, `Z` zoom in, `f` zoom full) and revealing the hierarchy window (Review Focus #1 & #4)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nwave_zoom_keys_z_shift_z_and_f() {
        let mut app = VcdViewerApp::default();
        app.index.min_timestamp = 0;
        app.index.max_timestamp = 1000;
        app.doc.viewport.start_time = 200.0;
        app.doc.viewport.end_time = 600.0; // span = 400.0

        // 'Z' (Shift + Z) -> Zoom In (span halves from 400 to 200)
        assert!(app.handle_waveform_key_shortcut(egui::Key::Z, true, false));
        assert!((app.doc.viewport.span() - 200.0).abs() < 1e-6);

        // 'z' (unshifted z) -> Zoom Out (span doubles from 200 to 400)
        assert!(app.handle_waveform_key_shortcut(egui::Key::Z, false, false));
        assert!((app.doc.viewport.span() - 400.0).abs() < 1e-6);

        // 'f' -> Zoom Full ([0, 1000])
        assert!(app.handle_waveform_key_shortcut(egui::Key::F, false, false));
        assert!((app.doc.viewport.start_time - 0.0).abs() < 1e-6);
        assert!((app.doc.viewport.end_time - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn test_reveal_hierarchy_window_sets_modal_and_default_scope() {
        let mut app = VcdViewerApp::default();
        app.show_hierarchy_modal = false;
        app.selected_scope = None;

        app.reveal_hierarchy_window();
        assert!(app.show_hierarchy_modal);
        assert!(app.selected_scope.is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ui::tests`
Expected: FAIL (`handle_waveform_key_shortcut` and `reveal_hierarchy_window` not defined).

- [ ] **Step 3: Implement `handle_waveform_key_shortcut`, `reveal_hierarchy_window`, and the toolbar `🌳 Signal Hierarchy` button in `src/ui/mod.rs` and `src/ui/toolbar.rs`**

In `src/ui/toolbar.rs`:
- Place a prominent `"🌳 Signal Hierarchy"` button directly next to `"📂 Load VCD"` (with tooltip `"Reveal Signal Hierarchy window of loaded VCD (G)"`) that calls `app.reveal_hierarchy_window()`.
- Update the Zoom button tooltips to advertise the Synopsys nWave keys:
  - `"🔍+ In"` -> `"Zoom In (Shift+Z or +)"`
  - `"🔍- Out"` -> `"Zoom Out (z or -)"`
  - `"⛶ Full"` -> `"Zoom Full (f)"`

- [ ] **Step 4: Run all tests and verify native + `wasm32` builds**

Run:
```bash
cargo test
cargo check --release
cargo check --target wasm32-unknown-unknown
```
Expected: All tests PASS and zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs src/ui/toolbar.rs
git commit -m "feat: add Synopsys nWave zoom shortcuts (Z/z/f) and Signal Hierarchy toolbar button"
```
