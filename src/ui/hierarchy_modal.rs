use crate::project::{DisplayedNode, SignalRowConfig};
use crate::ui::VcdViewerApp;
use crate::vcd_header::{ScopeId, SignalId, VcdHeader};
use std::collections::HashSet;
use std::sync::Arc;

pub fn apply_hierarchy_click_selection(
    checked: &mut HashSet<SignalId>,
    anchor: &mut Option<SignalId>,
    focus: &mut Option<SignalId>,
    clicked_id: SignalId,
    is_checkbox_click: bool,
    ctrl_or_cmd: bool,
    shift: bool,
    visible_sigs: &[SignalId],
) {
    if shift {
        let anc = anchor.unwrap_or(clicked_id);
        *anchor = Some(anc);
        *focus = Some(clicked_id);
        if let (Some(i0), Some(i1)) = (
            visible_sigs.iter().position(|&s| s == anc),
            visible_sigs.iter().position(|&s| s == clicked_id),
        ) {
            if !is_checkbox_click && !ctrl_or_cmd {
                checked.clear();
            }
            let start = i0.min(i1);
            let end = i0.max(i1);
            for &s in &visible_sigs[start..=end] {
                checked.insert(s);
            }
            return;
        }
    }

    if is_checkbox_click || ctrl_or_cmd {
        if checked.contains(&clicked_id) {
            checked.remove(&clicked_id);
        } else {
            checked.insert(clicked_id);
        }
        *anchor = Some(clicked_id);
        *focus = Some(clicked_id);
    } else {
        checked.clear();
        checked.insert(clicked_id);
        *anchor = Some(clicked_id);
        *focus = Some(clicked_id);
    }
}

pub fn apply_hierarchy_arrow_selection(
    checked: &mut HashSet<SignalId>,
    anchor: &mut Option<SignalId>,
    focus: &mut Option<SignalId>,
    move_down: bool,
    shift: bool,
    visible_sigs: &[SignalId],
) {
    if visible_sigs.is_empty() {
        return;
    }
    let cur_sid = focus.or(*anchor);
    let cur_idx = cur_sid.and_then(|c| visible_sigs.iter().position(|&s| s == c));
    let next_idx = match cur_idx {
        Some(idx) => {
            if move_down {
                (idx + 1).min(visible_sigs.len() - 1)
            } else {
                idx.saturating_sub(1)
            }
        }
        None => {
            if move_down {
                0
            } else {
                visible_sigs.len() - 1
            }
        }
    };
    let new_sid = visible_sigs[next_idx];
    *focus = Some(new_sid);

    if shift {
        let anc = anchor.unwrap_or(new_sid);
        *anchor = Some(anc);
        if let Some(a_idx) = visible_sigs.iter().position(|&s| s == anc) {
            checked.clear();
            let start = a_idx.min(next_idx);
            let end = a_idx.max(next_idx);
            for &s in &visible_sigs[start..=end] {
                checked.insert(s);
            }
            return;
        }
    }

    checked.clear();
    checked.insert(new_sid);
    *anchor = Some(new_sid);
}

fn collect_descendant_signals(header: &VcdHeader, scope_id: ScopeId, out: &mut Vec<SignalId>) {
    if let Some(scope) = header.scopes.get(scope_id.0) {
        out.extend(scope.signals.iter().copied());
        for &child in &scope.children {
            collect_descendant_signals(header, child, out);
        }
    }
}

pub fn filter_scope_signals(header: &VcdHeader, scope_id: ScopeId, query: &str) -> Vec<SignalId> {
    let Some(scope) = header.scopes.get(scope_id.0) else {
        return Vec::new();
    };
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return scope.signals.clone();
    }
    let mut candidates = Vec::new();
    collect_descendant_signals(header, scope_id, &mut candidates);
    candidates
        .into_iter()
        .filter(|&sid| {
            let sig = &header.signals[sid.0];
            sig.name.to_ascii_lowercase().contains(&q)
                || sig.full_path.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

fn render_scope_tree_node(
    ui: &mut egui::Ui,
    header: &VcdHeader,
    scope_id: ScopeId,
    selected_scope: &mut Option<ScopeId>,
) {
    let scope = &header.scopes[scope_id.0];
    let is_selected = *selected_scope == Some(scope_id);
    let label = format!("📁 {} ({})", scope.name, scope.signals.len());

    if scope.children.is_empty() {
        if ui.selectable_label(is_selected, label).clicked() {
            *selected_scope = Some(scope_id);
        }
    } else {
        let id = ui.make_persistent_id(("scope_node", scope_id.0));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                if ui.selectable_label(is_selected, label).clicked() {
                    *selected_scope = Some(scope_id);
                }
            })
            .body(|ui| {
                for &child_id in &scope.children {
                    render_scope_tree_node(ui, header, child_id, selected_scope);
                }
            });
    }
}

