use crate::format::{format_signal_value, FloatFormat, Radix, Signedness};
use crate::project::{DisplayedNode, RenderStyle, SignalLocation, VisibleRow};
use crate::ui::VcdViewerApp;
use crate::waveform_db::LogicState;
use std::collections::BTreeSet;

const PRESET_COLORS: [(&str, [u8; 3]); 7] = [
    ("Green", [166, 227, 161]),
    ("Blue", [137, 180, 250]),
    ("Yellow", [249, 226, 175]),
    ("Orange", [250, 179, 135]),
    ("Pink", [243, 139, 168]),
    ("Purple", [203, 166, 247]),
    ("Cyan", [148, 226, 213]),
];

pub fn apply_row_click_selection(
    selected: &mut BTreeSet<SignalLocation>,
    primary: &mut Option<SignalLocation>,
    clicked_loc: SignalLocation,
    ctrl_or_cmd: bool,
    shift: bool,
    ordered_visible_locs: &[SignalLocation],
) {
    if shift {
        if let Some(anchor) = *primary {
            if let (Some(i0), Some(i1)) = (
                ordered_visible_locs.iter().position(|&l| l == anchor),
                ordered_visible_locs.iter().position(|&l| l == clicked_loc),
            ) {
                if !ctrl_or_cmd {
                    selected.clear();
                }
                let start = i0.min(i1);
                let end = i0.max(i1);
                for &loc in &ordered_visible_locs[start..=end] {
                    selected.insert(loc);
                }
                return;
            }
        }
    }
    if ctrl_or_cmd {
        if selected.contains(&clicked_loc) {
            selected.remove(&clicked_loc);
            if *primary == Some(clicked_loc) {
                *primary = selected.iter().next().copied();
            }
        } else {
            selected.insert(clicked_loc);
            *primary = Some(clicked_loc);
        }
    } else {
        selected.clear();
        selected.insert(clicked_loc);
        *primary = Some(clicked_loc);
    }
}

pub fn resolve_context_menu_targets(
    selected: &mut BTreeSet<SignalLocation>,
    primary: &mut Option<SignalLocation>,
    right_clicked_loc: SignalLocation,
) -> BTreeSet<SignalLocation> {
    if !selected.contains(&right_clicked_loc) {
        selected.clear();
        selected.insert(right_clicked_loc);
        *primary = Some(right_clicked_loc);
    } else if primary.is_none() {
        *primary = Some(right_clicked_loc);
    }
    selected.clone()
}

pub fn render_header_columns(app: &mut VcdViewerApp, ui: &mut egui::Ui, header_height: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(app.signal_col_width, header_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(6.0);
            ui.strong("Signals");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                if ui.small_button("+Divider").on_hover_text("Add section divider").clicked() {
                    app.doc.rows.push(DisplayedNode::Divider {
                        label: "Bus Section".to_string(),
                    });
                }
                if ui
                    .small_button("+Group")
                    .on_hover_text("Group selected signals")
                    .clicked()
                {
                    let children = if !app.selected_locations.is_empty() {
                        let cut = app.doc.cut_locations(&app.selected_locations);
                        app.selected_locations.clear();
                        app.primary_selected = None;
                        cut
                    } else {
                        Vec::new()
                    };
                    app.doc.rows.push(DisplayedNode::Group {
                        name: format!("Group {}", app.doc.rows.len() + 1),
                        collapsed: false,
                        children,
                    });
                }
            });
        },
    );

    ui.separator();

    ui.allocate_ui_with_layout(
        egui::vec2(app.value_col_width, header_height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add_space(8.0);
            ui.strong(format!("Value @ {}", app.format_time(app.doc.cursor_time)));
        },
    );
}

