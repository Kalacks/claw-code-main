use eframe::egui;

use super::ClawGuiApp;

pub(super) fn render_chat_tab_v4(app: &mut ClawGuiApp, ctx: &egui::Context) {
    egui::SidePanel::left("claw_gui_threads_v4")
        .resizable(true)
        .default_width(300.0)
        .min_width(240.0)
        .show(ctx, |ui| app.render_thread_sidebar_v4_legacy(ui));

    egui::SidePanel::right("claw_gui_inspector_v4")
        .resizable(true)
        .default_width(320.0)
        .min_width(280.0)
        .show(ctx, |ui| app.render_inspector_v3_legacy(ui));

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| app.render_chat_stream_v6_legacy(ui));
    });
}

pub(super) fn render_thread_sidebar_v4(app: &mut ClawGuiApp, ui: &mut egui::Ui) {
    app.render_thread_sidebar_v4_legacy(ui);
}

pub(super) fn render_inspector_v3(app: &mut ClawGuiApp, ui: &mut egui::Ui) {
    app.render_inspector_v3_legacy(ui);
}

pub(super) fn render_chat_stream_v6(app: &mut ClawGuiApp, ui: &mut egui::Ui) {
    app.render_chat_stream_v6_legacy(ui);
}
