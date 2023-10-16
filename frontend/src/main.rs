#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use egui::Vec2;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    env_logger::init();

    let size: Vec2 = [900.0, 600.0].into();

    let native_options = eframe::NativeOptions {
        initial_window_size: Some(size),
        min_window_size: Some(size),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Lambda Benchmark",
        native_options,
        Box::new(|cc| Box::new(frontend::LambdaBenchmark::new(cc))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id",
                web_options,
                Box::new(|cc| Box::new(frontend::LambdaBenchmark::new(cc))),
            )
            .await
            .expect("failed to start eframe");
    });
}
