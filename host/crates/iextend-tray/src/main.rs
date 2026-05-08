mod app;

fn main() -> eframe::Result<()> {
    init_logging();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 240.0])
            .with_min_inner_size([320.0, 200.0])
            .with_title("iExtend"),
        ..Default::default()
    };
    eframe::run_native("iExtend", opts, Box::new(|_| Ok(Box::<app::TrayApp>::default())))
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}
