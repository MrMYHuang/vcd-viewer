use crate::format::{Radix, Signedness};
use crate::vcd_header::{SignalId, TimeUnit, VcdHeader};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderStyle {
    #[default]
    Digital,
    AnalogStep,
    AnalogLinear,
}

impl RenderStyle {
    pub fn label(&self) -> &'static str {
        match self {
            RenderStyle::Digital => "Digital Bus",
            RenderStyle::AnalogStep => "Analog (Step)",
            RenderStyle::AnalogLinear => "Analog (Linear)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalRowConfig {
    pub signal_path: String,
    pub alias: Option<String>,
    pub radix: Radix,
    pub signedness: Signedness,
    pub render_style: RenderStyle,
    pub color_rgb: [u8; 3],
    pub expand_bits: bool,
}

impl SignalRowConfig {
    pub fn new(signal_path: impl Into<String>, width: u32) -> Self {
        let default_color = if width <= 1 {
            [166, 227, 161] // Catppuccin green for 1-bit signals
        } else {
            [137, 180, 250] // Catppuccin blue for buses
        };
        Self {
            signal_path: signal_path.into(),
            alias: None,
            radix: if width <= 1 { Radix::Bin } else { Radix::Hex },
            signedness: Signedness::Unsigned,
            render_style: RenderStyle::Digital,
            color_rgb: default_color,
            expand_bits: false,
        }
    }

    pub fn with_radix(mut self, radix: Radix) -> Self {
        self.radix = radix;
        self
    }

