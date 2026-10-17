use crate::format::{format_signal_value, signal_value_to_analog, Radix, Signedness};
use crate::project::{RenderStyle, Viewport, VisibleRow};
use crate::ui::VcdViewerApp;
use crate::vcd_header::{TimeUnit, Timescale};
use crate::waveform_db::{LogicState, SignalValue, SignalWave};

pub fn format_timestamp_in_unit(ticks: u64, ts: Timescale, display_unit: TimeUnit) -> String {
    let fs = (ticks as f64) * (ts.factor as f64) * ts.unit.femtoseconds();
    let val = fs / display_unit.femtoseconds();
    let frac = (val - val.round()).abs();
    if frac < 1e-7 {
        format!("{:.0} {}", val, display_unit.label())
    } else {
        format!("{:.3} {}", val, display_unit.label())
    }
}

pub fn compute_nice_ticks(start_time: f64, end_time: f64, width_px: f32) -> Vec<u64> {
    let span = (end_time - start_time).max(1.0);
    let target_ticks = (width_px / 90.0).clamp(4.0, 14.0) as f64;
    let raw_step = (span / target_ticks).max(1.0);
    let mag = 10.0f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let nice_mult = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    let step = (nice_mult * mag).round().max(1.0) as u64;
    let first = ((start_time.max(0.0) as u64) / step) * step;
    let end_u = end_time.ceil().max(0.0) as u64;

    let mut out = Vec::new();
    let mut t = first;
    while t <= end_u {
        if (t as f64) >= start_time {
            out.push(t);
        }
        t = t.saturating_add(step);
        if out.len() > 32 {
            break;
        }
    }
    out
}