pub fn render_signal_and_value_panes(
    app: &mut VcdViewerApp,
    ui: &mut egui::Ui,
    visible_rows: &[VisibleRow],
    row_height: f32,
) {
    let mut action_copy_selected = false;
    let mut action_cut_selected = false;
    let mut action_delete_selected = false;
    let mut action_paste_after: Option<SignalLocation> = None;
    let mut action_group_selected = false;
    let mut action_toggle_group: Option<usize> = None;
    let mut action_delete_node: Option<usize> = None;
    let mut action_move_up: Option<usize> = None;
    let mut action_move_down: Option<usize> = None;

    let ordered_visible_locs: Vec<SignalLocation> = visible_rows
        .iter()
        .filter_map(|r| match r {
            VisibleRow::Signal { loc, .. } => Some(*loc),
            _ => None,
        })
        .collect();

    let total_width = app.signal_col_width + app.value_col_width + 8.0;
    ui.allocate_ui_with_layout(
        egui::vec2(total_width, visible_rows.len() as f32 * row_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            for vrow in visible_rows {
                let (row_rect, _) = ui.allocate_exact_size(
                    egui::vec2(total_width, row_height),
                    egui::Sense::hover(),
                );

                let sig_rect = egui::Rect::from_min_size(
                    row_rect.min,
                    egui::vec2(app.signal_col_width, row_height),
                );
                let val_rect = egui::Rect::from_min_size(
                    egui::pos2(row_rect.min.x + app.signal_col_width + 6.0, row_rect.min.y),
                    egui::vec2(app.value_col_width, row_height),
                );

                // Subtle bottom row border
                ui.painter().line_segment(
                    [
                        egui::pos2(row_rect.min.x, row_rect.max.y),
                        egui::pos2(row_rect.max.x, row_rect.max.y),
                    ],
                    egui::Stroke::new(1.0_f32, ui.visuals().faint_bg_color),
                );
                // Vertical separator between Signal and Value
                ui.painter().line_segment(
                    [
                        egui::pos2(sig_rect.max.x + 2.0, row_rect.min.y),
                        egui::pos2(sig_rect.max.x + 2.0, row_rect.max.y),
                    ],
                    egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
                );

                match vrow {
                    VisibleRow::GroupHeader {
                        node_idx,
                        name,
                        collapsed,
                        child_count,
                    } => {
                        ui.painter()
                            .rect_filled(row_rect, 0.0, ui.visuals().faint_bg_color);
                        let resp = ui.interact(
                            sig_rect,
                            ui.make_persistent_id(("grp_row", *node_idx)),
                            egui::Sense::click(),
                        );
                        if resp.clicked() {
                            action_toggle_group = Some(*node_idx);
                        }
                        let arrow = if *collapsed { "▶" } else { "▼" };
                        ui.painter().text(
                            egui::pos2(sig_rect.min.x + 8.0, sig_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("{} 📁 {} ({})", arrow, name, child_count),
                            egui::FontId::monospace(12.0),
                            egui::Color32::from_rgb(203, 166, 247),
                        );
                        resp.context_menu(|ui| {
                            if ui.button("Toggle Expand / Collapse").clicked() {
                                action_toggle_group = Some(*node_idx);
                                ui.close_menu();
                            }
                            if ui.button("Delete Group").clicked() {
                                action_delete_node = Some(*node_idx);
                                ui.close_menu();
                            }
                        });
                    }
                    VisibleRow::Divider { node_idx, label } => {
                        let resp = ui.interact(
                            sig_rect,
                            ui.make_persistent_id(("div_row", *node_idx)),
                            egui::Sense::click(),
                        );
                        ui.painter().text(
                            sig_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("─── {} ───", label),
                            egui::FontId::monospace(11.0),
                            ui.visuals().weak_text_color(),
                        );
                        resp.context_menu(|ui| {
                            if ui.button("Delete Divider").clicked() {
                                action_delete_node = Some(*node_idx);
                                ui.close_menu();
                            }
                        });
                    }
                    VisibleRow::Signal {
                        loc,
                        indented,
                        config,
                        signal_id,
                        width,
                    } => {
                        let is_selected = app.selected_locations.contains(loc);
                        if is_selected {
                            ui.painter().rect_filled(
                                row_rect,
                                0.0,
                                ui.visuals().selection.bg_fill.gamma_multiply(0.35),
                            );
                        }

                        let resp = ui.interact(
                            row_rect,
                            ui.make_persistent_id(("sig_row", loc.node_idx, loc.group_child_idx)),
                            egui::Sense::click(),
                        );
                        if resp.clicked() {
                            let (ctrl_or_cmd, shift) = ui.input(|i| {
                                (i.modifiers.command || i.modifiers.ctrl, i.modifiers.shift)
                            });
                            apply_row_click_selection(
                                &mut app.selected_locations,
                                &mut app.primary_selected,
                                *loc,
                                ctrl_or_cmd,
                                shift,
                                &ordered_visible_locs,
                            );
                        }

                        let indent_px = if *indented { 18.0 } else { 8.0 };
                        let expand_prefix = if *width > 1 {
                            if config.expand_bits {
                                "▼ "
                            } else {
                                "▶ "
                            }
                        } else {
                            "  "
                        };
                        let range_suffix = if *width > 1 {
                            format!("[{}:0]", width - 1)
                        } else {
                            String::new()
                        };
                        let trace_color = egui::Color32::from_rgb(
                            config.color_rgb[0],
                            config.color_rgb[1],
                            config.color_rgb[2],
                        );

                        ui.painter().text(
                            egui::pos2(sig_rect.min.x + indent_px, sig_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("{}{}{}", expand_prefix, config.display_label(), range_suffix),
                            egui::FontId::monospace(12.0),
                            ui.visuals().text_color(),
                        );

                        // Compute Value at Cursor
                        let val_str = if let (Some(sid), Some(hdr)) = (signal_id, &app.header) {
                            let vcd_id = hdr.signals[sid.0].vcd_id;
                            if let Some(wave) = app.waves.get(&vcd_id) {
                                if let Some((_, sval)) = wave.value_at(app.doc.cursor_time) {
                                    format_signal_value(sval, config.radix, config.signedness)
                                } else {
                                    "-".to_string()
                                }
                            } else {
                                "...".to_string()
                            }
                        } else {
                            "N/A".to_string()
                        };

                        let val_color = if val_str.contains('X') || val_str.contains('x') {
                            egui::Color32::from_rgb(243, 139, 168)
                        } else if val_str.contains('Z') || val_str.contains('z') {
                            egui::Color32::from_rgb(250, 179, 135)
                        } else {
                            trace_color
                        };

                        ui.painter().text(
                            egui::pos2(val_rect.max.x - 8.0, val_rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            val_str,
                            egui::FontId::monospace(12.0),
                            val_color,
                        );

                        // Right-click context menu supporting single or multi-selection
                        resp.context_menu(|ui| {
                            let targets = resolve_context_menu_targets(
                                &mut app.selected_locations,
                                &mut app.primary_selected,
                                *loc,
                            );
                            let n = targets.len();
                            if n > 1 {
                                ui.label(
                                    egui::RichText::new(format!("{} signals selected", n))
                                        .strong()
                                        .small(),
                                );
                            } else {
                                ui.label(egui::RichText::new(&config.signal_path).weak().small());
                            }
                            ui.separator();

                            let copy_label = if n > 1 {
                                format!("📋 Copy Selected ({})", n)
                            } else {
                                "📋 Copy Signal  (Cmd/Ctrl+C)".to_string()
                            };
                            if ui.button(copy_label).clicked() {
                                action_copy_selected = true;
                                ui.close_menu();
                            }

                            let cut_label = if n > 1 {
                                format!("✂ Cut Selected ({})", n)
                            } else {
                                "✂ Cut Signal   (Cmd/Ctrl+X)".to_string()
                            };
                            if ui.button(cut_label).clicked() {
                                action_cut_selected = true;
                                ui.close_menu();
                            }

                            if !app.clipboard_signals.is_empty() {
                                let paste_label = format!(
                                    "📄 Paste Below ({})",
                                    app.clipboard_signals.len()
                                );
                                if ui.button(paste_label).clicked() {
                                    action_paste_after = Some(*loc);
                                    ui.close_menu();
                                }
                            }

                            let del_label = if n > 1 {
                                format!("🗑 Delete Selected ({})", n)
                            } else {
                                "🗑 Delete Signal (Del)".to_string()
                            };
                            if ui
                                .button(
                                    egui::RichText::new(del_label)
                                        .color(egui::Color32::from_rgb(243, 139, 168)),
                                )
                                .clicked()
                            {
                                action_delete_selected = true;
                                ui.close_menu();
                            }

                            if ui.button(format!("📁 Move to New Group ({})", n)).clicked() {
                                action_group_selected = true;
                                ui.close_menu();
                            }

                            if n == 1 {
                                if let Some(live_cfg) = app.doc.get_signal_mut(*loc) {
                                    ui.menu_button("✏ Rename Alias", |ui| {
                                        let mut alias_text =
                                            live_cfg.alias.clone().unwrap_or_default();
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut alias_text)
                                                    .desired_width(140.0)
                                                    .hint_text("Custom label..."),
                                            )
                                            .changed()
                                        {
                                            live_cfg.alias = if alias_text.trim().is_empty() {
                                                None
                                            } else {
                                                Some(alias_text)
                                            };
                                        }
                                        if ui.small_button("Reset to VCD Path").clicked() {
                                            live_cfg.alias = None;
                                            ui.close_menu();
                                        }
                                    });
                                }

                                if loc.group_child_idx.is_none() {
                                    ui.separator();
                                    if ui.button("⬆ Move Up").clicked() {
                                        action_move_up = Some(loc.node_idx);
                                        ui.close_menu();
                                    }
                                    if ui.button("⬇ Move Down").clicked() {
                                        action_move_down = Some(loc.node_idx);
                                        ui.close_menu();
                                    }
                                }
                            }

                            if *width > 1 || n > 1 {
                                ui.separator();
                                let new_expand = !config.expand_bits;
                                let bit_label = if config.expand_bits {
                                    "↕ Collapse Bits"
                                } else {
                                    "↕ Expand Individual Bits"
                                };
                                if ui.button(bit_label).clicked() {
                                    app.doc.update_signals(&targets, |cfg| {
                                        cfg.expand_bits = new_expand;
                                    });
                                    ui.close_menu();
                                }
                            }

                            ui.separator();
                            let mut cur_radix = config.radix;
                            ui.menu_button("🔢 Radix", |ui| {
                                for (r, name) in [
                                    (Radix::Bin, "Binary (Bin)"),
                                    (Radix::Oct, "Octal (Oct)"),
                                    (Radix::Hex, "Hexadecimal (Hex)"),
                                    (Radix::Dec, "Decimal (Dec)"),
                                    (Radix::Ascii, "ASCII String"),
                                ] {
                                    if ui.radio_value(&mut cur_radix, r, name).clicked() {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.radix = r;
                                        });
                                        ui.close_menu();
                                    }
                                }
                            });

                            ui.menu_button("🧮 IEEE 754 Float", |ui| {
                                for fmt in FloatFormat::ALL {
                                    if ui
                                        .radio_value(
                                            &mut cur_radix,
                                            Radix::Float(fmt),
                                            fmt.label(),
                                        )
                                        .clicked()
                                    {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.radix = Radix::Float(fmt);
                                        });
                                        ui.close_menu();
                                    }
                                }
                            });

                            let mut cur_signedness = config.signedness;
                            ui.menu_button("➕ Signed / Unsigned", |ui| {
                                for (s, label) in [
                                    (Signedness::Unsigned, "Unsigned"),
                                    (Signedness::Signed2s, "Signed (2's Complement)"),
                                ] {
                                    if ui.radio_value(&mut cur_signedness, s, label).clicked() {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.signedness = s;
                                        });
                                        ui.close_menu();
                                    }
                                }
                            });

                            let mut cur_style = config.render_style;
                            ui.menu_button("📈 Waveform Style", |ui| {
                                for style in [
                                    RenderStyle::Digital,
                                    RenderStyle::AnalogStep,
                                    RenderStyle::AnalogLinear,
                                ] {
                                    if ui
                                        .radio_value(&mut cur_style, style, style.label())
                                        .clicked()
                                    {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.render_style = style;
                                        });
                                        ui.close_menu();
                                    }
                                }
                            });

                            let mut custom_rgb = config.color_rgb;
                            ui.menu_button("🎨 Trace Color", |ui| {
                                for (name, rgb) in PRESET_COLORS {
                                    let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                    if ui
                                        .button(
                                            egui::RichText::new(format!("● {}", name)).color(color),
                                        )
                                        .clicked()
                                    {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.color_rgb = rgb;
                                        });
                                        ui.close_menu();
                                    }
                                }
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label("Custom:");
                                    if ui.color_edit_button_srgb(&mut custom_rgb).changed() {
                                        app.doc.update_signals(&targets, |cfg| {
                                            cfg.color_rgb = custom_rgb;
                                        });
                                    }
                                });
                            });
                        });
                    }
                    VisibleRow::BitSlice {
                        loc: _,
                        signal_path: _,
                        signal_id,
                        bit_offset_from_msb,
                        bit_label,
                        color_rgb,
                    } => {
                        ui.painter().text(
                            egui::pos2(sig_rect.min.x + 24.0, sig_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            bit_label,
                            egui::FontId::monospace(11.0),
                            ui.visuals().weak_text_color(),
                        );

                        let bit_state = if let (Some(sid), Some(hdr)) = (signal_id, &app.header) {
                            let vcd_id = hdr.signals[sid.0].vcd_id;
                            app.waves
                                .get(&vcd_id)
                                .and_then(|w| w.bit_value_at(app.doc.cursor_time, *bit_offset_from_msb))
                        } else {
                            None
                        };

                        let (b_str, b_col) = match bit_state {
                            Some(LogicState::Zero) => (
                                "0",
                                egui::Color32::from_rgb(color_rgb[0], color_rgb[1], color_rgb[2]),
                            ),
                            Some(LogicState::One) => (
                                "1",
                                egui::Color32::from_rgb(color_rgb[0], color_rgb[1], color_rgb[2]),
                            ),
                            Some(LogicState::X) => ("X", egui::Color32::from_rgb(243, 139, 168)),
                            Some(LogicState::Z) => ("Z", egui::Color32::from_rgb(250, 179, 135)),
                            None => ("-", ui.visuals().weak_text_color()),
                        };

                        ui.painter().text(
                            egui::pos2(val_rect.max.x - 8.0, val_rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            b_str,
                            egui::FontId::monospace(11.0),
                            b_col,
                        );
                    }
                }
            }
        },
    );

    if let Some(idx) = action_toggle_group {
        if let Some(DisplayedNode::Group { collapsed, .. }) = app.doc.rows.get_mut(idx) {
            *collapsed = !*collapsed;
        }
    }
    if let Some(idx) = action_delete_node {
        if idx < app.doc.rows.len() {
            app.doc.rows.remove(idx);
            app.selected_locations = app
                .selected_locations
                .iter()
                .filter_map(|&loc| {
                    if loc.node_idx == idx {
                        None
                    } else if loc.node_idx > idx {
                        Some(SignalLocation {
                            node_idx: loc.node_idx - 1,
                            group_child_idx: loc.group_child_idx,
                        })
                    } else {
                        Some(loc)
                    }
                })
                .collect();
            app.primary_selected = app.selected_locations.iter().next().copied();
        }
    }
    if action_copy_selected {
        app.copy_selected_signals();
    }
    if action_cut_selected {
        app.cut_selected_signals();
    }
    if action_delete_selected {
        app.delete_selected_signals();
    }
    if let Some(anchor_loc) = action_paste_after {
        let clips = app.clipboard_signals.clone();
        app.doc.insert_signals_after(Some(anchor_loc), &clips);
        app.sync_watched_signals();
    }
    if action_group_selected && !app.selected_locations.is_empty() {
        let children = app.doc.cut_locations(&app.selected_locations);
        app.selected_locations.clear();
        app.primary_selected = None;
        app.doc.rows.push(DisplayedNode::Group {
            name: format!("Group {}", app.doc.rows.len() + 1),
            collapsed: false,
            children,
        });
    }
    if let Some(idx) = action_move_up {
        if idx > 0 && idx < app.doc.rows.len() {
            app.doc.rows.swap(idx, idx - 1);
            let new_loc = SignalLocation {
                node_idx: idx - 1,
                group_child_idx: None,
            };
            app.selected_locations.clear();
            app.selected_locations.insert(new_loc);
            app.primary_selected = Some(new_loc);
        }
    }
    if let Some(idx) = action_move_down {
        if idx + 1 < app.doc.rows.len() {
            app.doc.rows.swap(idx, idx + 1);
            let new_loc = SignalLocation {
                node_idx: idx + 1,
                group_child_idx: None,
            };
            app.selected_locations.clear();
            app.selected_locations.insert(new_loc);
            app.primary_selected = Some(new_loc);
        }
    }
}

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
