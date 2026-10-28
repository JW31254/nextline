#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
mod hotkey;
mod links;
mod model;
mod platform;
mod storage;
mod theme;
mod typer;
mod update;

fn main() -> eframe::Result {
    if !platform::acquire_single_instance() {
        platform::focus_existing_instance();
        return Ok(());
    }

    let core = core::Core::new(storage::load());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(platform::WINDOW_TITLE)
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([820.0, 520.0])
            .with_icon(app::window_icon()),
        ..Default::default()
    };

    eframe::run_native(platform::WINDOW_TITLE, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, core)))))
}