pub fn render_time_ruler(app: &mut VcdViewerApp, ui: &mut egui::Ui, ruler_height: f32) {
    let avail_w = ui.available_width().max(100.0);
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(avail_w, ruler_height),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().faint_bg_color);

    let ts = app.timescale();
    let ticks = compute_nice_ticks(
        app.doc.viewport.start_time,
        app.doc.viewport.end_time,
        rect.width(),
    );

    for t in ticks {
        let x = rect.min.x + app.doc.viewport.time_to_x(t as f64, rect.width());
        painter.line_segment(
            [
                egui::pos2(x, rect.max.y - 8.0),
                egui::pos2(x, rect.max.y),
            ],
            egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color()),
        );
        let label = format_timestamp_in_unit(t, ts, app.doc.display_unit);
        painter.text(
            egui::pos2(x + 3.0, rect.min.y + 4.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::monospace(10.5),
            ui.visuals().text_color(),
        );
    }

    // Draw Marker flags on ruler
    for (idx, m) in app.doc.markers.iter().enumerate() {
        let mx = rect.min.x + app.doc.viewport.time_to_x(m.timestamp as f64, rect.width());
        if mx >= rect.min.x && mx <= rect.max.x {
            let col = if idx == app.active_marker_idx {
                egui::Color32::from_rgb(249, 226, 175)
            } else {
                egui::Color32::from_rgb(250, 179, 135)
            };
            painter.text(
                egui::pos2(mx + 2.0, rect.max.y - 10.0),
                egui::Align2::LEFT_CENTER,
                format!("🚩{}", m.name),
                egui::FontId::monospace(10.0),
                col,
            );
        }
    }

    // Draw Primary Cursor flag on ruler
    let cx = rect.min.x
        + app
            .doc
            .viewport
            .time_to_x(app.doc.cursor_time as f64, rect.width());
    if cx >= rect.min.x && cx <= rect.max.x {
        painter.line_segment(
            [egui::pos2(cx, rect.min.y), egui::pos2(cx, rect.max.y)],
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(243, 139, 168)),
        );
    }

    if resp.clicked() || resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let local_x = (pos.x - rect.min.x).clamp(0.0, rect.width());
            app.doc.cursor_time = app.doc.viewport.x_to_time(local_x, rect.width());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_scalar_trace(
    painter: &egui::Painter,
    row_rect: egui::Rect,
    vp: Viewport,
    wave: &SignalWave,
    bit_offset: Option<usize>,
    base_color: egui::Color32,
) {
    let top_y = row_rect.min.y + 5.0;
    let bot_y = row_rect.max.y - 5.0;
    let mid_y = row_rect.center().y;
    let w = row_rect.width();

    let buckets = wave.query_viewport_lod(
        vp.start_time.max(0.0) as u64,
        vp.end_time.ceil().max(1.0) as u64,
        (w as usize).max(64),
        false,
    );

    let extract_state = |val: &SignalValue| -> LogicState {
        match (val, bit_offset) {
            (SignalValue::Scalar(s), _) => *s,
            (SignalValue::Vector(v), Some(off)) => v.get(off).copied().unwrap_or(LogicState::X),
            (SignalValue::Vector(v), None) => v.last().copied().unwrap_or(LogicState::X),
            (SignalValue::Real(_), _) => LogicState::Zero,
        }
    };

    let state_y = |st: LogicState| -> f32 {
        match st {
            LogicState::One => top_y,
            LogicState::Zero => bot_y,
            LogicState::X | LogicState::Z => mid_y,
        }
    };

    let mut prev_y: Option<f32> = None;
    for b in buckets {
        let x0 = (row_rect.min.x + vp.time_to_x(b.t_start as f64, w)).clamp(row_rect.min.x, row_rect.max.x);
        let x1 = (row_rect.min.x + vp.time_to_x(b.t_end as f64, w)).clamp(row_rect.min.x, row_rect.max.x);

        if b.transition_count > 2 && (x1 - x0) <= 3.0 {
            let block_rect = egui::Rect::from_min_max(
                egui::pos2(x0, top_y),
                egui::pos2(x1.max(x0 + 1.5), bot_y),
            );
            painter.rect_filled(block_rect, 0.0, base_color.gamma_multiply(0.55));
            prev_y = None;
            continue;
        }

        let st = extract_state(&wave.values[b.last_idx]);
        let y = state_y(st);
        let stroke_col = match st {
            LogicState::X => egui::Color32::from_rgb(243, 139, 168),
            LogicState::Z => egui::Color32::from_rgb(250, 179, 135),
            _ => base_color,
        };

        if st == LogicState::X && x1 > x0 {
            let xz_rect = egui::Rect::from_min_max(egui::pos2(x0, top_y), egui::pos2(x1, bot_y));
            painter.rect_filled(
                xz_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(243, 139, 168, 55),
            );
        }

        if let Some(py) = prev_y {
            if (py - y).abs() > 0.5 {
                painter.line_segment(
                    [egui::pos2(x0, py), egui::pos2(x0, y)],
                    egui::Stroke::new(1.5_f32, stroke_col),
                );
            }
        }
        painter.line_segment(
            [egui::pos2(x0, y), egui::pos2(x1, y)],
            egui::Stroke::new(1.5_f32, stroke_col),
        );
        prev_y = Some(y);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bus_trace(
    painter: &egui::Painter,
    row_rect: egui::Rect,
    vp: Viewport,
    wave: &SignalWave,
    radix: Radix,
    signedness: Signedness,
    base_color: egui::Color32,
    text_color: egui::Color32,
) {
    let top_y = row_rect.min.y + 4.0;
    let bot_y = row_rect.max.y - 4.0;
    let mid_y = row_rect.center().y;
    let w = row_rect.width();

    let buckets = wave.query_viewport_lod(
        vp.start_time.max(0.0) as u64,
        vp.end_time.ceil().max(1.0) as u64,
        (w as usize).max(64),
        signedness == Signedness::Signed2s,
    );

    for b in buckets {
        let x0 = (row_rect.min.x + vp.time_to_x(b.t_start as f64, w)).clamp(row_rect.min.x, row_rect.max.x);
        let x1 = (row_rect.min.x + vp.time_to_x(b.t_end as f64, w)).clamp(row_rect.min.x, row_rect.max.x);
        let seg_w = x1 - x0;
        if seg_w < 0.5 {
            continue;
        }

        let col = if b.has_xz {
            egui::Color32::from_rgb(243, 139, 168)
        } else {
            base_color
        };

        if b.transition_count > 2 && seg_w <= 4.0 {
            let block = egui::Rect::from_min_max(
                egui::pos2(x0, top_y),
                egui::pos2(x1.max(x0 + 1.5), bot_y),
            );
            painter.rect_filled(block, 0.0, col.gamma_multiply(0.5));
            continue;
        }

        let bevel = (seg_w * 0.25).min(3.5);
        let stroke = egui::Stroke::new(1.4_f32, col);

        // Crossover X transition on left edge
        painter.line_segment(
            [egui::pos2(x0, mid_y), egui::pos2(x0 + bevel, top_y)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x0, mid_y), egui::pos2(x0 + bevel, bot_y)],
            stroke,
        );
        // Top and bottom bus rails
        painter.line_segment(
            [egui::pos2(x0 + bevel, top_y), egui::pos2(x1 - bevel, top_y)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x0 + bevel, bot_y), egui::pos2(x1 - bevel, bot_y)],
            stroke,
        );
        // Right bevel
        painter.line_segment(
            [egui::pos2(x1 - bevel, top_y), egui::pos2(x1, mid_y)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x1 - bevel, bot_y), egui::pos2(x1, mid_y)],
            stroke,
        );

        if seg_w > 38.0 {
            let val_str = format_signal_value(&wave.values[b.last_idx], radix, signedness);
            let clip_rect = egui::Rect::from_min_max(
                egui::pos2(x0 + bevel + 2.0, top_y),
                egui::pos2(x1 - bevel - 2.0, bot_y),
            );
            painter.with_clip_rect(clip_rect).text(
                clip_rect.center(),
                egui::Align2::CENTER_CENTER,
                val_str,
                egui::FontId::monospace(11.0),
                text_color,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_analog_trace(
    painter: &egui::Painter,
    row_rect: egui::Rect,
    vp: Viewport,
    wave: &SignalWave,
    radix: Radix,
    signedness: Signedness,
    linear: bool,
    base_color: egui::Color32,
) {
    let top_y = row_rect.min.y + 3.0;
    let bot_y = row_rect.max.y - 3.0;
    let w = row_rect.width();

    let buckets = wave.query_viewport_lod(
        vp.start_time.max(0.0) as u64,
        vp.end_time.ceil().max(1.0) as u64,
        (w as usize).max(64),
        signedness == Signedness::Signed2s,
    );
    if buckets.is_empty() {
        return;
    }

    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for b in &buckets {
        if let Some(a) = signal_value_to_analog(&wave.values[b.last_idx], radix, signedness) {
            min_v = min_v.min(a);
            max_v = max_v.max(a);
        }
    }
    if !min_v.is_finite() || !max_v.is_finite() {
        return;
    }
    let range = (max_v - min_v).abs().max(1e-9);
    let val_to_y = |v: f64| -> f32 {
        let norm = ((v - min_v) / range).clamp(0.0, 1.0) as f32;
        bot_y - norm * (bot_y - top_y)
    };

    let stroke = egui::Stroke::new(1.5_f32, base_color);
    let mut prev_pt: Option<egui::Pos2> = None;

    for b in &buckets {
        let Some(a) = signal_value_to_analog(&wave.values[b.last_idx], radix, signedness) else {
            prev_pt = None;
            continue;
        };
        let x0 = (row_rect.min.x + vp.time_to_x(b.t_start as f64, w)).clamp(row_rect.min.x, row_rect.max.x);
        let x1 = (row_rect.min.x + vp.time_to_x(b.t_end as f64, w)).clamp(row_rect.min.x, row_rect.max.x);
        let y = val_to_y(a);

        if let Some(p) = prev_pt {
            if linear {
                painter.line_segment([p, egui::pos2(x0, y)], stroke);
            } else {
                painter.line_segment([p, egui::pos2(x0, p.y)], stroke);
                painter.line_segment([egui::pos2(x0, p.y), egui::pos2(x0, y)], stroke);
            }
        }
        if !linear {
            painter.line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], stroke);
            prev_pt = Some(egui::pos2(x1, y));
        } else {
            prev_pt = Some(egui::pos2(x0, y));
        }
    }
}

pub fn render_waveform_canvas(
    app: &mut VcdViewerApp,
    ui: &mut egui::Ui,
    visible_rows: &[VisibleRow],
    row_height: f32,
) {
    let avail_w = ui.available_width().max(100.0);
    let total_h = (visible_rows.len() as f32 * row_height).max(ui.available_height());

    let (canvas_rect, resp) = ui.allocate_exact_size(
        egui::vec2(avail_w, total_h),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(canvas_rect);
    painter.rect_filled(canvas_rect, 0.0, ui.visuals().extreme_bg_color);

    // Draw subtle vertical time grid lines matching the ruler ticks
    let ticks = compute_nice_ticks(
        app.doc.viewport.start_time,
        app.doc.viewport.end_time,
        canvas_rect.width(),
    );
    for t in ticks {
        let x = canvas_rect.min.x + app.doc.viewport.time_to_x(t as f64, canvas_rect.width());
        painter.line_segment(
            [
                egui::pos2(x, canvas_rect.min.y),
                egui::pos2(x, canvas_rect.max.y),
            ],
            egui::Stroke::new(1.0_f32, ui.visuals().faint_bg_color),
        );
    }

    // Render each visible row's waveform
    for (row_idx, vrow) in visible_rows.iter().enumerate() {
        let y0 = canvas_rect.min.y + (row_idx as f32) * row_height;
        let row_rect = egui::Rect::from_min_size(
            egui::pos2(canvas_rect.min.x, y0),
            egui::vec2(canvas_rect.width(), row_height),
        );

        painter.line_segment(
            [
                egui::pos2(row_rect.min.x, row_rect.max.y),
                egui::pos2(row_rect.max.x, row_rect.max.y),
            ],
            egui::Stroke::new(1.0_f32, ui.visuals().faint_bg_color),
        );

        match vrow {
            VisibleRow::GroupHeader { .. } => {
                painter.rect_filled(row_rect, 0.0, ui.visuals().faint_bg_color);
            }
            VisibleRow::Divider { .. } => {
                painter.line_segment(
                    [
                        egui::pos2(row_rect.min.x, row_rect.center().y),
                        egui::pos2(row_rect.max.x, row_rect.center().y),
                    ],
                    egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
                );
            }
            VisibleRow::Signal {
                loc,
                config,
                signal_id,
                width,
                ..
            } => {
                if app.selected_location == Some(*loc) {
                    painter.rect_filled(
                        row_rect,
                        0.0,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.18),
                    );
                }
                if let (Some(sid), Some(hdr)) = (signal_id, &app.header) {
                    let vcd_id = hdr.signals[sid.0].vcd_id;
                    if let Some(wave) = app.waves.get(&vcd_id) {
                        let base_col = egui::Color32::from_rgb(
                            config.color_rgb[0],
                            config.color_rgb[1],
                            config.color_rgb[2],
                        );
                        match config.render_style {
                            RenderStyle::Digital => {
                                if *width <= 1 {
                                    draw_scalar_trace(
                                        &painter,
                                        row_rect,
                                        app.doc.viewport,
                                        wave,
                                        None,
                                        base_col,
                                    );
                                } else {
                                    draw_bus_trace(
                                        &painter,
                                        row_rect,
                                        app.doc.viewport,
                                        wave,
                                        config.radix,
                                        config.signedness,
                                        base_col,
                                        ui.visuals().text_color(),
                                    );
                                }
                            }
                            RenderStyle::AnalogStep => {
                                draw_analog_trace(
                                    &painter,
                                    row_rect,
                                    app.doc.viewport,
                                    wave,
                                    config.radix,
                                    config.signedness,
                                    false,
                                    base_col,
                                );
                            }
                            RenderStyle::AnalogLinear => {
                                draw_analog_trace(
                                    &painter,
                                    row_rect,
                                    app.doc.viewport,
                                    wave,
                                    config.radix,
                                    config.signedness,
                                    true,
                                    base_col,
                                );
                            }
                        }
                    }
                }
            }
            VisibleRow::BitSlice {
                signal_id,
                bit_offset_from_msb,
                color_rgb,
                ..
            } => {
                if let (Some(sid), Some(hdr)) = (signal_id, &app.header) {
                    let vcd_id = hdr.signals[sid.0].vcd_id;
                    if let Some(wave) = app.waves.get(&vcd_id) {
                        let base_col =
                            egui::Color32::from_rgb(color_rgb[0], color_rgb[1], color_rgb[2]);
                        draw_scalar_trace(
                            &painter,
                            row_rect,
                            app.doc.viewport,
                            wave,
                            Some(*bit_offset_from_msb),
                            base_col,
                        );
                    }
                }
            }
        }
    }

    // Draw Marker vertical lines
    for (idx, m) in app.doc.markers.iter().enumerate() {
        let mx = canvas_rect.min.x
            + app
                .doc
                .viewport
                .time_to_x(m.timestamp as f64, canvas_rect.width());
        if mx >= canvas_rect.min.x && mx <= canvas_rect.max.x {
            let col = if idx == app.active_marker_idx {
                egui::Color32::from_rgb(249, 226, 175)
            } else {
                egui::Color32::from_rgb(250, 179, 135)
            };
            painter.line_segment(
                [
                    egui::pos2(mx, canvas_rect.min.y),
                    egui::pos2(mx, canvas_rect.max.y),
                ],
                egui::Stroke::new(1.2_f32, col),
            );
        }
    }

    // Draw Primary Cursor vertical line
    let cx = canvas_rect.min.x
        + app
            .doc
            .viewport
            .time_to_x(app.doc.cursor_time as f64, canvas_rect.width());
    if cx >= canvas_rect.min.x && cx <= canvas_rect.max.x {
        painter.line_segment(
            [
                egui::pos2(cx, canvas_rect.min.y),
                egui::pos2(cx, canvas_rect.max.y),
            ],
            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(243, 139, 168)),
        );
    }

    // Mouse Wheel Zoom (Ctrl + Scroll) and Horizontal Pan (Shift + Scroll)
    if resp.hovered() {
        let max_t = app.index.max_timestamp.max(100) as f64;
        ui.input(|i| {
            let scroll = i.smooth_scroll_delta;
            if i.modifiers.ctrl || i.modifiers.command || i.zoom_delta() != 1.0 {
                let z = if i.zoom_delta() != 1.0 {
                    1.0 / (i.zoom_delta() as f64)
                } else if scroll.y > 0.0 {
                    0.85
                } else if scroll.y < 0.0 {
                    1.18
                } else {
                    1.0
                };
                if (z - 1.0).abs() > 1e-3 {
                    let anchor = i
                        .pointer
                        .hover_pos()
                        .map(|p| ((p.x - canvas_rect.min.x) / canvas_rect.width()).clamp(0.0, 1.0))
                        .unwrap_or(0.5) as f64;
                    app.doc.viewport.zoom_factor(z, anchor, max_t);
                }
            } else if i.modifiers.shift || scroll.x.abs() > 0.1 {
                let delta_px = if scroll.x.abs() > 0.1 {
                    -scroll.x
                } else {
                    -scroll.y
                };
                let delta_time =
                    (delta_px as f64 / canvas_rect.width() as f64) * app.doc.viewport.span();
                app.doc.viewport.pan_by_time(delta_time, max_t);
            }
        });
    }

    // Horizontal Drag Range Zoom vs Click-to-set-Cursor
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.drag_zoom_start_x = Some((pos.x - canvas_rect.min.x).clamp(0.0, canvas_rect.width()));
            app.drag_zoom_curr_x = app.drag_zoom_start_x;
        }
    }
    if resp.dragged_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.drag_zoom_curr_x = Some((pos.x - canvas_rect.min.x).clamp(0.0, canvas_rect.width()));
        }
    }
    if let (Some(sx), Some(cx_drag)) = (app.drag_zoom_start_x, app.drag_zoom_curr_x) {
        if (cx_drag - sx).abs() >= 5.0 {
            let x_min = canvas_rect.min.x + sx.min(cx_drag);
            let x_max = canvas_rect.min.x + sx.max(cx_drag);
            let sel_rect = egui::Rect::from_min_max(
                egui::pos2(x_min, canvas_rect.min.y),
                egui::pos2(x_max, canvas_rect.max.y),
            );
            painter.rect_filled(
                sel_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(137, 180, 250, 45),
            );
            painter.rect_stroke(
                sel_rect,
                0.0,
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(137, 180, 250)),
            );
        }
    }
    if resp.drag_stopped_by(egui::PointerButton::Primary) {
        if let (Some(sx), Some(ex)) = (app.drag_zoom_start_x.take(), app.drag_zoom_curr_x.take()) {
            if !app.doc.viewport.zoom_drag_range(sx, ex, canvas_rect.width()) {
                // Treated as single click -> set primary cursor
                app.doc.cursor_time = app.doc.viewport.x_to_time(ex, canvas_rect.width());
            }
        }
    } else if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let local_x = (pos.x - canvas_rect.min.x).clamp(0.0, canvas_rect.width());
            app.doc.cursor_time = app.doc.viewport.x_to_time(local_x, canvas_rect.width());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcd_header::{TimeUnit, Timescale};

    #[test]
    fn test_compute_ruler_ticks_and_unit_formatting() {
        let ts = Timescale {
            factor: 1,
            unit: TimeUnit::Ns,
        };
        // Viewport from 0 to 1000 ns displayed in Us (microseconds)
        let label = format_timestamp_in_unit(500, ts, TimeUnit::Us);
        assert_eq!(label, "0.500 us");

        let ticks = compute_nice_ticks(0.0, 1000.0, 800.0);
        assert!(ticks.len() >= 4 && ticks.len() <= 20);
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
    }
}
