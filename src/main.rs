mod app;
mod components;
mod db;
mod faker;
mod schema_format;
mod session;
mod style;
mod tabs;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([750.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PG Tables",
        options,
        Box::new(|cc| Ok(Box::new(app::PgTablesApp::new(cc)))),
    )
}
