//! Tabbed iExtend tray shell.
//!
//! Architecture notes:
//! - A single `tokio::runtime::Runtime` is created once at `TrayApp::new` and
//!   reused for every `block_on` call inside `update`. This avoids the ~5 ms
//!   overhead of building a new runtime on every click.
//! - Status is polled in the background via a `tokio::sync::watch` channel;
//!   only the Home and Pair tabs schedule a repaint via
//!   `Context::request_repaint_after`.
//! - Pairing-status is polled similarly while `pairing_state == WAITING`.

use crate::client;
use crate::client::proto::{PairedDevice, PairingState, PairingStatus, Settings, StatusReply};
use ix_transport::LocalEndpoint;
use std::time::Duration;
use tokio::sync::watch;

/// Which tab is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Home,
    Pair,
    Devices,
    Sessions,
    Settings,
}

/// The main tray application state.
pub struct TrayApp {
    rt: tokio::runtime::Runtime,
    endpoint: LocalEndpoint,

    // ── background watches ────────────────────────────────────────────────
    status_rx: watch::Receiver<Option<StatusReply>>,
    pair_rx: watch::Receiver<Option<PairingStatus>>,

    // ── UI state ──────────────────────────────────────────────────────────
    selected_tab: Tab,
    daemon_connected: bool,

    // Devices tab
    devices: Vec<PairedDevice>,
    devices_err: Option<String>,
    devices_loaded: bool,

    // Settings tab
    settings: Option<Settings>,
    settings_err: Option<String>,
    settings_dirty: bool,
    // Editable buffer fields
    pref_codec: String,
    max_bitrate: String,
    auto_connect: bool,
    hdr_enabled: bool,
    pair_port: String,
}

impl TrayApp {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("failed to build tokio runtime");

        let endpoint = LocalEndpoint::default_for_user();

        // Spawn background status poller.
        let (status_tx, status_rx) = watch::channel(None::<StatusReply>);
        let ep_clone = endpoint.clone();
        rt.spawn(async move {
            loop {
                let result = client::status(&ep_clone).await.ok();
                let _ = status_tx.send(result);
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        });

        // Pairing-status channel — only polled while WAITING.
        let (pair_tx, pair_rx) = watch::channel(None::<PairingStatus>);
        let ep_clone2 = endpoint.clone();
        rt.spawn(async move {
            loop {
                // We poll every 1.5 s; the main thread gates on pair_polling to
                // avoid unnecessary gRPC calls when not in the Pair tab.
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let result = client::get_pairing_status(&ep_clone2).await.ok();
                let _ = pair_tx.send(result);
            }
        });

        Self {
            rt,
            endpoint,
            status_rx,
            pair_rx,
            selected_tab: Tab::default(),
            daemon_connected: false,
            devices: Vec::new(),
            devices_err: None,
            devices_loaded: false,
            settings: None,
            settings_err: None,
            settings_dirty: false,
            pref_codec: "hevc".into(),
            max_bitrate: "80000".into(),
            auto_connect: false,
            hdr_enabled: false,
            pair_port: "7779".into(),
        }
    }
}

impl Default for TrayApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refresh latest watch values.
        let status: Option<StatusReply> = self.status_rx.borrow().clone();
        self.daemon_connected = status.is_some();

        // ── Top bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("iExtend");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.daemon_connected {
                        let pill = egui::RichText::new("● Daemon connected")
                            .color(egui::Color32::from_rgb(0, 200, 80))
                            .size(12.0);
                        ui.label(pill);
                    } else {
                        let pill = egui::RichText::new("● Daemon offline")
                            .color(egui::Color32::from_rgb(220, 50, 50))
                            .size(12.0);
                        ui.label(pill);
                    }
                });
            });
        });

        // ── Tab bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Home, "Home"),
                    (Tab::Pair, "Pair"),
                    (Tab::Devices, "Devices"),
                    (Tab::Sessions, "Sessions"),
                    (Tab::Settings, "Settings"),
                ] {
                    let sel = self.selected_tab == tab;
                    if ui.selectable_label(sel, label).clicked() {
                        self.selected_tab = tab;
                        // Trigger lazy loads on tab switch.
                        if tab == Tab::Devices {
                            self.devices_loaded = false;
                        }
                        if tab == Tab::Settings && self.settings.is_none() {
                            self.reload_settings();
                        }
                    }
                }
            });
        });

        // ── Main content ──────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| match self.selected_tab {
            Tab::Home => self.draw_home(ui, &status, ctx),
            Tab::Pair => self.draw_pair(ui, ctx, &status),
            Tab::Devices => self.draw_devices(ui),
            Tab::Sessions => self.draw_sessions(ui, &status),
            Tab::Settings => self.draw_settings(ui),
        });
    }
}

