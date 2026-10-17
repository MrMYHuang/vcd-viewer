pub mod hierarchy_modal;
pub mod signal_list;
pub mod toolbar;
pub mod waveform_canvas;

#[cfg(not(target_arch = "wasm32"))]
use crate::byte_source::FileByteSource;
use crate::byte_source::{MemByteSource, VcdByteSource};
use crate::format::{find_value_transition, FloatFormat, Radix, Signedness};
use crate::project::{
    DisplayedNode, Marker, RenderStyle, SignalListDocument, SignalLocation, SignalRowConfig,
    ThemeMode,
};
use crate::vcd_header::{parse_vcd_header, ScopeId, SignalId, TimeUnit, Timescale, VcdHeader, VcdIdCode};
use crate::vcd_loader::{StreamingBodyScanner, VcdIndex};
use crate::waveform_db::SignalWave;
use std::collections::{BTreeSet, HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

pub enum WorkerEvent {
    VcdOpened {
        file_name: String,
        source: Arc<dyn VcdByteSource>,
        header: Arc<VcdHeader>,
    },
    WaveBatch {
        index: VcdIndex,
        waves: HashMap<VcdIdCode, SignalWave>,
    },
    SignalListLoaded(SignalListDocument),
    StatusMessage(String),
}

pub struct VcdViewerApp {
    pub file_name: String,
    pub source: Option<Arc<dyn VcdByteSource>>,
    pub header: Option<Arc<VcdHeader>>,
    pub waves: HashMap<VcdIdCode, SignalWave>,
    pub loaded_vcd_ids: HashSet<VcdIdCode>,
    pub index: VcdIndex,
    pub doc: SignalListDocument,

    // UI selection and modal state
    pub selected_locations: BTreeSet<SignalLocation>,
    pub primary_selected: Option<SignalLocation>,
    pub clipboard_signals: Vec<SignalRowConfig>,
    pub show_hierarchy_modal: bool,
    pub selected_scope: Option<ScopeId>,
    pub hierarchy_search_query: String,
    pub checked_signals: HashSet<SignalId>,

    // Toolbar search & marker state
    pub value_search_query: String,
    pub active_marker_idx: usize,
    pub status_message: String,

    // Split column widths
    pub signal_col_width: f32,
    pub value_col_width: f32,

    // Waveform drag zoom state
    pub drag_zoom_start_x: Option<f32>,
    pub drag_zoom_curr_x: Option<f32>,

    // Background worker channels
    pub event_tx: Sender<WorkerEvent>,
    pub event_rx: Receiver<WorkerEvent>,
    pub is_loading: bool,
}

impl Default for VcdViewerApp {
    fn default() -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let mut app = Self {
            file_name: String::new(),
            source: None,
            header: None,
            waves: HashMap::new(),
            loaded_vcd_ids: HashSet::new(),
            index: VcdIndex::default(),
            doc: SignalListDocument::default(),
            selected_locations: BTreeSet::new(),
            primary_selected: None,
            clipboard_signals: Vec::new(),
            show_hierarchy_modal: false,
            selected_scope: None,
            hierarchy_search_query: String::new(),
            checked_signals: HashSet::new(),
            value_search_query: String::new(),
            active_marker_idx: 0,
            status_message: "Ready — Load a VCD file or click 'Demo SoC' to explore".to_string(),
            signal_col_width: 240.0,
            value_col_width: 150.0,
            drag_zoom_start_x: None,
            drag_zoom_curr_x: None,
            event_tx,
            event_rx,
            is_loading: false,
        };
        app.load_demo_soc_vcd();
        app
    }
}

