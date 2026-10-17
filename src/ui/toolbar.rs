use crate::project::ThemeMode;
use crate::ui::VcdViewerApp;
use crate::vcd_header::TimeUnit;

pub fn render_top_toolbar(app: &mut VcdViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("📂 Load VCD").clicked() {
            app.trigger_open_vcd_dialog();
        }
        if ui.button("🧪 Demo SoC").on_hover_text("Load built-in demo SoC VCD trace").clicked() {
            app.load_demo_soc_vcd();
        }
        let select_btn = egui::Button::new(
            egui::RichText::new("➕ Select Signals...")
                .strong()
                .color(egui::Color32::from_rgb(17, 17, 27)),
        )
        .fill(egui::Color32::from_rgb(137, 180, 250));
        if ui.add(select_btn).clicked() {
            app.show_hierarchy_modal = !app.show_hierarchy_modal;
        }
        if ui.button("📋 Load List").clicked() {
            app.trigger_load_signal_list_dialog();
        }
        if ui.button("💾 Save List").clicked() {
            app.trigger_save_signal_list_dialog();
        }

        ui.separator();

        let max_t = app.index.max_timestamp.max(100) as f64;
        if ui.button("🔍+ In").on_hover_text("Zoom In (+)").clicked() {
            app.doc.viewport.zoom_factor(0.65, 0.5, max_t);
        }
        if ui.button("🔍- Out").on_hover_text("Zoom Out (-)").clicked() {
            app.doc.viewport.zoom_factor(1.5, 0.5, max_t);
        }
        if ui.button("⛶ Full").on_hover_text("Zoom Full (F)").clicked() {
            app.doc
                .viewport
                .zoom_full(app.index.min_timestamp, app.index.max_timestamp);
        }

        ui.separator();

        if ui
            .button("⏮ Prev Edge")
            .on_hover_text("Jump to previous transition of selected signal (P)")
            .clicked()
        {
            app.jump_prev_edge();
        }
        if ui
            .button("⏭ Next Edge")
            .on_hover_text("Jump to next transition of selected signal (N)")
            .clicked()
        {
            app.jump_next_edge();
        }

        ui.label("🔎 Value:");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.value_search_query)
                .desired_width(95.0)
                .hint_text("e.g. 0xFE / 3.14"),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.search_selected_value(true);
        }
        if ui.small_button("◀").on_hover_text("Find value backward").clicked() {
            app.search_selected_value(false);
        }
        if ui.small_button("▶").on_hover_text("Find value forward").clicked() {
            app.search_selected_value(true);
        }

        ui.separator();

        if ui
            .button("🚩 +Marker")
            .on_hover_text("Drop marker at current cursor time")
            .clicked()
        {
            app.add_marker_at_cursor();
        }

        ui.label("⏱ Unit:");
        egui::ComboBox::from_id_salt("time_unit_combo")
            .selected_text(app.doc.display_unit.label())
            .width(56.0)
            .show_ui(ui, |ui| {
                for unit in TimeUnit::ALL {
                    ui.selectable_value(&mut app.doc.display_unit, unit, unit.label());
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let theme_label = match app.doc.theme {
                ThemeMode::Dark => "🌙 Dark",
                ThemeMode::Light => "☀ Light",
            };
            if ui.button(theme_label).on_hover_text("Toggle Dark / Light Theme").clicked() {
                app.doc.theme = match app.doc.theme {
                    ThemeMode::Dark => ThemeMode::Light,
                    ThemeMode::Light => ThemeMode::Dark,
                };
                app.apply_theme(ctx);
            }
        });
    });

    ui.separator();

    // Delta Measurement Bar
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Cursor: {}",
                app.format_time(app.doc.cursor_time)
            ))
            .color(egui::Color32::from_rgb(243, 139, 168))
            .monospace()
            .strong(),
        );

        if !app.doc.markers.is_empty() {
            ui.separator();
            let mut remove_idx = None;
            for (idx, m) in app.doc.markers.iter().enumerate() {
                let is_active = idx == app.active_marker_idx;
                let text = format!("🚩 {}: {}", m.name, app.format_time(m.timestamp));
                let btn = ui.selectable_label(
                    is_active,
                    egui::RichText::new(text)
                        .color(egui::Color32::from_rgb(249, 226, 175))
                        .monospace(),
                );
                if btn.clicked() {
                    app.active_marker_idx = idx;
                    app.doc.cursor_time = m.timestamp;
                }
                if ui.small_button("✕").on_hover_text("Delete marker").clicked() {
                    remove_idx = Some(idx);
                }
            }
            if let Some(r) = remove_idx {
                app.doc.markers.remove(r);
                if app.active_marker_idx >= app.doc.markers.len() {
                    app.active_marker_idx = app.doc.markers.len().saturating_sub(1);
                }
            }

            if let Some(m) = app.doc.markers.get(app.active_marker_idx) {
                ui.separator();
                let ts = app.timescale();
                let tick_fs = (ts.factor as f64) * ts.unit.femtoseconds();
                let diff_ticks = (m.timestamp as i128) - (app.doc.cursor_time as i128);
                let diff_fs = (diff_ticks as f64) * tick_fs;
                let diff_in_unit = diff_fs / app.doc.display_unit.femtoseconds();
                let freq_str = if diff_fs.abs() > 0.0 {
                    let hz = 1e15 / diff_fs.abs();
                    if hz >= 1e9 {
                        format!("{:.2} GHz", hz / 1e9)
                    } else if hz >= 1e6 {
                        format!("{:.2} MHz", hz / 1e6)
                    } else if hz >= 1e3 {
                        format!("{:.2} kHz", hz / 1e3)
                    } else {
                        format!("{:.2} Hz", hz)
                    }
                } else {
                    "∞ Hz".to_string()
                };

                ui.label(
                    egui::RichText::new(format!(
                        "ΔT ({} - Cursor): {:+.3} {} ({})",
                        m.name,
                        diff_in_unit,
                        app.doc.display_unit.label(),
                        freq_str
                    ))
                    .color(egui::Color32::from_rgb(166, 227, 161))
                    .monospace()
                    .strong(),
                );
            }
        } else {
            ui.separator();
            ui.label(
                egui::RichText::new("Tip: Drag horizontally on waveform to zoom range | Right-click signal for IEEE 754 / Radix / Analog menu")
                    .weak()
                    .italics(),
            );
        }
    });
    ui.add_space(2.0);
}
