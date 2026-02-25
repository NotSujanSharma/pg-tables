mod app;
mod db;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([950.0, 650.0])
            .with_min_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PG Tables",
        options,
        Box::new(|cc| Ok(Box::new(app::PgTablesApp::new(cc)))),
    )
}

use eframe::egui;
