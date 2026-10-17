use crate::project::{DisplayedNode, SignalRowConfig};
use crate::ui::VcdViewerApp;
use crate::vcd_header::{ScopeId, SignalId, VcdHeader};
use std::sync::Arc;

pub fn filter_scope_signals(header: &VcdHeader, scope_id: ScopeId, query: &str) -> Vec<SignalId> {
    let Some(scope) = header.scopes.get(scope_id.0) else {
        return Vec::new();
    };
    let q = query.trim().to_ascii_lowercase();
    scope
        .signals
        .iter()
        .copied()
        .filter(|&sid| {
            if q.is_empty() {
                return true;
            }
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
        .default_size([680.0, 460.0])
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
                        for sid in visible_sigs {
                            let meta = &header_arc.signals[sid.0];
                            let mut is_checked = app.checked_signals.contains(&sid);
                            let range_str = match (meta.msb, meta.lsb) {
                                (Some(m), Some(l)) if m != l => format!(" [{}:{}]", m, l),
                                _ if meta.width > 1 => format!(" [{}:0]", meta.width - 1),
                                _ => String::new(),
                            };
                            let label = format!("{}{} ({:?})", meta.name, range_str, meta.var_type);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut is_checked, label).changed() {
                                    if is_checked {
                                        app.checked_signals.insert(sid);
                                    } else {
                                        app.checked_signals.remove(&sid);
                                    }
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
}