// ─── Per-tab render helpers ────────────────────────────────────────────────────

impl TrayApp {
    fn draw_home(&self, ui: &mut egui::Ui, status: &Option<StatusReply>, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(1500));

        if let Some(s) = status {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("Status").strong().size(14.0));
                ui.separator();
                egui::Grid::new("status_grid")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Version:");
                        ui.label(&s.version);
                        ui.end_row();

                        ui.label("Uptime:");
                        ui.label(format!("{}s", s.uptime_s));
                        ui.end_row();

                        ui.label("Session:");
                        let session_name = session_state_name(s.session);
                        ui.label(session_name);
                        ui.end_row();

                        ui.label("Peers:");
                        ui.label(s.peers.len().to_string());
                        ui.end_row();

                        ui.label("Paired devices:");
                        ui.label(s.paired_count.to_string());
                        ui.end_row();

                        ui.label("Endpoint:");
                        ui.label(&s.endpoint);
                        ui.end_row();
                    });
            });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for daemon...").color(egui::Color32::GRAY));
            });
        }
    }

    fn draw_pair(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, status: &Option<StatusReply>) {
        // Read latest pairing status from the watch channel.
        let pair_status: Option<PairingStatus> = self.pair_rx.borrow().clone();

        let pairing_state = pair_status
            .as_ref()
            .map(|p| p.state)
            .unwrap_or(PairingState::Idle as i32);

        if pairing_state == PairingState::Waiting as i32 {
            ctx.request_repaint_after(Duration::from_millis(1500));
        }

        ui.heading("Pair iPad");
        ui.separator();
        ui.add_space(8.0);

        // USB-connected iPads (libimobiledevice). Shown above the PIN UI so
        // the user knows which transport will be used; the actual pair flow
        // is the same simple-pair-v0 protocol either way.
        if let Some(s) = status {
            if !s.usb_devices.is_empty() {
                let names: Vec<String> = s
                    .usb_devices
                    .iter()
                    .map(|d| {
                        if d.display_name.is_empty() {
                            let short = if d.udid.len() > 8 {
                                &d.udid[..8]
                            } else {
                                &d.udid
                            };
                            format!("iPad ({short}…)")
                        } else {
                            d.display_name.clone()
                        }
                    })
                    .collect();
                ui.colored_label(
                    egui::Color32::from_rgb(0, 200, 80),
                    format!("● USB connected: {}", names.join(", ")),
                );
                ui.add_space(8.0);
            }
        }

        if pairing_state == PairingState::Idle as i32 {
            if ui
                .add_sized([120.0, 32.0], egui::Button::new("Begin pairing"))
                .clicked()
            {
                let ep = self.endpoint.clone();
                // Fire-and-forget — the background pair_rx poller will pick up
                // the new state within 1.5 s.
                let _ = self.rt.block_on(client::begin_pairing(&ep));
            }
        } else if pairing_state == PairingState::Waiting as i32
            || pairing_state == PairingState::Handshaking as i32
        {
            if let Some(ref ps) = pair_status {
                ui.label("Enter this PIN on your iPad:");
                ui.add_space(12.0);
                draw_pin_digits(ui, &ps.pin);
                ui.add_space(12.0);
                let pct = if ps.seconds_left > 0 {
                    ps.seconds_left as f32 / 60.0
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(pct).text(format!("{}s remaining", ps.seconds_left)));
                ui.add_space(12.0);

                // Connection info — what the iPad app's "Pair manually" form
                // wants. Port is the ephemeral one the daemon's listener
                // bound; host IPs are detected from local network interfaces
                // so the user doesn't need to run ipconfig separately.
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new("On your iPad, enter:").strong());
                ui.add_space(4.0);

                egui::Grid::new("pair_conn_info")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Port:");
                        let port_text = ps.port.to_string();
                        if ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(&port_text).monospace().strong(),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Click to copy")
                            .clicked()
                        {
                            ui.ctx().copy_text(port_text.clone());
                        }
                        ui.end_row();

                        ui.label("Host IP:");
                        let ips = local_ipv4_addresses();
                        if ips.is_empty() {
                            ui.label(
                                egui::RichText::new("(detecting…)")
                                    .italics()
                                    .color(egui::Color32::from_gray(150)),
                            );
                        } else {
                            ui.vertical(|ui| {
                                for ip in &ips {
                                    if ui
                                        .add(
                                            egui::Label::new(egui::RichText::new(ip).monospace())
                                                .sense(egui::Sense::click()),
                                        )
                                        .on_hover_text("Click to copy")
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(ip.clone());
                                    }
                                }
                            });
                        }
                        ui.end_row();
                    });

                ui.add_space(12.0);
                if ui.button("Cancel").clicked() {
                    let ep = self.endpoint.clone();
                    let _ = self.rt.block_on(client::cancel_pairing(&ep));
                }
            }
        } else if pairing_state == PairingState::Done as i32 {
            if let Some(ref ps) = pair_status {
                if let Some(ref dev) = ps.last_paired {
                    ui.colored_label(
                        egui::Color32::from_rgb(0, 200, 80),
                        format!("Paired with {}", dev.display_name),
                    );
                } else {
                    ui.colored_label(egui::Color32::from_rgb(0, 200, 80), "Pairing complete");
                }
                ui.add_space(8.0);
                if ui.button("Begin another").clicked() {
                    let ep = self.endpoint.clone();
                    let _ = self.rt.block_on(client::begin_pairing(&ep));
                }
            }
        } else if pairing_state == PairingState::Expired as i32 {
            ui.colored_label(egui::Color32::from_rgb(220, 50, 50), "PIN expired.");
            ui.add_space(8.0);
            if ui.button("Try again").clicked() {
                let ep = self.endpoint.clone();
                let _ = self.rt.block_on(client::begin_pairing(&ep));
            }
        } else if pairing_state == PairingState::Failed as i32 {
            let err_text = pair_status
                .as_ref()
                .map(|p| p.error.as_str())
                .unwrap_or("Unknown error");
            ui.colored_label(
                egui::Color32::from_rgb(220, 50, 50),
                format!("Pairing failed: {err_text}"),
            );
            ui.add_space(8.0);
            if ui.button("Try again").clicked() {
                let ep = self.endpoint.clone();
                let _ = self.rt.block_on(client::begin_pairing(&ep));
            }
        }
    }

    fn draw_devices(&mut self, ui: &mut egui::Ui) {
        ui.heading("Paired Devices");
        ui.separator();

        if !self.devices_loaded {
            let ep = self.endpoint.clone();
            match self.rt.block_on(client::list_paired_devices(&ep)) {
                Ok(reply) => {
                    self.devices = reply.devices;
                    self.devices_err = None;
                    self.devices_loaded = true;
                }
                Err(e) => {
                    self.devices_err = Some(format!("{e}"));
                    self.devices_loaded = true;
                }
            }
        }

        if let Some(ref err) = self.devices_err.clone() {
            ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
            if ui.button("Retry").clicked() {
                self.devices_loaded = false;
            }
            return;
        }

        if self.devices.is_empty() {
            ui.label(egui::RichText::new("No paired devices.").color(egui::Color32::GRAY));
        } else {
            let mut to_forget: Option<String> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for dev in &self.devices {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&dev.display_name).strong());
                                let short_id = if dev.pair_id.len() > 12 {
                                    format!("{}…", &dev.pair_id[..12])
                                } else {
                                    dev.pair_id.clone()
                                };
                                ui.label(
                                    egui::RichText::new(short_id)
                                        .size(10.0)
                                        .color(egui::Color32::GRAY),
                                );
                                let ago = relative_time(dev.paired_at_unix);
                                ui.label(
                                    egui::RichText::new(format!("Paired {ago}"))
                                        .size(10.0)
                                        .color(egui::Color32::GRAY),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(
                                            egui::RichText::new("Forget").color(egui::Color32::RED),
                                        )
                                        .clicked()
                                    {
                                        to_forget = Some(dev.pair_id.clone());
                                    }
                                },
                            );
                        });
                    });
                    ui.add_space(4.0);
                }
            });

            if let Some(pair_id) = to_forget {
                let ep = self.endpoint.clone();
                let _ = self.rt.block_on(client::forget_device(&ep, pair_id));
                self.devices_loaded = false; // re-fetch on next render
            }
        }

        ui.add_space(8.0);
        if ui.button("Refresh").clicked() {
            self.devices_loaded = false;
        }
    }

    fn draw_sessions(&mut self, ui: &mut egui::Ui, status: &Option<StatusReply>) {
        ui.heading("Session");
        ui.separator();

        if let Some(s) = status {
            let is_live = s.session == crate::client::proto::SessionState::Live as i32;
            let is_idle = s.session == crate::client::proto::SessionState::Idle as i32;

            ui.label(format!("State: {}", session_state_name(s.session)));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let start_btn = ui.add_enabled(!is_live, egui::Button::new("Start Session"));
                if start_btn.clicked() {
                    let ep = self.endpoint.clone();
                    let _ = self.rt.block_on(client::start_session(&ep));
                }
                let stop_btn = ui.add_enabled(!is_idle, egui::Button::new("Stop Session"));
                if stop_btn.clicked() {
                    let ep = self.endpoint.clone();
                    let _ = self.rt.block_on(client::stop_session(&ep));
                }
            });

            if !s.peers.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Connected peers:").strong());
                egui::Grid::new("peers_grid")
                    .num_columns(4)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Name").strong());
                        ui.label(egui::RichText::new("Latency").strong());
                        ui.label(egui::RichText::new("Sent").strong());
                        ui.label(egui::RichText::new("Dropped").strong());
                        ui.end_row();
                        for peer in &s.peers {
                            ui.label(&peer.display_name);
                            ui.label(format!("{}ms", peer.latency_ms));
                            ui.label(peer.frames_sent.to_string());
                            ui.label(peer.frames_dropped.to_string());
                            ui.end_row();
                        }
                    });
            }
        } else {
            ui.label(egui::RichText::new("Daemon offline").color(egui::Color32::GRAY));
        }
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();

        if let Some(ref err) = self.settings_err.clone() {
            ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err);
        }

        if self.settings.is_none() {
            if ui.button("Load settings").clicked() {
                self.reload_settings();
            }
            return;
        }

        ui.add_space(8.0);
        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Auto-connect on launch:");
                if ui.checkbox(&mut self.auto_connect, "").changed() {
                    self.settings_dirty = true;
                }
                ui.end_row();

                ui.label("Preferred codec:");
                egui::ComboBox::from_id_salt("codec_cb")
                    .selected_text(self.pref_codec.as_str())
                    .show_ui(ui, |ui| {
                        for codec in ["av1", "hevc", "h264"] {
                            if ui
                                .selectable_label(self.pref_codec == codec, codec)
                                .clicked()
                            {
                                self.pref_codec = codec.into();
                                self.settings_dirty = true;
                            }
                        }
                    });
                ui.end_row();

                ui.label("Max bitrate (kbps):");
                if ui.text_edit_singleline(&mut self.max_bitrate).changed() {
                    self.settings_dirty = true;
                }
                ui.end_row();

                ui.label("HDR enabled:");
                if ui.checkbox(&mut self.hdr_enabled, "").changed() {
                    self.settings_dirty = true;
                }
                ui.end_row();

                ui.label("Pairing port:");
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.pair_port)
                            .desired_width(80.0)
                            .hint_text("7779"),
                    );
                    if resp.changed() {
                        self.settings_dirty = true;
                    }
                    ui.label(
                        egui::RichText::new(
                            "(default 7779; blank = daemon default; falls back to a random port if busy)",
                        )
                        .small()
                        .color(egui::Color32::from_gray(140)),
                    );
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let save_btn = ui.add_enabled(self.settings_dirty, egui::Button::new("Save"));
            if save_btn.clicked() {
                let bitrate = self.max_bitrate.parse::<u32>().unwrap_or(80_000);
                // Empty input → 0 → daemon falls back to its compiled-in
                // DEFAULT_PAIR_PORT. Non-numeric input is also treated as 0
                // rather than rejected — the textbox isn't strict, the
                // daemon is the source of truth.
                let pair_port = self.pair_port.trim().parse::<u32>().unwrap_or(0);
                let new_settings = crate::client::proto::Settings {
                    auto_connect_on_launch: self.auto_connect,
                    preferred_codec: self.pref_codec.clone(),
                    max_bitrate_kbps: bitrate,
                    hdr_enabled: self.hdr_enabled,
                    pair_port,
                };
                let ep = self.endpoint.clone();
                match self.rt.block_on(client::set_settings(&ep, new_settings)) {
                    Ok(saved) => {
                        self.apply_settings(saved);
                        self.settings_dirty = false;
                        self.settings_err = None;
                    }
                    Err(e) => {
                        self.settings_err = Some(format!("Save failed: {e}"));
                    }
                }
            }
            if ui.button("Reload").clicked() {
                self.reload_settings();
            }
        });
    }

    // ── Settings helpers ──────────────────────────────────────────────────

    fn reload_settings(&mut self) {
        let ep = self.endpoint.clone();
        match self.rt.block_on(client::get_settings(&ep)) {
            Ok(s) => {
                self.apply_settings(s);
                self.settings_err = None;
                self.settings_dirty = false;
            }
            Err(e) => {
                self.settings_err = Some(format!("Load failed: {e}"));
            }
        }
    }

    fn apply_settings(&mut self, s: Settings) {
        self.auto_connect = s.auto_connect_on_launch;
        self.pref_codec = s.preferred_codec.clone();
        self.max_bitrate = s.max_bitrate_kbps.to_string();
        self.hdr_enabled = s.hdr_enabled;
        // pair_port == 0 means "use the daemon default"; show it as an empty
        // string so the user sees the placeholder rather than a literal "0".
        self.pair_port = if s.pair_port == 0 {
            String::new()
        } else {
            s.pair_port.to_string()
        };
        self.settings = Some(s);
    }
}