impl VcdViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app = Self::default();
        app.apply_theme(&cc.egui_ctx);
        app
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        match self.doc.theme {
            ThemeMode::Dark => {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = egui::Color32::from_rgb(24, 24, 37);
                visuals.window_fill = egui::Color32::from_rgb(30, 30, 46);
                visuals.extreme_bg_color = egui::Color32::from_rgb(17, 17, 27);
                ctx.set_visuals(visuals);
            }
            ThemeMode::Light => {
                let mut visuals = egui::Visuals::light();
                visuals.panel_fill = egui::Color32::from_rgb(242, 244, 248);
                visuals.window_fill = egui::Color32::from_rgb(255, 255, 255);
                visuals.extreme_bg_color = egui::Color32::from_rgb(255, 255, 255);
                ctx.set_visuals(visuals);
            }
        }
    }

    pub fn timescale(&self) -> Timescale {
        self.header
            .as_ref()
            .map(|h| h.timescale)
            .unwrap_or_default()
    }

    pub fn format_time(&self, raw_ticks: u64) -> String {
        let ts = self.timescale();
        let fs = (raw_ticks as f64) * (ts.factor as f64) * ts.unit.femtoseconds();
        let val = fs / self.doc.display_unit.femtoseconds();
        if (val - val.round()).abs() < 1e-6 {
            format!("{:.1} {}", val, self.doc.display_unit.label())
        } else {
            format!("{:.3} {}", val, self.doc.display_unit.label())
        }
    }

    pub fn load_vcd_from_source(&mut self, file_name: String, source: Arc<dyn VcdByteSource>) {
        match parse_vcd_header(source.as_ref()) {
            Ok(hdr) => {
                let hdr_arc = Arc::new(hdr);
                self.file_name = file_name;
                self.selected_scope = hdr_arc.root_scopes.first().copied();
                self.header = Some(hdr_arc);
                self.source = Some(source);
                self.waves.clear();
                self.loaded_vcd_ids.clear();
                self.index = VcdIndex::default();
                self.status_message = format!("Loaded VCD header: {}", self.file_name);
                self.sync_watched_signals();
            }
            Err(e) => {
                self.status_message = format!("Failed to parse VCD header: {}", e);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_vcd_file_path(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "trace.vcd".to_string());
        match FileByteSource::open(&path) {
            Ok(src) => {
                self.load_vcd_from_source(name, Arc::new(src));
                self.show_hierarchy_modal = true;
            }
            Err(e) => {
                self.status_message = format!("Error opening {}: {}", path.display(), e);
            }
        }
    }

    pub fn trigger_open_vcd_dialog(&mut self) {
        let tx = self.event_tx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("VCD Waveform", &["vcd"])
                    .pick_file()
                {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "trace.vcd".to_string());
                    match FileByteSource::open(&path) {
                        Ok(src) => {
                            let src_arc: Arc<dyn VcdByteSource> = Arc::new(src);
                            match parse_vcd_header(src_arc.as_ref()) {
                                Ok(hdr) => {
                                    let _ = tx.send(WorkerEvent::VcdOpened {
                                        file_name: name,
                                        source: src_arc,
                                        header: Arc::new(hdr),
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                        "Header error: {}",
                                        e
                                    )));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                "Open error: {}",
                                e
                            )));
                        }
                    }
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(handle) = rfd::AsyncFileDialog::new()
                    .add_filter("VCD Waveform", &["vcd"])
                    .pick_file()
                    .await
                {
                    let name = handle.file_name();
                    let bytes = handle.read().await;
                    let src_arc: Arc<dyn VcdByteSource> = Arc::new(MemByteSource::new(bytes));
                    match parse_vcd_header(src_arc.as_ref()) {
                        Ok(hdr) => {
                            let _ = tx.send(WorkerEvent::VcdOpened {
                                file_name: name,
                                source: src_arc,
                                header: Arc::new(hdr),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                "Header error: {}",
                                e
                            )));
                        }
                    }
                }
            });
        }
    }

    pub fn trigger_load_signal_list_dialog(&mut self) {
        let tx = self.event_tx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Signal List JSON", &["json"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match SignalListDocument::from_json(&content) {
                            Ok(doc) => {
                                let _ = tx.send(WorkerEvent::SignalListLoaded(doc));
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                    "Invalid signal list JSON: {}",
                                    e
                                )));
                            }
                        },
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                "Failed to read file: {}",
                                e
                            )));
                        }
                    }
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(handle) = rfd::AsyncFileDialog::new()
                    .add_filter("Signal List JSON", &["json"])
                    .pick_file()
                    .await
                {
                    let bytes = handle.read().await;
                    if let Ok(content) = String::from_utf8(bytes) {
                        if let Ok(doc) = SignalListDocument::from_json(&content) {
                            let _ = tx.send(WorkerEvent::SignalListLoaded(doc));
                        }
                    }
                }
            });
        }
    }

    pub fn trigger_save_signal_list_dialog(&mut self) {
        let Ok(json) = self.doc.to_json() else {
            return;
        };
        let tx = self.event_tx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Signal List JSON", &["json"])
                    .set_file_name("signals.vcdview.json")
                    .save_file()
                {
                    match std::fs::write(&path, json) {
                        Ok(_) => {
                            let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                "Saved signal list to {}",
                                path.display()
                            )));
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::StatusMessage(format!(
                                "Save error: {}",
                                e
                            )));
                        }
                    }
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(handle) = rfd::AsyncFileDialog::new()
                    .add_filter("Signal List JSON", &["json"])
                    .set_file_name("signals.vcdview.json")
                    .save_file()
                    .await
                {
                    let _ = handle.write(json.as_bytes()).await;
                    let _ = tx.send(WorkerEvent::StatusMessage(
                        "Saved signal list file".to_string(),
                    ));
                }
            });
        }
    }

    pub fn collect_watched_vcd_ids(&self) -> HashSet<VcdIdCode> {
        let mut set = HashSet::new();
        let Some(ref hdr) = self.header else {
            return set;
        };
        for node in &self.doc.rows {
            match node {
                DisplayedNode::Signal(cfg) => {
                    if let Some(&sid) = hdr.path_to_signal.get(&cfg.signal_path) {
                        set.insert(hdr.signals[sid.0].vcd_id);
                    }
                }
                DisplayedNode::Group { children, .. } => {
                    for cfg in children {
                        if let Some(&sid) = hdr.path_to_signal.get(&cfg.signal_path) {
                            set.insert(hdr.signals[sid.0].vcd_id);
                        }
                    }
                }
                DisplayedNode::Divider { .. } => {}
            }
        }
        set
    }

    pub fn sync_watched_signals(&mut self) {
        let Some(ref source) = self.source else {
            return;
        };
        let Some(ref header) = self.header else {
            return;
        };
        let needed = self.collect_watched_vcd_ids();
        let missing: HashSet<VcdIdCode> = needed
            .difference(&self.loaded_vcd_ids)
            .copied()
            .collect();

        if missing.is_empty() && self.index.is_complete {
            return;
        }

        for &id in &missing {
            self.loaded_vcd_ids.insert(id);
        }

        let scan_ids = if self.index.is_complete {
            missing
        } else {
            needed
        };

        let src_clone = Arc::clone(source);
        let hdr_clone = Arc::clone(header);
        let tx = self.event_tx.clone();
        self.is_loading = true;

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let mut scanner =
                    StreamingBodyScanner::new(src_clone.as_ref(), &hdr_clone, scan_ids, 8 * 1024 * 1024);
                while let Ok(true) = scanner.step_chunk() {
                    let _ = tx.send(WorkerEvent::StatusMessage(format!(
                        "Indexing VCD: {:.2} MB...",
                        scanner.index.bytes_indexed as f64 / (1024.0 * 1024.0)
                    )));
                }
                let _ = tx.send(WorkerEvent::WaveBatch {
                    index: scanner.index,
                    waves: scanner.waves,
                });
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut scanner =
                StreamingBodyScanner::new(src_clone.as_ref(), &hdr_clone, scan_ids, 2 * 1024 * 1024);
            while let Ok(true) = scanner.step_chunk() {}
            let _ = tx.send(WorkerEvent::WaveBatch {
                index: scanner.index,
                waves: scanner.waves,
            });
        }
    }

    pub fn poll_worker_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                WorkerEvent::VcdOpened {
                    file_name,
                    source,
                    header,
                } => {
                    self.file_name = file_name;
                    self.selected_scope = header.root_scopes.first().copied();
                    self.header = Some(header);
                    self.source = Some(source);
                    self.waves.clear();
                    self.loaded_vcd_ids.clear();
                    self.index = VcdIndex::default();
                    self.show_hierarchy_modal = true;
                    self.sync_watched_signals();
                }
                WorkerEvent::WaveBatch { index, waves } => {
                    let first_load = self.index.max_timestamp == 0 && index.max_timestamp > 0;
                    self.index = index;
                    for (k, v) in waves {
                        self.waves.insert(k, v);
                    }
                    if first_load {
                        self.doc
                            .viewport
                            .zoom_full(self.index.min_timestamp, self.index.max_timestamp);
                    }
                    self.is_loading = false;
                    self.status_message = format!(
                        "Loaded {} signals | Max Time: {} | Indexed {:.2} KB",
                        self.waves.len(),
                        self.format_time(self.index.max_timestamp),
                        self.index.bytes_indexed as f64 / 1024.0
                    );
                }
                WorkerEvent::SignalListLoaded(doc) => {
                    self.doc = doc;
                    self.selected_locations.clear();
                    self.primary_selected = None;
                    self.apply_theme(ctx);
                    self.sync_watched_signals();
                    self.status_message = "Loaded signal list configuration".to_string();
                }
                WorkerEvent::StatusMessage(msg) => {
                    self.status_message = msg;
                }
            }
        }
    }

    pub fn select_all_signals(&mut self) {
        self.selected_locations = self.doc.all_signal_locations();
        if self.primary_selected.is_none() {
            self.primary_selected = self.selected_locations.iter().next().copied();
        }
        self.status_message = format!("Selected {} signal(s)", self.selected_locations.len());
    }

    pub fn copy_selected_signals(&mut self) {
        if !self.selected_locations.is_empty() {
            self.clipboard_signals = self.doc.copy_locations(&self.selected_locations);
            self.status_message = format!("Copied {} signal(s)", self.clipboard_signals.len());
        }
    }

    pub fn cut_selected_signals(&mut self) {
        if !self.selected_locations.is_empty() {
            self.clipboard_signals = self.doc.cut_locations(&self.selected_locations);
            self.selected_locations.clear();
            self.primary_selected = None;
            self.status_message = format!("Cut {} signal(s)", self.clipboard_signals.len());
        }
    }

    pub fn paste_clipboard_signals(&mut self) {
        if !self.clipboard_signals.is_empty() {
            let anchor = self
                .primary_selected
                .or_else(|| self.selected_locations.iter().next_back().copied());
            let clips = self.clipboard_signals.clone();
            self.doc.insert_signals_after(anchor, &clips);
            self.sync_watched_signals();
            self.status_message = format!("Pasted {} signal(s)", clips.len());
        }
    }

    pub fn delete_selected_signals(&mut self) {
        if !self.selected_locations.is_empty() {
            let count = self.selected_locations.len();
            self.doc.delete_locations(&self.selected_locations);
            self.selected_locations.clear();
            self.primary_selected = None;
            self.status_message = format!("Deleted {} signal(s)", count);
        }
    }

    pub fn selected_signal_wave(&self) -> Option<(&SignalRowConfig, &SignalWave)> {
        let loc = self
            .primary_selected
            .or_else(|| self.selected_locations.iter().next().copied())?;
        let cfg = self.doc.get_signal(loc)?;
        let hdr = self.header.as_ref()?;
        let &sid = hdr.path_to_signal.get(&cfg.signal_path)?;
        let vcd_id = hdr.signals[sid.0].vcd_id;
        let wave = self.waves.get(&vcd_id)?;
        Some((cfg, wave))
    }

    pub fn jump_prev_edge(&mut self) {
        if let Some((_, wave)) = self.selected_signal_wave() {
            if let Some(t) = wave.prev_transition(self.doc.cursor_time) {
                self.doc.cursor_time = t;
                self.ensure_cursor_visible();
            }
        }
    }

    pub fn jump_next_edge(&mut self) {
        if let Some((_, wave)) = self.selected_signal_wave() {
            if let Some(t) = wave.next_transition(self.doc.cursor_time) {
                self.doc.cursor_time = t;
                self.ensure_cursor_visible();
            }
        }
    }

    pub fn search_selected_value(&mut self, forward: bool) {
        let query = self.value_search_query.clone();
        if let Some((cfg, wave)) = self.selected_signal_wave() {
            if let Some(t) = find_value_transition(
                wave,
                self.doc.cursor_time,
                forward,
                &query,
                cfg.radix,
                cfg.signedness,
            ) {
                self.doc.cursor_time = t;
                self.ensure_cursor_visible();
                self.status_message = format!("Found '{}' at {}", query, self.format_time(t));
            } else {
                self.status_message = format!("Value '{}' not found", query);
            }
        }
    }

    pub fn ensure_cursor_visible(&mut self) {
        let c = self.doc.cursor_time as f64;
        if c < self.doc.viewport.start_time || c > self.doc.viewport.end_time {
            let span = self.doc.viewport.span();
            let new_start = (c - span * 0.5).max(0.0);
            self.doc.viewport.start_time = new_start;
            self.doc.viewport.end_time = new_start + span;
        }
    }

    pub fn add_marker_at_cursor(&mut self) {
        let next_num = self.doc.markers.len() + 1;
        self.doc.markers.push(Marker {
            name: format!("M{}", next_num),
            timestamp: self.doc.cursor_time,
        });
        self.active_marker_idx = self.doc.markers.len() - 1;
    }

    pub fn load_demo_soc_vcd(&mut self) {
        let mut vcd = String::from(
            "$date 2026-10-17 $end\n\
             $version Rust VCD Viewer Demo SoC $end\n\
             $timescale 1 ns $end\n\
             $scope module top_soc $end\n\
               $scope module u_cpu_core $end\n\
                 $var wire 1 ! clk $end\n\
                 $var wire 1 \" rst_n $end\n\
                 $var reg 32 # pc [31:0] $end\n\
                 $scope module u_fpu $end\n\
                   $var reg 32 $ fp_result [31:0] $end\n\
                 $upscope $end\n\
               $upscope $end\n\
               $scope module u_axi_bus $end\n\
                 $var wire 1 % awvalid $end\n\
                 $var reg 4 & awaddr [3:0] $end\n\
               $upscope $end\n\
               $scope module u_sensor_adc $end\n\
                 $var reg 16 ' adc_sample [15:0] $end\n\
               $upscope $end\n\
             $upscope $end\n\
             $enddefinitions $end\n",
        );

        let fp32_vals: [u32; 8] = [
            0x0000_0000, // 0.0
            0x3F80_0000, // 1.0
            0x4000_0000, // 2.0
            0x4049_0FDB, // 3.1415927 (pi)
            0xC020_0000, // -2.5
            0x4228_0000, // 42.0
            0x7F80_0000, // +Inf
            0x3F00_0000, // 0.5
        ];

        for step in 0..=40u64 {
            let t = step * 5;
            let clk = step % 2;
            let rst_n = if t >= 15 { 1 } else { 0 };
            let pc: u32 = if rst_n == 0 {
                0x8000_0000
            } else {
                0x8000_0000 + ((step / 2) as u32) * 4
            };
            let fp_bits = fp32_vals[((step / 4) as usize) % fp32_vals.len()];
            let awvalid = if step % 6 == 2 || step % 6 == 3 { 1 } else { 0 };
            let awaddr = ((step / 2) & 0xF) as u8;
            let angle = (step as f64) * 0.28;
            let sine_i16 = (angle.sin() * 2048.0) as i16;
            let adc_u16 = sine_i16 as u16;

            vcd.push_str(&format!("#{}\n", t));
            vcd.push_str(&format!("{}!\n", clk));
            vcd.push_str(&format!("{}\"\n", rst_n));
            if step == 4 {
                // Inject an X state on PC at t=20ns for visual demonstration
                vcd.push_str("bx #\n");
            } else {
                vcd.push_str(&format!("b{:032b} #\n", pc));
            }
            vcd.push_str(&format!("b{:032b} $\n", fp_bits));
            vcd.push_str(&format!("{}%\n", awvalid));
            vcd.push_str(&format!("b{:04b} &\n", awaddr));
            vcd.push_str(&format!("b{:016b} '\n", adc_u16));
        }

        let src = Arc::new(MemByteSource::new(vcd.into_bytes()));
        self.doc = SignalListDocument {
            version: 1,
            theme: ThemeMode::Dark,
            display_unit: TimeUnit::Ns,
            cursor_time: 65,
            markers: vec![Marker {
                name: "M1".to_string(),
                timestamp: 125,
            }],
            viewport: crate::project::Viewport {
                start_time: 0.0,
                end_time: 200.0,
            },
            rows: vec![
                DisplayedNode::Group {
                    name: "CPU Clocks & Control".to_string(),
                    collapsed: false,
                    children: vec![
                        SignalRowConfig::new("top_soc.u_cpu_core.clk", 1),
                        SignalRowConfig::new("top_soc.u_cpu_core.rst_n", 1),
                        SignalRowConfig::new("top_soc.u_cpu_core.pc", 32),
                    ],
                },
                DisplayedNode::Divider {
                    label: "FPU & AXI Interconnect".to_string(),
                },
                DisplayedNode::Signal({
                    let mut cfg =
                        SignalRowConfig::new("top_soc.u_cpu_core.u_fpu.fp_result", 32);
                    cfg.radix = Radix::Float(FloatFormat::Fp32);
                    cfg.color_rgb = [249, 226, 175];
                    cfg
                }),
                DisplayedNode::Signal(SignalRowConfig::new("top_soc.u_axi_bus.awvalid", 1)),
                DisplayedNode::Signal(SignalRowConfig::new("top_soc.u_axi_bus.awaddr", 4)),
                DisplayedNode::Signal({
                    let mut cfg =
                        SignalRowConfig::new("top_soc.u_sensor_adc.adc_sample", 16);
                    cfg.radix = Radix::Dec;
                    cfg.signedness = Signedness::Signed2s;
                    cfg.render_style = RenderStyle::AnalogStep;
                    cfg.color_rgb = [250, 179, 135];
                    cfg
                }),
            ],
        };
        let initial_loc = SignalLocation {
            node_idx: 2,
            group_child_idx: None,
        };
        self.selected_locations.clear();
        self.selected_locations.insert(initial_loc);
        self.primary_selected = Some(initial_loc);
        self.load_vcd_from_source("demo_soc.vcd".to_string(), src);
    }
}