    pub fn display_label(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.signal_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisplayedNode {
    Signal(SignalRowConfig),
    Group {
        name: String,
        collapsed: bool,
        children: Vec<SignalRowConfig>,
    },
    Divider {
        label: String,
    },
}

impl DisplayedNode {
    pub fn with_radix(mut self, radix: Radix) -> Self {
        if let DisplayedNode::Signal(ref mut s) = self {
            s.radix = radix;
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalLocation {
    pub node_idx: usize,
    pub group_child_idx: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum VisibleRow {
    GroupHeader {
        node_idx: usize,
        name: String,
        collapsed: bool,
        child_count: usize,
    },
    Divider {
        node_idx: usize,
        label: String,
    },
    Signal {
        loc: SignalLocation,
        indented: bool,
        config: SignalRowConfig,
        signal_id: Option<SignalId>,
        width: u32,
    },
    BitSlice {
        loc: SignalLocation,
        signal_path: String,
        signal_id: Option<SignalId>,
        bit_offset_from_msb: usize,
        bit_label: String,
        color_rgb: [u8; 3],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub name: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Viewport {
    pub start_time: f64,
    pub end_time: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            end_time: 1000.0,
        }
    }
}

impl Viewport {
    pub fn span(&self) -> f64 {
        (self.end_time - self.start_time).max(1.0)
    }

    pub fn zoom_factor(&mut self, factor: f64, anchor_ratio: f64, max_time: f64) {
        let ratio = anchor_ratio.clamp(0.0, 1.0);
        let old_span = self.span();
        let anchor_time = self.start_time + old_span * ratio;
        let new_span = (old_span * factor).clamp(1.0, max_time.max(10.0) * 1.2);
        let mut new_start = anchor_time - new_span * ratio;
        if new_start < 0.0 {
            new_start = 0.0;
        }
        self.start_time = new_start;
        self.end_time = new_start + new_span;
    }

    pub fn pan_by_time(&mut self, delta: f64, max_time: f64) {
        let span = self.span();
        let mut new_start = (self.start_time + delta).max(0.0);
        let limit = (max_time.max(span) - span).max(0.0);
        if new_start > limit {
            new_start = limit;
        }
        self.start_time = new_start;
        self.end_time = new_start + span;
    }

    pub fn zoom_full(&mut self, min_time: u64, max_time: u64) {
        let s = min_time as f64;
        let e = (max_time as f64).max(s + 10.0);
        self.start_time = s;
        self.end_time = e;
    }

    pub fn zoom_drag_range(&mut self, px_start: f32, px_end: f32, canvas_width: f32) -> bool {
        if canvas_width <= 1.0 || (px_end - px_start).abs() < 5.0 {
            return false;
        }
        let x0 = px_start.min(px_end).clamp(0.0, canvas_width);
        let x1 = px_start.max(px_end).clamp(0.0, canvas_width);
        let span = self.span();
        let t0 = self.start_time + (x0 as f64 / canvas_width as f64) * span;
        let t1 = self.start_time + (x1 as f64 / canvas_width as f64) * span;
        if (t1 - t0) < 1.0 {
            self.start_time = t0.floor();
            self.end_time = self.start_time + 1.0;
        } else {
            self.start_time = t0;
            self.end_time = t1;
        }
        true
    }

    pub fn time_to_x(&self, time: f64, width: f32) -> f32 {
        if width <= 0.0 {
            return 0.0;
        }
        (((time - self.start_time) / self.span()) * width as f64) as f32
    }

    pub fn x_to_time(&self, x: f32, width: f32) -> u64 {
        if width <= 0.0 {
            return self.start_time.max(0.0) as u64;
        }
        let ratio = (x / width).clamp(0.0, 1.0) as f64;
        (self.start_time + ratio * self.span()).round().max(0.0) as u64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalListDocument {
    pub version: u32,
    pub theme: ThemeMode,
    pub display_unit: TimeUnit,
    pub cursor_time: u64,
    pub markers: Vec<Marker>,
    pub viewport: Viewport,
    pub rows: Vec<DisplayedNode>,
}

impl Default for SignalListDocument {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeMode::Dark,
            display_unit: TimeUnit::Ns,
            cursor_time: 0,
            markers: Vec::new(),
            viewport: Viewport::default(),
            rows: Vec::new(),
        }
    }
}

impl SignalListDocument {
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    pub fn get_signal_mut(&mut self, loc: SignalLocation) -> Option<&mut SignalRowConfig> {
        let node = self.rows.get_mut(loc.node_idx)?;
        match (node, loc.group_child_idx) {
            (DisplayedNode::Signal(cfg), None) => Some(cfg),
            (DisplayedNode::Group { children, .. }, Some(c_idx)) => children.get_mut(c_idx),
            _ => None,
        }
    }

    pub fn get_signal(&self, loc: SignalLocation) -> Option<&SignalRowConfig> {
        let node = self.rows.get(loc.node_idx)?;
        match (node, loc.group_child_idx) {
            (DisplayedNode::Signal(cfg), None) => Some(cfg),
            (DisplayedNode::Group { children, .. }, Some(c_idx)) => children.get(c_idx),
            _ => None,
        }
    }

    pub fn delete_location(&mut self, loc: SignalLocation) {
        if let Some(c_idx) = loc.group_child_idx {
            if let Some(DisplayedNode::Group { children, .. }) = self.rows.get_mut(loc.node_idx) {
                if c_idx < children.len() {
                    children.remove(c_idx);
                }
            }
        } else if loc.node_idx < self.rows.len() {
            self.rows.remove(loc.node_idx);
        }
    }

    pub fn insert_signal_after(&mut self, loc: Option<SignalLocation>, cfg: SignalRowConfig) {
        if let Some(l) = loc {
            if let Some(c_idx) = l.group_child_idx {
                if let Some(DisplayedNode::Group { children, .. }) = self.rows.get_mut(l.node_idx)
                {
                    let pos = (c_idx + 1).min(children.len());
                    children.insert(pos, cfg);
                    return;
                }
            }
            let pos = (l.node_idx + 1).min(self.rows.len());
            self.rows.insert(pos, DisplayedNode::Signal(cfg));
        } else {
            self.rows.push(DisplayedNode::Signal(cfg));
        }
    }

    pub fn flatten_visible_rows(&self, header: Option<&VcdHeader>) -> Vec<VisibleRow> {
        let mut out = Vec::new();

        let push_sig = |out: &mut Vec<VisibleRow>,
                        loc: SignalLocation,
                        indented: bool,
                        cfg: &SignalRowConfig| {
            let meta = header
                .and_then(|h| h.path_to_signal.get(&cfg.signal_path).copied())
                .map(|sid| &header.unwrap().signals[sid.0]);
            let signal_id = meta.map(|m| m.id);
            let width = meta.map(|m| m.width).unwrap_or(1);

            out.push(VisibleRow::Signal {
                loc,
                indented,
                config: cfg.clone(),
                signal_id,
                width,
            });

            if cfg.expand_bits && width > 1 {
                let msb = meta.and_then(|m| m.msb).unwrap_or(width as i32 - 1);
                let lsb = meta.and_then(|m| m.lsb).unwrap_or(0);
                let step = if msb >= lsb { -1 } else { 1 };
                for offset in 0..(width as usize) {
                    let bit_num = msb + (offset as i32) * step;
                    out.push(VisibleRow::BitSlice {
                        loc,
                        signal_path: cfg.signal_path.clone(),
                        signal_id,
                        bit_offset_from_msb: offset,
                        bit_label: format!("  [{}]", bit_num),
                        color_rgb: cfg.color_rgb,
                    });
                }
            }
        };

        for (node_idx, node) in self.rows.iter().enumerate() {
            match node {
                DisplayedNode::Signal(cfg) => {
                    push_sig(
                        &mut out,
                        SignalLocation {
                            node_idx,
                            group_child_idx: None,
                        },
                        false,
                        cfg,
                    );
                }
                DisplayedNode::Group {
                    name,
                    collapsed,
                    children,
                } => {
                    out.push(VisibleRow::GroupHeader {
                        node_idx,
                        name: name.clone(),
                        collapsed: *collapsed,
                        child_count: children.len(),
                    });
                    if !*collapsed {
                        for (c_idx, child) in children.iter().enumerate() {
                            push_sig(
                                &mut out,
                                SignalLocation {
                                    node_idx,
                                    group_child_idx: Some(c_idx),
                                },
                                true,
                                child,
                            );
                        }
                    }
                }
                DisplayedNode::Divider { label } => {
                    out.push(VisibleRow::Divider {
                        node_idx,
                        label: label.clone(),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{FloatFormat, Radix};
    use crate::vcd_header::TimeUnit;

    #[test]
    fn test_viewport_drag_zoom_ignores_tiny_clicks() {
        let mut vp = Viewport {
            start_time: 0.0,
            end_time: 1000.0,
        };
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
            markers: vec![Marker {
                name: "M1".into(),
                timestamp: 185,
            }],
            viewport: Viewport {
                start_time: 0.0,
                end_time: 500.0,
            },
            rows: vec![
                DisplayedNode::Signal(SignalRowConfig::new("top.u_fpu.out", 32))
                    .with_radix(Radix::Float(FloatFormat::Fp32)),
            ],
        };
        let json = doc.to_json().unwrap();
        let restored = SignalListDocument::from_json(&json).unwrap();
        assert_eq!(restored.theme, ThemeMode::Dark);
        assert_eq!(restored.markers.len(), 1);
        assert_eq!(restored.rows.len(), 1);
    }
}