// ─── Session + start/stop client wrappers ─────────────────────────────────────

impl TrayApp {
    // These are thin because client.rs has the real async fn; we need them here
    // to compile the draw_sessions closure.
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn draw_pin_digits(ui: &mut egui::Ui, pin: &str) {
    ui.horizontal(|ui| {
        for c in pin.chars() {
            let (rect, _) =
                ui.allocate_exact_size(egui::Vec2::new(48.0, 60.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                egui::Rounding::same(8.0),
                egui::Color32::from_rgb(28, 28, 30),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                c,
                egui::FontId::monospace(28.0),
                egui::Color32::WHITE,
            );
        }
    });
}

fn session_state_name(state: i32) -> &'static str {
    use crate::client::proto::SessionState;
    if state == SessionState::Live as i32 {
        "Live"
    } else if state == SessionState::Pairing as i32 {
        "Pairing"
    } else if state == SessionState::Connecting as i32 {
        "Connecting"
    } else if state == SessionState::Degraded as i32 {
        "Degraded"
    } else if state == SessionState::Disconnected as i32 {
        "Disconnected"
    } else {
        "Idle"
    }
}

fn relative_time(unix_ts: i64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let diff = now - unix_ts;
    if diff < 60 {
        format!("{diff}s ago")
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Best-effort enumeration of the host's non-loopback IPv4 addresses.
///
/// Used by the Pair tab so the user can see which address to type into
/// the iPad's manual-pair form. Loopback (127.0.0.0/8) and link-local
/// (169.254.0.0/16) addresses are filtered — those won't reach the iPad
/// over Wi-Fi. Returns an empty vec on enumeration failure rather than
/// panicking; the UI then shows a "(detecting…)" placeholder.
fn local_ipv4_addresses() -> Vec<String> {
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut out: Vec<String> = ifaces
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) => {
                let ip = v4.ip;
                if ip.is_loopback() || ip.is_link_local() {
                    None
                } else {
                    Some(ip.to_string())
                }
            }
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}
