//! The eframe update loop: navigation, page routing, modal and toast.

use eframe::egui;

use crate::engine_bridge;
use crate::widgets;

use super::App;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.time = ctx.input(|i| i.time);
        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.theme_tick(ctx);
        self.poll_events();
        self.persist_traffic_tick();

        // Bottom navigation.
        let mut nav_clicked: Option<usize> = None;
        egui::TopBottomPanel::bottom("nav")
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                widgets::nav_bar(ui, self.palette, self.page, &mut nav_clicked);
            });
        if let Some(i) = nav_clicked {
            self.page = i;
        }

        // Main content.
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(self.palette.bg)
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| match self.page {
                0 => self.draw_home(ui),
                1 => self.draw_configs(ui),
                2 => self.draw_logs(ui),
                _ => self.draw_more(ui),
            });

        // Modal on top of everything.
        let modal_open = self.draw_modal(ctx);

        // Toast.
        if let Some(toast) = &self.toast {
            toast.draw(ctx, self.palette);
        }
        if self.toast.as_ref().is_some_and(|t| t.expired(self.time)) {
            self.toast = None;
        }

        // Keep animating while connected/connecting or while a toast shows.
        let state = self.bridge.shared().state();
        if state == engine_bridge::STATE_CONNECTED || state == engine_bridge::STATE_CONNECTING {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        } else if self.toast.is_some() || modal_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}