impl eframe::App for VcdViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker_events(ctx);

        // Keyboard shortcuts (disabled when typing in a text field or hierarchy modal is open)
        if !ctx.wants_keyboard_input() && !self.show_hierarchy_modal {
            ctx.input(|i| {
                let cmd_or_ctrl = i.modifiers.command || i.modifiers.ctrl;
                let has_cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut));
                let has_copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
                let has_paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));

                if cmd_or_ctrl && i.key_pressed(egui::Key::A) {
                    self.select_all_signals();
                } else if has_cut || (cmd_or_ctrl && i.key_pressed(egui::Key::X)) {
                    self.cut_selected_signals();
                } else if has_copy || (cmd_or_ctrl && i.key_pressed(egui::Key::C)) {
                    self.copy_selected_signals();
                } else if has_paste || (cmd_or_ctrl && i.key_pressed(egui::Key::V)) {
                    self.paste_clipboard_signals();
                } else if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                    self.delete_selected_signals();
                } else if !cmd_or_ctrl {
                    if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                        let max_t = self.index.max_timestamp.max(100) as f64;
                        self.doc.viewport.zoom_factor(0.7, 0.5, max_t);
                    }
                    if i.key_pressed(egui::Key::Minus) {
                        let max_t = self.index.max_timestamp.max(100) as f64;
                        self.doc.viewport.zoom_factor(1.4, 0.5, max_t);
                    }
                    if i.key_pressed(egui::Key::F) {
                        self.doc
                            .viewport
                            .zoom_full(self.index.min_timestamp, self.index.max_timestamp);
                    }
                    if i.key_pressed(egui::Key::N) {
                        self.jump_next_edge();
                    }
                    if i.key_pressed(egui::Key::P) {
                        self.jump_prev_edge();
                    }
                }
            });
        }

        egui::TopBottomPanel::top("top_toolbar_panel").show(ctx, |ui| {
            toolbar::render_top_toolbar(self, ctx, ui);
        });

        egui::TopBottomPanel::bottom("bottom_status_bar")
            .exact_height(24.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(format!("📄 {}", self.file_name));
                    ui.separator();
                    ui.label(&self.status_message);
                    if self.is_loading {
                        ui.separator();
                        ui.spinner();
                        ui.label("Streaming VCD chunks...");
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(ctx.style().visuals.panel_fill))
            .show(ctx, |ui| {
                let visible_rows = self.doc.flatten_visible_rows(self.header.as_deref());
                let header_h = 28.0;
                let row_h = 26.0;

                // Top column headers + Time Ruler
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    signal_list::render_header_columns(self, ui, header_h);
                    ui.separator();
                    waveform_canvas::render_time_ruler(self, ui, header_h);
                });
                ui.separator();

                // Synchronized vertical scroll area for Signal List + Value Column + Waveform Canvas
                egui::ScrollArea::vertical()
                    .id_salt("main_waveform_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            signal_list::render_signal_and_value_panes(
                                self,
                                ui,
                                &visible_rows,
                                row_h,
                            );
                            ui.separator();
                            waveform_canvas::render_waveform_canvas(
                                self,
                                ui,
                                &visible_rows,
                                row_h,
                            );
                        });
                    });
            });

        hierarchy_modal::render_hierarchy_modal(self, ctx);
    }
}
