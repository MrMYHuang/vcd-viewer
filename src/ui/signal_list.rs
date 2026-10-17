use crate::format::{format_signal_value, FloatFormat, Radix, Signedness};
use crate::project::{DisplayedNode, RenderStyle, SignalLocation, SignalRowConfig, VisibleRow};
use crate::ui::VcdViewerApp;
use crate::waveform_db::LogicState;

const PRESET_COLORS: [(&str, [u8; 3]); 7] = [
    ("Green", [166, 227, 161]),
    ("Blue", [137, 180, 250]),
    ("Yellow", [249, 226, 175]),
    ("Orange", [250, 179, 135]),
    ("Pink", [243, 139, 168]),
    ("Purple", [203, 166, 247]),
    ("Cyan", [148, 226, 213]),
];

pub fn render_signal_context_menu(
    ui: &mut egui::Ui,
    loc: SignalLocation,
    width: u32,
    doc_signal: &mut SignalRowConfig,
    clipboard: &mut Option<SignalRowConfig>,
    action_delete: &mut Option<SignalLocation>,
    action_paste_after: &mut Option<(SignalLocation, SignalRowConfig)>,
    action_move_up: &mut Option<usize>,
    action_move_down: &mut Option<usize>,
) {
    ui.label(egui::RichText::new(&doc_signal.signal_path).weak().small());
    ui.separator();

    if ui.button("📋 Copy Signal").clicked() {
        *clipboard = Some(doc_signal.clone());
        ui.close_menu();
    }
    if let Some(ref clip) = clipboard {
        if ui.button("📄 Paste Signal Below").clicked() {
            *action_paste_after = Some((loc, clip.clone()));
            ui.close_menu();
        }
    }
    if ui
        .button(egui::RichText::new("🗑 Delete Signal").color(egui::Color32::from_rgb(243, 139, 168)))
        .clicked()
    {
        *action_delete = Some(loc);
        ui.close_menu();
    }
    ui.menu_button("✏ Rename Alias", |ui| {
        let mut alias_text = doc_signal.alias.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut alias_text)
                    .desired_width(140.0)
                    .hint_text("Custom label..."),
            )
            .changed()
        {
            doc_signal.alias = if alias_text.trim().is_empty() {
                None
            } else {
                Some(alias_text)
            };
        }
        if ui.small_button("Reset to VCD Path").clicked() {
            doc_signal.alias = None;
            ui.close_menu();
        }
    });

    if loc.group_child_idx.is_none() {
        ui.separator();
        if ui.button("⬆ Move Up").clicked() {
            *action_move_up = Some(loc.node_idx);
            ui.close_menu();
        }
        if ui.button("⬇ Move Down").clicked() {
            *action_move_down = Some(loc.node_idx);
            ui.close_menu();
        }
    }

    if width > 1 {
        ui.separator();
        let bit_label = if doc_signal.expand_bits {
            "↕ Collapse Bits"
        } else {
            "↕ Expand Individual Bits"
        };
        if ui.button(bit_label).clicked() {
            doc_signal.expand_bits = !doc_signal.expand_bits;
            ui.close_menu();
        }
    }

    ui.separator();
    ui.menu_button("🔢 Radix", |ui| {
        for (r, name) in [
            (Radix::Bin, "Binary (Bin)"),
            (Radix::Oct, "Octal (Oct)"),
            (Radix::Hex, "Hexadecimal (Hex)"),
            (Radix::Dec, "Decimal (Dec)"),
            (Radix::Ascii, "ASCII String"),
        ] {
            if ui.radio_value(&mut doc_signal.radix, r, name).clicked() {
                ui.close_menu();
            }
        }
    });

    ui.menu_button("🧮 IEEE 754 Float", |ui| {
        for fmt in FloatFormat::ALL {
            if ui
                .radio_value(&mut doc_signal.radix, Radix::Float(fmt), fmt.label())
                .clicked()
            {
                ui.close_menu();
            }
        }
    });

    ui.menu_button("➕ Signed / Unsigned", |ui| {
        if ui
            .radio_value(
                &mut doc_signal.signedness,
                Signedness::Unsigned,
                "Unsigned",
            )
            .clicked()
        {
            ui.close_menu();
        }
        if ui
            .radio_value(
                &mut doc_signal.signedness,
                Signedness::Signed2s,
                "Signed (2's Complement)",
            )
            .clicked()
        {
            ui.close_menu();
        }
    });

    ui.menu_button("📈 Waveform Style", |ui| {
        for style in [
            RenderStyle::Digital,
            RenderStyle::AnalogStep,
            RenderStyle::AnalogLinear,
        ] {
            if ui
                .radio_value(&mut doc_signal.render_style, style, style.label())
                .clicked()
            {
                ui.close_menu();
            }
        }
    });

    ui.menu_button("🎨 Trace Color", |ui| {
        for (name, rgb) in PRESET_COLORS {
            let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            if ui
                .button(egui::RichText::new(format!("● {}", name)).color(color))
                .clicked()
            {
                doc_signal.color_rgb = rgb;
                ui.close_menu();
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Custom:");
            ui.color_edit_button_srgb(&mut doc_signal.color_rgb);
        });
    });
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
                if ui.small_button("+Group").on_hover_text("Group selected signal").clicked() {
                    let children = if let Some(loc) = app.selected_location.take() {
                        if let Some(cfg) = app.doc.get_signal(loc).cloned() {
                            app.doc.delete_location(loc);
                            vec![cfg]
                        } else {
                            Vec::new()
                        }
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
    let mut action_delete: Option<SignalLocation> = None;
    let mut action_paste_after: Option<(SignalLocation, SignalRowConfig)> = None;
    let mut action_toggle_group: Option<usize> = None;
    let mut action_delete_node: Option<usize> = None;
    let mut action_move_up: Option<usize> = None;
    let mut action_move_down: Option<usize> = None;

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
                        let is_selected = app.selected_location == Some(*loc);
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
                            app.selected_location = Some(*loc);
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

                        // Right-click context menu on the live SignalRowConfig
                        resp.context_menu(|ui| {
                            if let Some(live_cfg) = app.doc.get_signal_mut(*loc) {
                                render_signal_context_menu(
                                    ui,
                                    *loc,
                                    *width,
                                    live_cfg,
                                    &mut app.clipboard_signal,
                                    &mut action_delete,
                                    &mut action_paste_after,
                                    &mut action_move_up,
                                    &mut action_move_down,
                                );
                            }
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
        }
    }
    if let Some(loc) = action_delete {
        app.doc.delete_location(loc);
        if app.selected_location == Some(loc) {
            app.selected_location = None;
        }
    }
    if let Some((loc, cfg)) = action_paste_after {
        app.doc.insert_signal_after(Some(loc), cfg);
        app.sync_watched_signals();
    }
    if let Some(idx) = action_move_up {
        if idx > 0 && idx < app.doc.rows.len() {
            app.doc.rows.swap(idx, idx - 1);
            app.selected_location = Some(SignalLocation {
                node_idx: idx - 1,
                group_child_idx: None,
            });
        }
    }
    if let Some(idx) = action_move_down {
        if idx + 1 < app.doc.rows.len() {
            app.doc.rows.swap(idx, idx + 1);
            app.selected_location = Some(SignalLocation {
                node_idx: idx + 1,
                group_child_idx: None,
            });
        }
    }
}