pub fn render_hierarchy_modal(app: &mut VcdViewerApp, ctx: &egui::Context) {
    if !app.show_hierarchy_modal {
        return;
    }

    let Some(header_arc) = app.header.as_ref().map(Arc::clone) else {
        return;
    };

    let mut open = app.show_hierarchy_modal;
    let mut signals_to_add: Vec<SignalId> = Vec::new();

    egui::Window::new("Signal Hierarchy Browser")
        .open(&mut open)
        .default_size([720.0, 480.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔍 Filter Signals:");
                ui.add(
                    egui::TextEdit::singleline(&mut app.hierarchy_search_query)
                        .desired_width(240.0)
                        .hint_text("Filter by signal or module name..."),
                );
                if ui.button("Clear").clicked() {
                    app.hierarchy_search_query.clear();
                }
                ui.separator();
                ui.label(
                    egui::RichText::new("Tip: Shift+Click or Shift+↑/↓ to select range")
                        .weak()
                        .small(),
                );
            });
            ui.separator();

            ui.columns(2, |cols| {
                // Left column: Expandable/collapsible Design Hierarchy Tree
                cols[0].heading("Design Hierarchy");
                cols[0].add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("hierarchy_tree_scroll")
                    .max_height(340.0)
                    .show(&mut cols[0], |ui| {
                        for &root_id in &header_arc.root_scopes {
                            render_scope_tree_node(
                                ui,
                                &header_arc,
                                root_id,
                                &mut app.selected_scope,
                            );
                        }
                    });

                // Right column: Signals in selected scope (or global search if query non-empty)
                let active_scope = app
                    .selected_scope
                    .or_else(|| header_arc.root_scopes.first().copied());
                let visible_sigs: Vec<SignalId> = if let Some(sid) = active_scope {
                    filter_scope_signals(&header_arc, sid, &app.hierarchy_search_query)
                } else {
                    Vec::new()
                };

                // Keyboard navigation (Up/Down/Shift+Up/Shift+Down and Cmd/Ctrl+A) in hierarchy signal list
                if !ctx.wants_keyboard_input() {
                    ctx.input(|i| {
                        let cmd_or_ctrl = i.modifiers.command || i.modifiers.ctrl;
                        if cmd_or_ctrl && i.key_pressed(egui::Key::A) {
                            for &sid in &visible_sigs {
                                app.checked_signals.insert(sid);
                            }
                        } else if i.key_pressed(egui::Key::ArrowDown) {
                            apply_hierarchy_arrow_selection(
                                &mut app.checked_signals,
                                &mut app.hierarchy_anchor_signal,
                                &mut app.hierarchy_focus_signal,
                                true,
                                i.modifiers.shift,
                                &visible_sigs,
                            );
                        } else if i.key_pressed(egui::Key::ArrowUp) {
                            apply_hierarchy_arrow_selection(
                                &mut app.checked_signals,
                                &mut app.hierarchy_anchor_signal,
                                &mut app.hierarchy_focus_signal,
                                false,
                                i.modifiers.shift,
                                &visible_sigs,
                            );
                        }
                    });
                }

                cols[1].horizontal(|ui| {
                    ui.heading(format!("Signals ({})", visible_sigs.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("+ Add All in Scope").clicked() {
                            signals_to_add.extend(visible_sigs.iter().copied());
                        }
                        if ui
                            .button(format!("+ Add Selected ({})", app.checked_signals.len()))
                            .clicked()
                        {
                            let mut sorted: Vec<SignalId> =
                                app.checked_signals.drain().collect();
                            sorted.sort_by_key(|s| s.0);
                            signals_to_add.extend(sorted);
                        }
                    });
                });
                cols[1].add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("scope_signals_scroll")
                    .max_height(340.0)
                    .show(&mut cols[1], |ui| {
                        for &sid in &visible_sigs {
                            let meta = &header_arc.signals[sid.0];
                            let is_checked = app.checked_signals.contains(&sid);
                            let range_str = match (meta.msb, meta.lsb) {
                                (Some(m), Some(l)) if m != l => format!(" [{}:{}]", m, l),
                                _ if meta.width > 1 => format!(" [{}:0]", meta.width - 1),
                                _ => String::new(),
                            };
                            let label = format!("{}{} ({:?})", meta.name, range_str, meta.var_type);
                            ui.horizontal(|ui| {
                                let mut box_val = is_checked;
                                let cb_resp = ui.checkbox(&mut box_val, "");
                                let lbl_resp = ui.selectable_label(is_checked, label);
                                if cb_resp.clicked() || lbl_resp.clicked() {
                                    let (ctrl_or_cmd, shift) = ui.input(|i| {
                                        (i.modifiers.command || i.modifiers.ctrl, i.modifiers.shift)
                                    });
                                    apply_hierarchy_click_selection(
                                        &mut app.checked_signals,
                                        &mut app.hierarchy_anchor_signal,
                                        &mut app.hierarchy_focus_signal,
                                        sid,
                                        cb_resp.clicked(),
                                        ctrl_or_cmd,
                                        shift,
                                        &visible_sigs,
                                    );
                                }
                                if ui.small_button("Add").on_hover_text(&meta.full_path).clicked() {
                                    signals_to_add.push(sid);
                                }
                            });
                        }
                    });
            });
        });

    app.show_hierarchy_modal = open;

    if !signals_to_add.is_empty() {
        for sid in signals_to_add {
            let meta = &header_arc.signals[sid.0];
            app.doc
                .rows
                .push(DisplayedNode::Signal(SignalRowConfig::new(
                    meta.full_path.clone(),
                    meta.width,
                )));
        }
        app.sync_watched_signals();
    }
}

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

    #[test]
    fn test_filter_signals_searches_descendant_scopes_when_query_non_empty() {
        let vcd = b"$timescale 1ns $end
$scope module top_soc $end
  $scope module u_cpu $end
    $var wire 1 ! clk $end
    $var reg 32 \" pc [31:0] $end
  $upscope $end
$upscope $end
$enddefinitions $end";
        let hdr = parse_vcd_header(&MemByteSource::new(vcd.to_vec())).unwrap();
        // Root scope top_soc has 0 direct signals, but searching "pc" from root must find top_soc.u_cpu.pc
        let matched = filter_scope_signals(&hdr, hdr.root_scopes[0], "pc");
        assert_eq!(matched.len(), 1);
        assert_eq!(hdr.signals[matched[0].0].full_path, "top_soc.u_cpu.pc");
    }

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
}
