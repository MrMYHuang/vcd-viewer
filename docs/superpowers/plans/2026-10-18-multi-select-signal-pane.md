# Signal Pane Multi-Selection, Keyboard Shortcuts & Batch Context Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add multi-signal selection (`Ctrl/Cmd+Click`, `Shift+Click`, `Cmd/Ctrl+A`), clipboard cut/copy/paste/delete shortcuts (`Cmd/Ctrl+X`, `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, `Delete`/`Backspace`), and batch right-click context menu actions (radix, IEEE 754 float, signedness, waveform render style, trace color, expand/collapse bits, copy/cut/delete) to the Signal List pane.

**Architecture:** Extend `SignalLocation` (`Ord`, `PartialOrd`, `Hash`) and add a `BTreeSet<SignalLocation>` selection model plus batch mutation methods on `SignalListDocument` (`src/project.rs`). Wire keyboard shortcuts (`Cmd/Ctrl+A`, `Cmd/Ctrl+X`, `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, `Delete`/`Backspace`), modifier-aware mouse clicks (`Ctrl/Cmd+Click`, `Shift+Click`), and right-click target resolution in `src/ui/mod.rs` and `src/ui/signal_list.rs` so right-clicking any selected signal applies menu operations across all selected signals.

**Tech Stack:** Rust (2021 edition), `egui` / `eframe` `0.29`.

**Spec:** `docs/superpowers/specs/2026-10-17-vcd-viewer-design.md` (§3.1.2 & §3.3)

## Global Constraints

- **Cross-Platform Modifiers:** Support both `Cmd` (macOS `Modifiers::command`) and `Ctrl` (Windows/Linux/macOS `Modifiers::ctrl` or `Modifiers::command`) for Select All (`A`), Cut (`X`), Copy (`C`), Paste (`V`), and multi-select toggle click (`modifiers.command || modifiers.ctrl`).
- **Right-Click Preserves Multi-Selection:** Right-clicking a signal that is already part of `selected_locations` must keep the entire multi-selection intact and apply context menu actions to all selected signals; right-clicking an unselected signal selects that single signal first and opens the menu for it.
- **Index-Safe Batch Deletion & Cut:** Deleting or cutting multiple `SignalLocation`s (including multiple children inside the same `Group` or multiple top-level rows) must remove items in reverse `(node_idx, group_child_idx)` order so earlier removals never shift or invalidate later indices.

## Review Focus

1. **Reverse-order batch deletion across top-level rows and groups:** Deleting `{ node_idx: 0 }` and `{ node_idx: 2 }` or `{ node_idx: 1, group_child_idx: Some(0) }` and `{ node_idx: 1, group_child_idx: Some(2) }` in ascending order would shift indices and delete the wrong signal or panic; `delete_locations` must sort and remove in strictly descending order.
2. **Right-click on an unselected row vs. already-selected row:** If signals A, B, and C are selected and the user right-clicks B, all three (A, B, C) must stay selected and receive the radix/color/delete change; if the user right-clicks unselected signal D, selection must switch to `{D}` alone before applying the menu action.
3. **`Ctrl+Click` on macOS vs Windows/Linux:** The user specifically asked for `ctrl+click to select multiple items` as well as `cmd+a` / `cmd+x`; checking `modifiers.ctrl || modifiers.command` for toggle-click ensures both `Ctrl+Click` and `Cmd+Click` toggle individual items on macOS, Windows, and Linux.
4. **Keyboard shortcut focus guard:** Pressing `Backspace`/`Delete` or `Cmd+A`/`Cmd+X` while typing inside the toolbar's `Find Value` text box, the Hierarchy Browser's search box, or the context menu's `Rename Alias` input must edit the text field rather than deleting or cutting signals (`!ctx.wants_keyboard_input()`).
5. **Batch paste ordering:** Cutting (`Cmd/Ctrl+X`) or copying (`Cmd/Ctrl+C`) multiple signals and pasting (`Cmd/Ctrl+V` or context menu `Paste`) must insert the signals in their original top-to-bottom visual order, not reversed.

---

## File Structure

- Modify: `src/project.rs` — Derive `PartialOrd`, `Ord`, `Hash` on `SignalLocation`; add `all_signal_locations()`, `delete_locations()`, `take_locations()` (for cut), `insert_signals_after()`, and `update_signals()` batch methods on `SignalListDocument`.
- Modify: `src/ui/mod.rs` — Replace single `selected_location: Option<SignalLocation>` and `clipboard_signal: Option<SignalRowConfig>` with `selected_locations: BTreeSet<SignalLocation>`, `primary_selected: Option<SignalLocation>`, and `clipboard_signals: Vec<SignalRowConfig>`; handle `Cmd/Ctrl+A`, `Cmd/Ctrl+X`, `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, and `Delete`/`Backspace` shortcuts.
- Modify: `src/ui/signal_list.rs` — Handle plain click, `Ctrl/Cmd+Click` toggle, `Shift+Click` range selection, right-click target set resolution, and batch context menu actions (Copy, Cut, Paste, Delete, Expand/Collapse Bits, Radix, IEEE 754 Float, Signed/Unsigned, Waveform Style, Trace Color) across all target `SignalLocation`s.
- Modify: `src/ui/waveform_canvas.rs` — Highlight all rows in `app.selected_locations` on the waveform canvas.

---

### Task 1: Multi-Selection & Batch Operations on `SignalListDocument` (`src/project.rs`)

**Files:**
- Modify: `src/project.rs`

**Interfaces:**
- Produces (`src/project.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct SignalLocation { pub node_idx: usize, pub group_child_idx: Option<usize> }`
  - Methods on `SignalListDocument`:
    - `pub fn all_signal_locations(&self) -> std::collections::BTreeSet<SignalLocation>` (returns all signal locations across top-level rows and non-collapsed/collapsed groups)
    - `pub fn delete_locations(&mut self, locs: &std::collections::BTreeSet<SignalLocation>)` (deletes in reverse order `.iter().rev()` so indices never shift)
    - `pub fn cut_locations(&mut self, locs: &std::collections::BTreeSet<SignalLocation>) -> Vec<SignalRowConfig>` (clones configs in ascending visual order, then calls `delete_locations(locs)`)
    - `pub fn copy_locations(&self, locs: &std::collections::BTreeSet<SignalLocation>) -> Vec<SignalRowConfig>` (clones configs in ascending visual order)
    - `pub fn insert_signals_after(&mut self, anchor: Option<SignalLocation>, configs: &[SignalRowConfig])` (inserts `configs` preserving their top-to-bottom order)
    - `pub fn update_signals<F: FnMut(&mut SignalRowConfig)>(&mut self, locs: &std::collections::BTreeSet<SignalLocation>, f: F)`

- [ ] **Step 1: Write failing tests in `src/project.rs` for `all_signal_locations`, reverse-order `delete_locations` & `cut_locations` (Review Focus #1 & #5), and batch `update_signals`**

```rust
    #[test]
    fn test_batch_select_cut_paste_and_radix_update() {
        use std::collections::BTreeSet;
        let mut doc = SignalListDocument {
            rows: vec![
                DisplayedNode::Signal(SignalRowConfig::new("top.a", 8)),
                DisplayedNode::Group {
                    name: "G1".into(),
                    collapsed: false,
                    children: vec![
                        SignalRowConfig::new("top.g_b", 16),
                        SignalRowConfig::new("top.g_c", 16),
                        SignalRowConfig::new("top.g_d", 16),
                    ],
                },
                DisplayedNode::Signal(SignalRowConfig::new("top.e", 32)),
            ],
            ..Default::default()
        };

        let all = doc.all_signal_locations();
        assert_eq!(all.len(), 5);

        // Select top.a (0, None), top.g_b (1, Some(0)), top.g_d (1, Some(2)), and top.e (2, None)
        let sel: BTreeSet<SignalLocation> = [
            SignalLocation { node_idx: 0, group_child_idx: None },
            SignalLocation { node_idx: 1, group_child_idx: Some(0) },
            SignalLocation { node_idx: 1, group_child_idx: Some(2) },
            SignalLocation { node_idx: 2, group_child_idx: None },
        ]
        .into_iter()
        .collect();

        // Batch update radix to Dec and signedness to Signed2s
        doc.update_signals(&sel, |cfg| {
            cfg.radix = Radix::Dec;
            cfg.signedness = Signedness::Signed2s;
        });
        assert_eq!(
            doc.get_signal(SignalLocation { node_idx: 1, group_child_idx: Some(2) }).unwrap().radix,
            Radix::Dec
        );
        // Unselected top.g_c (1, Some(1)) remains Hex
        assert_eq!(
            doc.get_signal(SignalLocation { node_idx: 1, group_child_idx: Some(1) }).unwrap().radix,
            Radix::Hex
        );

        // Cut selected items: must return 4 items in ascending order and leave only Group G1 with [top.g_c]
        let cut = doc.cut_locations(&sel);
        let cut_paths: Vec<&str> = cut.iter().map(|c| c.signal_path.as_str()).collect();
        assert_eq!(cut_paths, vec!["top.a", "top.g_b", "top.g_d", "top.e"]);
        assert_eq!(doc.all_signal_locations().len(), 1);
        assert_eq!(
            doc.get_signal(SignalLocation { node_idx: 0, group_child_idx: Some(0) })
                .unwrap()
                .signal_path,
            "top.g_c"
        );

        // Paste cut items after remaining signal: preserves order
        doc.insert_signals_after(
            Some(SignalLocation { node_idx: 0, group_child_idx: Some(0) }),
            &cut,
        );
        assert_eq!(doc.all_signal_locations().len(), 5);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib project::tests::test_batch_select_cut_paste_and_radix_update`
Expected: FAIL (missing `Ord` on `SignalLocation` and batch methods on `SignalListDocument`).

- [ ] **Step 3: Implement `Ord` on `SignalLocation` and batch methods on `SignalListDocument` in `src/project.rs`**

In `delete_locations`, iterate `locs.iter().rev()`. Because `BTreeSet<SignalLocation>` orders by `(node_idx, group_child_idx)`, iterating in reverse processes higher `node_idx` first and higher `Some(c_idx)` before lower `Some(c_idx)` within the same group. In `insert_signals_after`, iterate `configs.iter().rev()` when inserting at a fixed index `pos` (or advance `pos` by `1` on each insertion) so pasted items appear in their original order.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib project`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/project.rs
git commit -m "feat: add multi-signal selection set and batch update/cut/copy/delete/paste on SignalListDocument"
```

---

### Task 2: App State, Keyboard Shortcuts (`Cmd/Ctrl+A`, `Cmd/Ctrl+X`, `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, `Del`) & Batch Right-Click Context Menu (`src/ui/mod.rs`, `src/ui/signal_list.rs`, `src/ui/waveform_canvas.rs`)

**Files:**
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/signal_list.rs`
- Modify: `src/ui/waveform_canvas.rs`

**Interfaces:**
- Consumes: `SignalLocation`, `SignalListDocument` batch methods (`crate::project`).
- Produces:
  - Updated fields on `VcdViewerApp` (`src/ui/mod.rs`):
    - `pub selected_locations: std::collections::BTreeSet<SignalLocation>`
    - `pub primary_selected: Option<SignalLocation>` (anchor for `Shift+Click` range selection and `Prev/Next Edge` / `Find Value` navigation)
    - `pub clipboard_signals: Vec<SignalRowConfig>`
  - Helper methods on `VcdViewerApp`:
    - `pub fn select_all_signals(&mut self)`
    - `pub fn cut_selected_signals(&mut self)`
    - `pub fn copy_selected_signals(&mut self)`
    - `pub fn paste_clipboard_signals(&mut self)`
    - `pub fn delete_selected_signals(&mut self)`
  - Pure selection helper in `src/ui/signal_list.rs`:
    - `pub fn apply_row_click_selection(selected: &mut BTreeSet<SignalLocation>, primary: &mut Option<SignalLocation>, clicked_loc: SignalLocation, ctrl_or_cmd: bool, shift: bool, ordered_visible_locs: &[SignalLocation])`
    - `pub fn resolve_context_menu_targets(selected: &mut BTreeSet<SignalLocation>, primary: &mut Option<SignalLocation>, right_clicked_loc: SignalLocation) -> BTreeSet<SignalLocation>`

- [ ] **Step 1: Write failing unit tests in `src/ui/signal_list.rs` for `Ctrl/Cmd+Click` toggle, `Shift+Click` range selection, and right-click target resolution (Review Focus #2 & #3)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_ctrl_click_shift_click_and_right_click_targets() {
        let locs = vec![
            SignalLocation { node_idx: 0, group_child_idx: None },
            SignalLocation { node_idx: 1, group_child_idx: None },
            SignalLocation { node_idx: 2, group_child_idx: None },
            SignalLocation { node_idx: 3, group_child_idx: None },
        ];
        let mut selected = BTreeSet::new();
        let mut primary = None;

        // 1. Plain click on index 0 selects only {0}
        apply_row_click_selection(&mut selected, &mut primary, locs[0], false, false, &locs);
        assert_eq!(selected.len(), 1);
        assert!(selected.contains(&locs[0]));

        // 2. Ctrl+Click on index 2 adds {2} -> {0, 2}
        apply_row_click_selection(&mut selected, &mut primary, locs[2], true, false, &locs);
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&locs[0]) && selected.contains(&locs[2]));

        // 3. Right-click on already-selected index 0 preserves {0, 2}
        let targets = resolve_context_menu_targets(&mut selected, &mut primary, locs[0]);
        assert_eq!(targets.len(), 2);

        // 4. Right-click on unselected index 3 switches selection to {3}
        let targets2 = resolve_context_menu_targets(&mut selected, &mut primary, locs[3]);
        assert_eq!(targets2.len(), 1);
        assert!(targets2.contains(&locs[3]));

        // 5. Shift+Click from primary (3) to index 1 selects {1, 2, 3}
        apply_row_click_selection(&mut selected, &mut primary, locs[1], false, true, &locs);
        assert_eq!(selected.len(), 3);
        assert!(selected.contains(&locs[1]) && selected.contains(&locs[2]) && selected.contains(&locs[3]));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib signal_list::tests::test_ctrl_click_shift_click_and_right_click_targets`
Expected: FAIL

- [ ] **Step 3: Implement multi-selection, keyboard shortcuts (`Cmd/Ctrl+A`, `Cmd/Ctrl+X`, `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, `Del`), and batch context menu in `src/ui/mod.rs`, `src/ui/signal_list.rs`, and `src/ui/waveform_canvas.rs`**

- In `src/ui/mod.rs`:
  - Handle `egui::Event::Cut`, `egui::Event::Copy`, `egui::Event::Paste` as well as key presses with `i.modifiers.command || i.modifiers.ctrl` for `Key::A` (`select_all_signals`), `Key::X` (`cut_selected_signals`), `Key::C` (`copy_selected_signals`), `Key::V` (`paste_clipboard_signals`), and `Key::Delete` / `Key::Backspace` (`delete_selected_signals`), guarded by `!ctx.wants_keyboard_input()` (Review Focus #4).
  - Update `+Group` button in `src/ui/signal_list.rs` so clicking `+Group` moves all currently selected signals into the newly created group.
- In `src/ui/signal_list.rs`:
  - Show the count of selected signals in the context menu header when `targets.len() > 1` (e.g., `"3 signals selected"`).
  - Support `📋 Copy Selected (N)`, `✂ Cut Selected (N)`, `📄 Paste Below (M)`, `🗑 Delete Selected (N)`, `↕ Expand / Collapse Bits`, `🔢 Radix`, `🧮 IEEE 754 Float`, `➕ Signed / Unsigned`, `📈 Waveform Style`, and `🎨 Trace Color` across all `targets` via `app.doc.update_signals(&targets, |cfg| ...)`.
- In `src/ui/waveform_canvas.rs`:
  - Highlight any `VisibleRow::Signal` whose `loc` is in `app.selected_locations`.

- [ ] **Step 4: Run all tests and check native + `wasm32` builds**

Run:
```bash
cargo test
cargo check --release
cargo check --target wasm32-unknown-unknown
```
Expected: All tests PASS and zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/ui/
git commit -m "feat: add multi-select (Cmd/Ctrl+A, Ctrl+Click, Shift+Click), Cut/Copy/Paste/Del shortcuts, and batch right-click menu"
```
