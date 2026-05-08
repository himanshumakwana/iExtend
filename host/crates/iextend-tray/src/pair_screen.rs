#![allow(dead_code)] // Plan 5 will wire this into the live UI; for now it's an unused module.

//! egui pair screen — shows the active 4-digit PIN with a 60-second
//! countdown. Pulls the PIN from the daemon over the existing localhost gRPC
//! channel; for Plan 7 we render directly with a dummy PIN since the gRPC
//! `StartPairing` RPC isn't defined until Plan 5 closes the session lifecycle.

use std::time::Instant;

/// Pairing-screen state: holds the active PIN and the deadline.
pub struct PairScreen {
    pub pin: String,
    pub started: Instant,
    pub window_secs: u64,
}

impl PairScreen {
    /// New screen with a freshly-generated dummy PIN. Replace
    /// [`Self::pin`] from the daemon's response once gRPC RPC lands in Plan 5.
    pub fn new() -> Self {
        Self {
            pin: dummy_pin(),
            started: Instant::now(),
            window_secs: 60,
        }
    }

    /// Seconds remaining; clamped at zero.
    pub fn remaining(&self) -> u64 {
        self.window_secs
            .saturating_sub(self.started.elapsed().as_secs())
    }

    /// True once the PIN window has expired.
    pub fn expired(&self) -> bool {
        self.remaining() == 0
    }

    /// Render the screen. Returns true when the user has clicked Cancel.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading("Pair iPad");
        ui.separator();
        ui.add_space(8.0);

        if self.expired() {
            ui.colored_label(egui::Color32::RED, "PIN expired. Click Pair iPad to try again.");
            return ui.button("Close").clicked();
        }

        ui.label("Enter this PIN on your iPad:");
        ui.add_space(12.0);

        // Big monospaced PIN cells.
        ui.horizontal(|ui| {
            for c in self.pin.chars() {
                let (rect, _) = ui.allocate_exact_size(
                    egui::Vec2::new(48.0, 60.0),
                    egui::Sense::hover(),
                );
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

        ui.add_space(12.0);
        let pct = self.remaining() as f32 / self.window_secs as f32;
        ui.add(egui::ProgressBar::new(pct).text(format!("{}s", self.remaining())));
        ui.add_space(8.0);
        ui.button("Cancel").clicked()
    }
}

impl Default for PairScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn dummy_pin() -> String {
    // Real PIN generation lives daemon-side via
    // `ix_rtc::pairing::generate_pin`; this is a placeholder for the
    // egui screen until the gRPC `StartPairing` is added in Plan 5.
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(0)
        % 10_000;
    format!("{n:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_screen_has_60s() {
        let s = PairScreen::new();
        assert!(s.remaining() <= 60);
        assert!(!s.expired());
    }

    #[test]
    fn pin_is_4_digits() {
        let s = PairScreen::new();
        assert_eq!(s.pin.len(), 4);
        assert!(s.pin.chars().all(|c| c.is_ascii_digit()));
    }
}
