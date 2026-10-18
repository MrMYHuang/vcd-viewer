#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use vcd_viewer::ui::VcdViewerApp;

#[cfg(not(target_arch = "wasm32"))]
fn load_app_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let icon_bytes = include_bytes!("../assets/icon-256.png");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(std::sync::Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 820.0])
        .with_min_inner_size([800.0, 480.0])
        .with_title("VCD Waveform Viewer");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "VCD Waveform Viewer",
        native_options,
        Box::new(|cc| {
            let mut app = VcdViewerApp::new(cc);
            if let Some(arg_path) = std::env::args().nth(1) {
                app.open_vcd_file_path(std::path::PathBuf::from(arg_path));
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("vcd_viewer_canvas")
            .expect("Failed to find vcd_viewer_canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("vcd_viewer_canvas was not a HtmlCanvasElement");

        let web_options = eframe::WebOptions::default();
        let _ = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(VcdViewerApp::new(cc)))),
            )
            .await;
    });
}
