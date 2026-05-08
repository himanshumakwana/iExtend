use ix_transport::LocalEndpoint;

#[derive(Default)]
pub struct TrayApp {
    daemon_status: Option<String>,
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("iExtend");
            ui.label("Plan 2 scaffold — no real connection yet.");
            ui.separator();

            let endpoint = LocalEndpoint::default_for_user();
            ui.label(format!("Endpoint: {}", endpoint.0));

            if ui.button("Ping iextendd (placeholder)").clicked() {
                self.daemon_status = Some("Plan 6 wires this up.".into());
            }
            if let Some(s) = &self.daemon_status { ui.label(s); }
        });
    }
}
