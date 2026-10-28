//! The egui front-end. All state lives in `Core`; this file only draws it and queues actions.

use crate::core::{Core, Source, Status};
use crate::hotkey::{Combo, HotkeyAction};
use crate::model::{self, AfterKey, AppData, NewlineMode, Script, Settings, Step, ThemeChoice};
use crate::platform;
use crate::update;
use crate::theme::{
    self, Palette, card_frame, chip, danger_button, icon_button, keycap, one_line, palette, primary_button, small_caps,
    step_segments,
};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Event, Frame, Key, Layout, Margin, Panel, RichText, Sense, Stroke, TextEdit, vec2,
};
use egui_phosphor::regular as icons;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::HWND;

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
const SPEED_PRESETS: [(&str, u32); 3] = [("Slow", 12), ("Normal", 25), ("Fast", 50)];
const NORMAL_MIN_SIZE: [f32; 2] = [820.0, 520.0];
const COMPACT_SIZE: [f32; 2] = [460.0, 330.0];

static QUITTING: AtomicBool = AtomicBool::new(false);

enum Act {
    Type(Source),
    Stop,
    Import,
    ExportAll,
    ExportScript(String),
    ToggleCompact,
    StartRecording(HotkeyAction),
    StopRecording,
    SetHotkey(HotkeyAction, Combo),
    ResetHotkeys,
}

enum StepOp {
    Select(usize),
    Edit(usize),
    Up(usize),
    Down(usize),
    Duplicate(usize),
    Delete(usize),
    Add,
}

enum ScriptOp {
    Duplicate,
    Delete,
}

#[derive(Default)]
struct History {
    undo: Vec<Vec<Script>>,
    redo: Vec<Vec<Script>>,
    last_push: Option<Instant>,
    force_break: bool,
}

pub struct App {
    core: Arc<Core>,
    query: String,
    category: Option<String>,
    compact: bool,
    normal_size: Option<egui::Vec2>,
    history: History,
    committed: Vec<Script>,
    committed_settings: Settings,
    committed_selection: (String, String),
    recording: Option<HotkeyAction>,
    hotkey_message: Option<String>,
    settings_open: bool,
    hwnd: Option<HWND>,
    applied_theme: Option<ThemeChoice>,
    applied_capture: Option<bool>,
    logo: egui::TextureHandle,
    last_step_seen: String,
    scroll_to_step: bool,
    /// The step whose editor is open. Independent from the "next" step so skipping never moves the layout.
    editing: Option<String>,
    focus_editor: bool,
    /// Update version the user hid with the close button this session.
    update_dismissed: Option<String>,
    focus_search: bool,
    _tray: Option<TrayIcon>,
}

fn load_icon_image() -> image::RgbaImage {
    image::load_from_memory(ICON_PNG).expect("embedded icon is a valid PNG").to_rgba8()
}

pub fn window_icon() -> egui::IconData {
    let img = load_icon_image();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

fn show_main_window(ctx: &egui::Context) {
    if let Some(hwnd) = platform::find_main_window(true) {
        platform::show_window(hwnd);
    }
    ctx.request_repaint();
}

fn build_tray(core: &Arc<Core>, ctx: &egui::Context) -> Option<TrayIcon> {
    let show = MenuItem::with_id("show", "Show NextLine", true, None);
    let type_now = MenuItem::with_id("type", "Type current step", true, None);
    let stop = MenuItem::with_id("stop", "Stop typing", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show, &type_now, &stop, &PredefinedMenuItem::separator(), &quit]).ok()?;

    let img = load_icon_image();
    let icon = tray_icon::Icon::from_rgba(img.clone().into_raw(), img.width(), img.height()).ok()?;

    {
        let ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event
            {
                show_main_window(&ctx);
            }
        }));
    }
    {
        let ctx = ctx.clone();
        let core = Arc::clone(core);
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
            "show" => show_main_window(&ctx),
            "type" => core.type_current(Source::Hotkey),
            "stop" => core.stop(),
            "quit" => {
                QUITTING.store(true, Ordering::SeqCst);
                let _ = core.save_now();
                std::process::exit(0);
            }
            _ => {}
        }));
    }

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("NextLine")
        .with_icon(icon)
        .build()
        .ok()
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, core: Arc<Core>) -> App {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply_style(&cc.egui_ctx);
        core.attach_ctx(cc.egui_ctx.clone());
        core.mark_dirty();

        let img = load_icon_image();
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let logo = cc.egui_ctx.load_texture("logo", color, egui::TextureOptions::LINEAR);

        let (committed, committed_settings, committed_selection) = {
            let data = core.data();
            (data.scripts.clone(), data.settings.clone(), (data.selected_script.clone(), data.selected_step.clone()))
        };
        let tray = build_tray(&core, &cc.egui_ctx);
        let committed_selection_step = committed_selection.1.clone();

        App {
            core,
            query: String::new(),
            category: None,
            compact: false,
            normal_size: None,
            history: History::default(),
            committed,
            committed_settings,
            committed_selection,
            recording: None,
            hotkey_message: None,
            settings_open: false,
            hwnd: None,
            applied_theme: None,
            applied_capture: None,
            logo,
            last_step_seen: committed_selection_step,
            scroll_to_step: false,
            editing: None,
            focus_editor: false,
            update_dismissed: None,
            focus_search: false,
            _tray: tray,
        }
    }

    // ---------------------------------------------------------------- history

    fn observe_history(&mut self, data: &AppData) {
        if data.scripts != self.committed {
            let recent = self.history.last_push.is_some_and(|t| t.elapsed() < Duration::from_millis(700));
            if self.history.force_break || !recent {
                self.history.undo.push(self.committed.clone());
                if self.history.undo.len() > 200 {
                    self.history.undo.remove(0);
                }
                self.history.last_push = Some(Instant::now());
            }
            self.history.force_break = false;
            self.history.redo.clear();
            self.committed = data.scripts.clone();
        }
        let selection = (data.selected_script.clone(), data.selected_step.clone());
        if selection != self.committed_selection || data.settings != self.committed_settings || data.scripts != self.committed {
            self.committed_selection = selection;
            self.committed_settings = data.settings.clone();
            self.core.mark_dirty();
        }
    }

    fn mark_structural(&mut self) {
        self.history.force_break = true;
    }

    fn undo(&mut self, data: &mut AppData) {
        if let Some(previous) = self.history.undo.pop() {
            self.history.redo.push(std::mem::replace(&mut data.scripts, previous));
            self.committed = data.scripts.clone();
            data.sanitize();
            self.core.mark_dirty();
        }
    }

    fn redo(&mut self, data: &mut AppData) {
        if let Some(next) = self.history.redo.pop() {
            self.history.undo.push(std::mem::replace(&mut data.scripts, next));
            self.committed = data.scripts.clone();
            data.sanitize();
            self.core.mark_dirty();
        }
    }

    // ---------------------------------------------------------------- shortcuts & hotkey recorder

    fn handle_shortcuts(&mut self, ctx: &egui::Context, data: &mut AppData, acts: &mut Vec<Act>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, find, settings, presenter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, Key::Y)
                    || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, Key::F),
                i.consume_key(egui::Modifiers::COMMAND, Key::Comma),
                i.consume_key(egui::Modifiers::COMMAND, Key::M),
            )
        });
        if undo {
            self.undo(data);
        }
        if redo {
            self.redo(data);
        }
        if find {
            self.focus_search = true;
        }
        if settings {
            self.settings_open = !self.settings_open;
        }
        if presenter {
            acts.push(Act::ToggleCompact);
        }
    }

    fn capture_hotkey(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let Some(action) = self.recording else { return };
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let Event::Key { key, pressed: true, repeat: false, modifiers, .. } = event {
                if key == Key::Escape {
                    acts.push(Act::StopRecording);
                } else {
                    match Combo::from_parts(modifiers.ctrl, modifiers.alt, modifiers.shift, false, key.name()) {
                        Ok(combo) => acts.push(Act::SetHotkey(action, combo)),
                        Err(error) => self.hotkey_message = Some(error),
                    }
                }
                break;
            }
        }
    }

    // ---------------------------------------------------------------- actions needing the data lock released

    fn run_acts(&mut self, ctx: &egui::Context, acts: Vec<Act>) {
        for act in acts {
            match act {
                Act::Type(source) => self.core.type_current(source),
                Act::Stop => self.core.stop(),
                Act::Import => self.import(),
                Act::ExportAll => {
                    let scripts = self.core.data().scripts.clone();
                    self.export("nextline-library.json", &scripts);
                }
                Act::ExportScript(id) => {
                    let script = self.core.data().scripts.iter().find(|s| s.id == id).cloned();
                    if let Some(script) = script {
                        let name = format!("{}.json", script.title.trim().replace(|c: char| !c.is_alphanumeric(), "-").to_lowercase());
                        self.export(&name, std::slice::from_ref(&script));
                    }
                }
                Act::ToggleCompact => self.toggle_compact(ctx),
                Act::StartRecording(action) => {
                    self.core.suspend_hotkeys();
                    self.recording = Some(action);
                    self.hotkey_message = None;
                }
                Act::StopRecording => {
                    self.recording = None;
                    self.core.apply_hotkeys();
                }
                Act::SetHotkey(action, combo) => match self.core.set_hotkey(action, combo) {
                    Ok(()) => {
                        self.recording = None;
                        self.hotkey_message = None;
                    }
                    Err(error) => {
                        self.hotkey_message = Some(error);
                        self.core.suspend_hotkeys();
                    }
                },
                Act::ResetHotkeys => {
                    self.core.data().settings.hotkeys = model::Hotkeys::default();
                    self.core.apply_hotkeys();
                    self.core.mark_dirty();
                    self.recording = None;
                    self.hotkey_message = None;
                }
            }
        }
    }

    fn import(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("NextLine scripts", &["json"]).pick_file() else { return };
        let result = match std::fs::metadata(&path) {
            Ok(meta) if meta.len() > 5_000_000 => Err("Choose a JSON file smaller than 5 MB.".to_string()),
            Ok(_) => std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| model::parse_import(&text)),
            Err(error) => Err(error.to_string()),
        };
        match result {
            Ok(mut scripts) => {
                let count = scripts.len();
                let first = scripts[0].id.clone();
                self.mark_structural();
                let mut data = self.core.data();
                scripts.append(&mut data.scripts);
                data.scripts = scripts;
                data.select_script(&first);
                drop(data);
                self.category = None;
                self.core.push_notice(format!("Imported {count} script{}.", if count == 1 { "" } else { "s" }), false);
            }
            Err(error) => self.core.push_notice(format!("Import failed. {error}"), true),
        }
    }

    fn export(&mut self, file_name: &str, scripts: &[Script]) {
        let Some(path) = rfd::FileDialog::new().set_file_name(file_name).add_filter("JSON", &["json"]).save_file() else {
            return;
        };
        match std::fs::write(&path, model::export_json(scripts)) {
            Ok(()) => self.core.push_notice(format!("Exported {} script{}.", scripts.len(), if scripts.len() == 1 { "" } else { "s" }), false),
            Err(error) => self.core.push_notice(format!("Export failed: {error}"), true),
        }
    }

    fn toggle_compact(&mut self, ctx: &egui::Context) {
        use egui::{ViewportCommand as Cmd, WindowLevel};
        if self.compact {
            ctx.send_viewport_cmd(Cmd::MinInnerSize(vec2(NORMAL_MIN_SIZE[0], NORMAL_MIN_SIZE[1])));
            ctx.send_viewport_cmd(Cmd::InnerSize(self.normal_size.unwrap_or(vec2(1040.0, 680.0))));
            ctx.send_viewport_cmd(Cmd::WindowLevel(WindowLevel::Normal));
        } else {
            self.normal_size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
            ctx.send_viewport_cmd(Cmd::MinInnerSize(vec2(380.0, 260.0)));
            ctx.send_viewport_cmd(Cmd::InnerSize(vec2(COMPACT_SIZE[0], COMPACT_SIZE[1])));
            ctx.send_viewport_cmd(Cmd::WindowLevel(WindowLevel::AlwaysOnTop));
        }
        self.compact = !self.compact;
    }

    // ---------------------------------------------------------------- shared widgets

    fn status_chip(&self, ui: &mut egui::Ui, p: &Palette, data: &AppData) {
        let (text, color) = match self.core.status() {
            Status::Typing { done, total } => (format!("{}  Typing… {}%", icons::KEYBOARD, done * 100 / total.max(1)), p.accent),
            Status::Countdown { ends } => {
                let secs = ends.saturating_duration_since(Instant::now()).as_secs() + 1;
                (format!("{}  Switch windows - typing in {secs}s", icons::TIMER), p.accent)
            }
            Status::Idle => match self.core.hotkey_error(HotkeyAction::Type) {
                Some(_) => (format!("{}  Hotkey unavailable - see Settings", icons::WARNING_CIRCLE), p.danger),
                None => (format!("{}  Ready - {} types the next step", icons::CHECK_CIRCLE, data.settings.hotkeys.type_step), p.ok),
            },
        };
        ui.label(RichText::new(text).color(color).strong());
    }

    fn speed_row(&self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData) {
        ui.horizontal(|ui| {
            ui.label(small_caps(p, "Speed"));
            for (label, cps) in SPEED_PRESETS {
                let selected = data.settings.speed_cps == cps;
                let text = if selected { RichText::new(label).color(p.on_accent).strong() } else { RichText::new(label) };
                let mut button = egui::Button::new(text).min_size(vec2(84.0, 30.0)).corner_radius(CornerRadius::same(15));
                if selected {
                    button = button.fill(p.accent);
                }
                if ui.add(button).clicked() {
                    data.settings.speed_cps = cps;
                }
            }
        });
    }

    /// Restart / Back / Type / Skip row. Clicking Type always uses the countdown so the user can switch windows first.
    /// It is a secondary action: the hotkey is the primary way to present, unless the hotkey is unavailable.
    fn type_controls(&self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData, acts: &mut Vec<Act>, height: f32, hint: bool) {
        let busy = self.core.is_busy();
        let hotkey_broken = self.core.hotkey_error(HotkeyAction::Type).is_some();
        ui.horizontal(|ui| {
            let restart = egui::Button::new(RichText::new(icons::ARROW_COUNTER_CLOCKWISE).size(16.0)).min_size(vec2(height, height));
            if ui.add_enabled(!busy && data.step_index() > 0, restart).on_hover_text("Restart from step 1").clicked() {
                data.select_step_index(0);
            }
            let back = egui::Button::new(RichText::new(icons::SKIP_BACK).size(16.0)).min_size(vec2(height, height));
            if ui.add_enabled(!busy, back).on_hover_text(format!("Previous step ({})", data.settings.hotkeys.back)).clicked() {
                data.go_back();
            }
            if busy {
                if danger_button(ui, p, format!("{}  Stop", icons::STOP), vec2(150.0, height))
                    .on_hover_text(format!("Stop typing ({})", data.settings.hotkeys.stop))
                    .clicked()
                {
                    acts.push(Act::Stop);
                }
            } else {
                let label = format!("{}  Type in {}s", icons::PLAY, data.settings.countdown_secs);
                let tip = format!(
                    "Starts after a countdown so you can click into the target window.\nWhile presenting, press {} instead.",
                    data.settings.hotkeys.type_step
                );
                let clicked = if hotkey_broken {
                    primary_button(ui, p, label, vec2(150.0, height)).on_hover_text(tip).clicked()
                } else {
                    ui.add(egui::Button::new(label).min_size(vec2(150.0, height))).on_hover_text(tip).clicked()
                };
                if clicked {
                    acts.push(Act::Type(Source::Manual));
                }
            }
            let skip = egui::Button::new(RichText::new(icons::SKIP_FORWARD).size(16.0)).min_size(vec2(height, height));
            if ui.add_enabled(!busy, skip).on_hover_text(format!("Skip to next step ({})", data.settings.hotkeys.skip)).clicked() {
                data.advance();
            }
            if hint {
                ui.add_space(6.0);
                if hotkey_broken {
                    ui.label(RichText::new(format!("{}  Hotkey unavailable", icons::WARNING_CIRCLE)).color(p.danger));
                } else {
                    ui.label(RichText::new("or press").color(p.weak));
                    keycap(ui, p, &data.settings.hotkeys.type_step);
                    ui.label(RichText::new("in your app").color(p.weak));
                }
            }
        });
    }

    /// Toggle for excluding the window from screen sharing; important enough to live outside Settings.
    fn privacy_toggle(&self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData) {
        let on = data.settings.hide_from_capture;
        let (icon, tip) = if on {
            (icons::EYE_SLASH, "Hidden from screen sharing and recordings. Click to show it again.")
        } else {
            (icons::EYE, "Visible to screen sharing. Click to hide this window from meeting apps and recordings.")
        };
        let text = RichText::new(icon).size(16.0).color(if on { p.accent } else { ui.visuals().text_color() });
        if ui.add(egui::Button::new(text).frame(false).min_size(vec2(28.0, 28.0))).on_hover_text(tip).clicked() {
            data.settings.hide_from_capture = !on;
        }
    }

    /// Latest feedback message, with an Undo button after destructive edits.
    fn notice_line(&mut self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData) {
        let Some(notice) = self.core.notice().filter(|n| n.at.elapsed() < Duration::from_secs(10)) else { return };
        let color = if notice.is_error { p.danger } else { ui.visuals().text_color() };
        ui.label(RichText::new(&notice.text).color(color));
        if notice.undoable && !self.history.undo.is_empty() && ui.link("Undo").clicked() {
            self.undo(data);
            self.core.clear_notice();
        }
    }

    fn progress_bar(&self, ui: &mut egui::Ui, p: &Palette, countdown_secs: u32) {
        if let Status::Typing { done, total } = self.core.status() {
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).fill(p.accent).desired_height(6.0));
        } else if let Status::Countdown { ends } = self.core.status() {
            let left = ends.saturating_duration_since(Instant::now()).as_secs_f32();
            ui.add(egui::ProgressBar::new(1.0 - left / (countdown_secs as f32).max(0.1)).fill(p.accent).desired_height(6.0));
        }
    }

    // ---------------------------------------------------------------- full window

    /// Slim "update available" strip under the title bar. Never shown while typing or in presenter mode.
    fn update_banner(&mut self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData) {
        let Some(info) = self.core.available_update(&data.settings.skipped_version) else { return };
        if self.core.is_busy() || self.update_dismissed.as_deref() == Some(info.version.as_str()) {
            return;
        }
        Panel::top("update_banner")
            .frame(Frame::new().fill(p.accent_soft).inner_margin(Margin::symmetric(16, 7)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let text = RichText::new(format!("{}  NextLine {} is available", icons::ARROW_CIRCLE_UP, info.version))
                        .color(p.accent)
                        .strong();
                    let label = ui.label(text);
                    if !info.summary.is_empty() {
                        label.on_hover_text(&info.summary);
                    }
                    if update::is_trusted_release_url(&info.url) && ui.button("View release").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(info.url.clone()));
                    }
                    if ui.link("Skip this version").clicked() {
                        data.settings.skipped_version = info.version.clone();
                        self.core.mark_dirty();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, icons::X, "Hide until next launch").clicked() {
                            self.update_dismissed = Some(info.version.clone());
                        }
                    });
                });
            });
    }

    fn main_ui(&mut self, ui: &mut egui::Ui, data: &mut AppData, acts: &mut Vec<Act>) {
        let p = palette(ui.visuals().dark_mode);
        let panel_fill = ui.visuals().panel_fill;

        Panel::top("top")
            .frame(Frame::new().fill(panel_fill).inner_margin(Margin::symmetric(16, 10)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.image((self.logo.id(), vec2(28.0, 28.0)));
                    ui.label(RichText::new("NextLine").size(19.0).strong());
                    ui.add_space(14.0);
                    self.status_chip(ui, &p, data);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, icons::GEAR, "Settings (Ctrl+,)").clicked() {
                            self.settings_open = !self.settings_open;
                        }
                        if icon_button(ui, icons::PICTURE_IN_PICTURE, "Presenter mode: compact, always on top (Ctrl+M)").clicked() {
                            acts.push(Act::ToggleCompact);
                        }
                        self.privacy_toggle(ui, &p, data);
                        ui.add_space(6.0);
                        let can_redo = !self.history.redo.is_empty();
                        let can_undo = !self.history.undo.is_empty();
                        if ui
                            .add_enabled(can_redo, egui::Button::new(RichText::new(icons::ARROW_U_UP_RIGHT).size(16.0)).frame(false))
                            .on_hover_text("Redo (Ctrl+Y)")
                            .clicked()
                        {
                            self.redo(data);
                        }
                        if ui
                            .add_enabled(can_undo, egui::Button::new(RichText::new(icons::ARROW_U_UP_LEFT).size(16.0)).frame(false))
                            .on_hover_text("Undo (Ctrl+Z)")
                            .clicked()
                        {
                            self.undo(data);
                        }
                    });
                });
            });

        self.update_banner(ui, &p, data);

        Panel::bottom("bottom")
            .frame(Frame::new().fill(panel_fill).inner_margin(Margin::symmetric(16, 7)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.notice_line(ui, &p, data);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let h = data.settings.hotkeys.clone();
                        // Right-to-left layout: add in reverse so they read left to right.
                        for (key, label) in [(&h.stop, "stop"), (&h.back, "back"), (&h.skip, "skip"), (&h.type_step, "type")] {
                            ui.label(RichText::new(label).small().color(p.weak));
                            keycap(ui, &p, key);
                            ui.add_space(6.0);
                        }
                    });
                });
            });

        Panel::left("scripts")
            .resizable(true)
            .default_size(290.0)
            .size_range(220.0..=400.0)
            .frame(Frame::new().fill(panel_fill).inner_margin(Margin::symmetric(14, 8)))
            .show(ui, |ui| self.sidebar(ui, &p, data, acts));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(panel_fill).inner_margin(Margin::symmetric(18, 8)))
            .show(ui, |ui| self.workspace(ui, &p, data, acts));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData, acts: &mut Vec<Act>) {
        let search = TextEdit::singleline(&mut self.query)
            .hint_text(format!("{}  Search scripts and steps", icons::MAGNIFYING_GLASS))
            .desired_width(f32::INFINITY)
            .id(egui::Id::new("search"));
        let response = ui.add(search);
        if std::mem::take(&mut self.focus_search) {
            response.request_focus();
        }

        let categories = data.categories();
        if categories.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.spacing_mut().button_padding = vec2(9.0, 4.0);
                if ui.selectable_label(self.category.is_none(), "All").clicked() {
                    self.category = None;
                }
                for category in &categories {
                    if ui.selectable_label(self.category.as_deref() == Some(category), category).clicked() {
                        self.category = Some(category.clone());
                    }
                }
            });
        }
        ui.add_space(4.0);

        let visible: Vec<(String, String, String, usize)> = data
            .scripts
            .iter()
            .filter(|s| self.category.as_ref().is_none_or(|c| s.category.trim() == c) && s.matches(&self.query))
            .map(|s| (s.id.clone(), s.title.clone(), s.category.clone(), s.steps.len()))
            .collect();

        let footer_height = 100.0;
        let list_height = (ui.available_height() - footer_height).max(60.0);
        egui::ScrollArea::vertical().max_height(list_height).auto_shrink(false).show(ui, |ui| {
            if visible.is_empty() {
                ui.add_space(12.0);
                ui.label(RichText::new("No scripts match your search.").color(p.weak));
            }
            for (id, title, category, steps) in &visible {
                let selected = *id == data.selected_script;
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
                let fill = if selected {
                    p.accent_soft
                } else if response.hovered() {
                    ui.visuals().widgets.hovered.weak_bg_fill
                } else {
                    Color32::TRANSPARENT
                };
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, CornerRadius::same(8), fill);
                if selected {
                    painter.rect_filled(
                        egui::Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(3.0, rect.height() - 20.0)),
                        CornerRadius::same(2),
                        p.accent,
                    );
                }
                let title_text = if title.trim().is_empty() { "Untitled script" } else { title.as_str() };
                let text_rect = rect.shrink2(vec2(14.0, 0.0));
                let clipped = ui.painter_at(text_rect);
                clipped.text(
                    text_rect.left_top() + vec2(0.0, 9.0),
                    egui::Align2::LEFT_TOP,
                    title_text,
                    egui::FontId::proportional(15.0),
                    ui.visuals().strong_text_color(),
                );
                let sub = format!(
                    "{}{} step{}",
                    if category.trim().is_empty() { String::new() } else { format!("{}  ·  ", category.trim()) },
                    steps,
                    if *steps == 1 { "" } else { "s" }
                );
                clipped.text(
                    text_rect.left_top() + vec2(0.0, 30.0),
                    egui::Align2::LEFT_TOP,
                    sub,
                    egui::FontId::proportional(12.5),
                    p.weak,
                );
                if response.clicked() && !selected {
                    data.select_script(id);
                    self.editing = None;
                    self.scroll_to_step = true;
                }
            }
        });

        ui.separator();
        ui.add_space(4.0);
        let full = ui.available_width();
        let primary = egui::Button::new(RichText::new(format!("{}  New script", icons::PLUS)).color(p.on_accent).strong())
            .fill(p.accent)
            .min_size(vec2(full, 34.0));
        if ui.add(primary).clicked() {
            self.mark_structural();
            let script = Script { title: "New script".into(), category: "General".into(), ..Script::default() };
            let at = data.script_index() + 1;
            let id = script.id.clone();
            data.scripts.insert(at, script);
            data.select_script(&id);
            self.editing = None;
            self.query.clear();
            self.category = None;
        }
        let half = (full - ui.spacing().item_spacing.x) / 2.0;
        ui.horizontal(|ui| {
            let import = egui::Button::new(format!("{}  Import", icons::DOWNLOAD_SIMPLE)).min_size(vec2(half, 30.0));
            if ui.add(import).on_hover_text("Import scripts from a JSON file").clicked() {
                acts.push(Act::Import);
            }
            let export = egui::Button::new(format!("{}  Export all", icons::UPLOAD_SIMPLE)).min_size(vec2(half, 30.0));
            if ui.add(export).on_hover_text("Export the whole library to a JSON file").clicked() {
                acts.push(Act::ExportAll);
            }
        });
    }

    fn workspace(&mut self, ui: &mut egui::Ui, p: &Palette, data: &mut AppData, acts: &mut Vec<Act>) {
        let mut script_op = None;
        let mut step_op: Option<StepOp> = None;
        let selected_id = data.selected_step.clone();
        if self.last_step_seen != selected_id {
            self.last_step_seen = selected_id.clone();
            self.scroll_to_step = true;
        }
        let step_index = data.step_index();
        let script_id = data.selected_script.clone();
        let current = data.selected_step().clone();
        let step_total = data.selected_script().steps.len();

        // Header and run card stay pinned; only the step list scrolls, so the controls never scroll away.
        {
            ui.add_space(4.0);

            // Script header
            {
                let idx = data.script_index();
                let script = &mut data.scripts[idx];
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, icons::TRASH, "Delete this script").clicked() {
                            script_op = Some(ScriptOp::Delete);
                        }
                        if icon_button(ui, icons::UPLOAD_SIMPLE, "Export this script").clicked() {
                            acts.push(Act::ExportScript(script_id.clone()));
                        }
                        if icon_button(ui, icons::COPY, "Duplicate this script").clicked() {
                            script_op = Some(ScriptOp::Duplicate);
                        }
                        ui.add(
                            TextEdit::singleline(&mut script.category)
                                .hint_text("Category")
                                .desired_width(130.0)
                                .text_color(p.weak),
                        )
                        .on_hover_text("Category - used for the filter chips");
                        ui.label(RichText::new(icons::TAG).color(p.weak));
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(
                                TextEdit::singleline(&mut script.title)
                                    .font(egui::FontId::proportional(24.0))
                                    .hint_text("Script title")
                                    .frame(Frame::NONE)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    });
                });
            }
            ui.add_space(6.0);

            // Run card
            card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(small_caps(p, &format!("Up next  ·  step {} of {}", step_index + 1, step_total)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        step_segments(ui, p, step_index, step_total, 150.0);
                    });
                });
                if current.body.trim().is_empty() {
                    ui.label(
                        RichText::new(format!("This step is empty - use {} to add text.", icons::PENCIL_SIMPLE))
                            .color(p.weak)
                            .italics(),
                    );
                } else {
                    ui.add(egui::Label::new(RichText::new(one_line(&current.body, 320)).size(17.0)).wrap());
                }
                if !current.note.trim().is_empty() {
                    ui.label(RichText::new(format!("{}  {}", icons::NOTE_PENCIL, current.note.trim())).italics().color(p.weak));
                }
                ui.add_space(2.0);
                self.type_controls(ui, p, data, acts, 38.0, true);
                self.progress_bar(ui, p, data.settings.countdown_secs);
                self.speed_row(ui, p, data);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut data.settings.humanize, "Human rhythm")
                        .on_hover_text("Vary the pace and pause naturally at the end of sentences and clauses.");
                    let chars = current.body.chars().count();
                    if chars > 0 {
                        let secs =
                            if data.settings.speed_cps == 0 { 0.0 } else { chars as f32 / data.settings.speed_cps as f32 * 1.15 };
                        ui.label(RichText::new(format!("{chars} characters  ·  about {}", format_duration(secs))).small().color(p.weak));
                    }
                });
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Steps").size(17.0).strong());
                ui.label(RichText::new(format!("{step_total}")).color(p.weak));
                ui.label(RichText::new("Click a step to make it next").small().color(p.weak));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(format!("{}  Add step", icons::PLUS)).clicked() {
                        step_op = Some(StepOp::Add);
                    }
                });
            });

            let idx = data.script_index();
            let steps = &mut data.scripts[idx].steps;
            let scroll = std::mem::take(&mut self.scroll_to_step);
            let editing = self.editing.clone();
            let focus_pending = std::mem::take(&mut self.focus_editor);
            egui::ScrollArea::vertical().id_salt(("steps", script_id.clone())).auto_shrink(false).show(ui, |ui| {
                for (i, step) in steps.iter_mut().enumerate() {
                    let state = match i.cmp(&step_index) {
                        std::cmp::Ordering::Less => StepState::Done,
                        std::cmp::Ordering::Equal => StepState::Next,
                        std::cmp::Ordering::Greater => StepState::Upcoming,
                    };
                    let is_editing = editing.as_deref() == Some(step.id.as_str());
                    ui.push_id(step.id.clone(), |ui| {
                        let frame = Frame::new()
                            .fill(if state == StepState::Next { p.accent_soft } else { p.card })
                            .stroke(if state == StepState::Next {
                                Stroke::new(1.5, p.accent)
                            } else {
                                Stroke::new(1.0, p.card_border)
                            })
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 9));
                        let response = frame.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            step_header(ui, p, i, step, state, is_editing, step_total, &mut step_op);
                            if is_editing {
                                step_editor(ui, p, step, focus_pending);
                            }
                        });
                        if (state == StepState::Next && scroll) || (is_editing && focus_pending) {
                            response.response.scroll_to_me(None);
                        }
                    });
                }
                ui.add_space(16.0);
            });
        }

        if let Some(op) = step_op {
            self.mark_structural();
            self.apply_step_op(data, op);
        }
        match script_op {
            Some(ScriptOp::Duplicate) => {
                self.mark_structural();
                let copy = data.selected_script().duplicate();
                let id = copy.id.clone();
                data.scripts.insert(data.script_index() + 1, copy);
                data.select_script(&id);
                self.editing = None;
                self.core.push_notice("Script duplicated.", false);
            }
            Some(ScriptOp::Delete) => {
                if data.scripts.len() > 1 {
                    self.mark_structural();
                    let at = data.script_index();
                    data.scripts.remove(at);
                    let next = data.scripts[at.min(data.scripts.len() - 1)].id.clone();
                    data.select_script(&next);
                    self.editing = None;
                    self.core.push_undoable_notice("Script deleted.");
                } else {
                    self.core.push_notice("Keep at least one script in the library.", true);
                }
            }
            None => {}
        }
    }

    // ---------------------------------------------------------------- presenter (compact) mode

    fn compact_ui(&mut self, ui: &mut egui::Ui, data: &mut AppData, acts: &mut Vec<Act>) {
        let p = palette(ui.visuals().dark_mode);
        let panel_fill = ui.visuals().panel_fill;
        egui::CentralPanel::default().frame(Frame::new().fill(panel_fill).inner_margin(Margin::same(14))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(data.selected_script().title.clone()).strong().size(16.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, icons::ARROWS_OUT_SIMPLE, "Back to the full window (Ctrl+M)").clicked() {
                        acts.push(Act::ToggleCompact);
                    }
                    self.privacy_toggle(ui, &p, data);
                });
            });
            let (index, total) = (data.step_index(), data.selected_script().steps.len());
            ui.horizontal(|ui| {
                ui.label(small_caps(&p, &format!("Step {} of {}", index + 1, total)));
                step_segments(ui, &p, index, total, 90.0);
                ui.add_space(6.0);
                self.status_chip(ui, &p, data);
            });
            let step = data.selected_step().clone();
            egui::ScrollArea::vertical().max_height((ui.available_height() - 128.0).max(40.0)).auto_shrink([false, false]).show(
                ui,
                |ui| {
                    if step.body.trim().is_empty() {
                        ui.label(RichText::new("This step is empty.").italics().color(p.weak));
                    } else {
                        ui.label(RichText::new(&step.body).size(16.0));
                    }
                    if !step.note.trim().is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("{}  {}", icons::NOTE_PENCIL, step.note.trim())).italics().color(p.weak));
                    }
                },
            );
            self.progress_bar(ui, &p, data.settings.countdown_secs);
            self.type_controls(ui, &p, data, acts, 36.0, false);
            ui.horizontal(|ui| self.notice_line(ui, &p, data));
        });
    }

    // ---------------------------------------------------------------- settings

    fn settings_window(&mut self, ctx: &egui::Context, data: &mut AppData, acts: &mut Vec<Act>) {
        if !self.settings_open {
            return;
        }
        let p = palette(ctx.global_style().visuals.dark_mode);
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .vscroll(true)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(430.0);
                ui.label(small_caps(&p, "Hotkeys (work while NextLine is in the tray)"));
                egui::Grid::new("hotkeys").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    for action in HotkeyAction::ALL {
                        ui.label(action.label());
                        let current = match action {
                            HotkeyAction::Type => &data.settings.hotkeys.type_step,
                            HotkeyAction::Skip => &data.settings.hotkeys.skip,
                            HotkeyAction::Back => &data.settings.hotkeys.back,
                            HotkeyAction::Stop => &data.settings.hotkeys.stop,
                        };
                        ui.horizontal(|ui| {
                            let recording = self.recording == Some(action);
                            let label = if recording { "Press keys…  (Esc cancels)".to_string() } else { current.clone() };
                            let mut button = egui::Button::new(RichText::new(label).monospace()).min_size(vec2(190.0, 28.0));
                            if recording {
                                button = button.stroke(Stroke::new(1.5, p.accent));
                            }
                            if ui.add(button).on_hover_text("Click, then press the new key combination").clicked() {
                                acts.push(if recording { Act::StopRecording } else { Act::StartRecording(action) });
                            }
                            if let Some(error) = self.core.hotkey_error(action) {
                                ui.label(RichText::new(icons::WARNING_CIRCLE).color(p.danger)).on_hover_text(error);
                            }
                        });
                        ui.end_row();
                    }
                });
                if let Some(message) = &self.hotkey_message {
                    ui.label(RichText::new(message).color(p.danger));
                }
                if ui.small_button("Reset hotkeys to defaults").clicked() {
                    acts.push(Act::ResetHotkeys);
                }

                ui.separator();
                ui.label(small_caps(&p, "Typing"));
                ui.horizontal(|ui| {
                    ui.label("New lines in a step");
                    egui::ComboBox::from_id_salt("newline")
                        .selected_text(match data.settings.newline_mode {
                            NewlineMode::ShiftEnter => "Shift + Enter (safe for chat boxes)",
                            NewlineMode::Enter => "Enter",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut data.settings.newline_mode, NewlineMode::ShiftEnter, "Shift + Enter (safe for chat boxes)");
                            ui.selectable_value(&mut data.settings.newline_mode, NewlineMode::Enter, "Enter");
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Countdown for the on-screen button");
                    ui.add(egui::Slider::new(&mut data.settings.countdown_secs, 1..=10).suffix(" s"));
                });
                ui.checkbox(&mut data.settings.wrap_at_end, "Return to step 1 after the last step");

                ui.separator();
                ui.label(small_caps(&p, "Window"));
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    for (choice, label) in [(ThemeChoice::System, "System"), (ThemeChoice::Light, "Light"), (ThemeChoice::Dark, "Dark")] {
                        ui.selectable_value(&mut data.settings.theme, choice, label);
                    }
                });
                ui.checkbox(&mut data.settings.close_to_tray, "Closing the window keeps running in the tray");
                ui.checkbox(&mut data.settings.hide_from_capture, "Hide this window from screen sharing and recordings")
                    .on_hover_text("Uses Windows display affinity (Windows 10 2004+). You still see the window; meeting apps and recorders do not.");

                ui.separator();
                ui.label(small_caps(&p, "Updates"));
                ui.checkbox(&mut data.settings.check_updates, "Check GitHub for a new version about once a day")
                    .on_hover_text("The only network request NextLine makes. It sends nothing about you or your scripts.");
                ui.horizontal(|ui| {
                    let checking = self.core.is_checking_for_updates();
                    if ui.add_enabled(!checking, egui::Button::new("Check now")).clicked() {
                        self.core.request_update_check();
                    }
                    let last = data.settings.last_update_check;
                    let when = if last == 0 { "never".to_string() } else { format!("{} ago", ago(update::now_unix().saturating_sub(last))) };
                    ui.label(RichText::new(format!("Version {}  ·  last checked {when}", update::CURRENT_VERSION)).small().color(p.weak));
                });
                if self.core.is_checking_for_updates() {
                    ui.label(RichText::new("Checking…").color(p.weak));
                } else if let Some((text, is_error)) = self.core.update_message() {
                    ui.label(RichText::new(text).color(if is_error { p.danger } else { p.weak }));
                }
            });
        self.settings_open = open;
        if !open && self.recording.is_some() {
            acts.push(Act::StopRecording);
        }
    }
}

fn ago(secs: u64) -> String {
    match secs {
        0..=89 => "moments".into(),
        90..=5_399 => format!("{} min", (secs + 30) / 60),
        5_400..=129_599 => format!("{} h", (secs + 1_800) / 3_600),
        _ => format!("{} days", secs / 86_400),
    }
}

fn format_duration(secs: f32) -> String {
    if secs < 1.0 { "under a second".into() } else { format!("{secs:.0} s") }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepState {
    Done,
    Next,
    Upcoming,
}

#[allow(clippy::too_many_arguments)]
fn step_header(
    ui: &mut egui::Ui,
    p: &Palette,
    index: usize,
    step: &Step,
    state: StepState,
    is_editing: bool,
    total: usize,
    op: &mut Option<StepOp>,
) {
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
        let response = response.on_hover_text("Make this the next step");
        let (fill, text_color) = match state {
            StepState::Next => (p.accent, p.on_accent),
            StepState::Done => (p.ok.gamma_multiply(0.18), p.ok),
            StepState::Upcoming => (p.card_border, p.weak),
        };
        ui.painter().circle_filled(rect.center(), 13.0, fill);
        let glyph = if state == StepState::Done { icons::CHECK.to_string() } else { (index + 1).to_string() };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(13.0), text_color);
        if response.clicked() {
            *op = Some(StepOp::Select(index));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, icons::TRASH, "Delete step").clicked() {
                *op = Some(StepOp::Delete(index));
            }
            if icon_button(ui, icons::COPY, "Duplicate step").clicked() {
                *op = Some(StepOp::Duplicate(index));
            }
            let down = egui::Button::new(RichText::new(icons::ARROW_DOWN).size(16.0)).frame(false);
            if ui.add_enabled(index + 1 < total, down).on_hover_text("Move down").clicked() {
                *op = Some(StepOp::Down(index));
            }
            let up = egui::Button::new(RichText::new(icons::ARROW_UP).size(16.0)).frame(false);
            if ui.add_enabled(index > 0, up).on_hover_text("Move up").clicked() {
                *op = Some(StepOp::Up(index));
            }
            let pencil_color = if is_editing { p.accent } else { ui.visuals().text_color() };
            let pencil = egui::Button::new(RichText::new(icons::PENCIL_SIMPLE).size(16.0).color(pencil_color)).frame(false);
            if ui.add(pencil.min_size(vec2(28.0, 28.0))).on_hover_text(if is_editing { "Close editor" } else { "Edit step" }).clicked() {
                *op = Some(StepOp::Edit(index));
            }
            if step.after_key != AfterKey::None {
                chip(ui, p, format!(" then {} ", step.after_key.label()));
            }
            if step.delay_ms > 0 {
                chip(ui, p, format!(" {} {} ms ", icons::TIMER, step.delay_ms));
            }
            if !step.note.trim().is_empty() {
                chip(ui, p, format!(" {} ", icons::NOTE_PENCIL)).on_hover_text(step.note.trim());
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if state == StepState::Next {
                    ui.label(RichText::new("NEXT").size(11.0).strong().color(p.accent));
                }
                let empty = step.body.trim().is_empty();
                let preview = if empty { "(empty step)".to_string() } else { one_line(&step.body, 160) };
                let text = match (empty, state) {
                    (true, _) => RichText::new(preview).italics().color(p.weak),
                    (_, StepState::Done) => RichText::new(preview).color(p.weak),
                    (_, StepState::Next) => RichText::new(preview).strong(),
                    _ => RichText::new(preview),
                };
                let label = ui.add(egui::Label::new(text).truncate().sense(Sense::click()));
                if label.clicked() {
                    *op = Some(StepOp::Select(index));
                }
                if label.double_clicked() {
                    *op = Some(StepOp::Edit(index));
                }
            });
        });
    });
}

fn step_editor(ui: &mut egui::Ui, p: &Palette, step: &mut Step, focus: bool) {
    ui.add_space(4.0);
    let body = ui.add(
        TextEdit::multiline(&mut step.body)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .hint_text("What should be typed for this step?"),
    );
    if focus {
        body.request_focus();
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Then press");
        egui::ComboBox::from_id_salt("after_key").selected_text(step.after_key.label()).show_ui(ui, |ui| {
            for key in AfterKey::ALL {
                ui.selectable_value(&mut step.after_key, key, key.label());
            }
        });
        ui.add_space(8.0);
        ui.label("Wait first");
        ui.add(egui::DragValue::new(&mut step.delay_ms).range(0..=60_000).speed(25).suffix(" ms"));
    });
    ui.add(
        TextEdit::multiline(&mut step.note)
            .desired_rows(1)
            .desired_width(f32::INFINITY)
            .hint_text(format!("{}  Presenter note - shown to you, never typed", icons::NOTE_PENCIL))
            .text_color(p.weak),
    );
}

impl App {
    fn apply_step_op(&mut self, data: &mut AppData, op: StepOp) {
        let idx = data.script_index();
        match op {
            StepOp::Select(i) => data.select_step_index(i),
            StepOp::Edit(i) => {
                let id = data.scripts[idx].steps[i].id.clone();
                if self.editing.as_deref() == Some(id.as_str()) {
                    self.editing = None;
                } else {
                    self.editing = Some(id);
                    self.focus_editor = true;
                }
            }
            StepOp::Up(i) if i > 0 => data.scripts[idx].steps.swap(i, i - 1),
            StepOp::Down(i) if i + 1 < data.scripts[idx].steps.len() => data.scripts[idx].steps.swap(i, i + 1),
            StepOp::Duplicate(i) => {
                let mut copy = data.scripts[idx].steps[i].clone();
                copy.id = model::new_id();
                self.editing = Some(copy.id.clone());
                self.focus_editor = true;
                data.scripts[idx].steps.insert(i + 1, copy);
            }
            StepOp::Delete(i) => {
                let steps = &mut data.scripts[idx].steps;
                if steps.len() <= 1 {
                    self.core.push_notice("Keep at least one step in the script.", true);
                } else {
                    let removed = steps.remove(i);
                    let keep = steps[i.min(steps.len() - 1)].id.clone();
                    if !steps.iter().any(|s| s.id == data.selected_step) {
                        data.selected_step = keep;
                    }
                    if self.editing.as_deref() == Some(removed.id.as_str()) {
                        self.editing = None;
                    }
                    self.core.push_undoable_notice("Step deleted.");
                }
            }
            StepOp::Add => {
                let step = Step::default();
                self.editing = Some(step.id.clone());
                self.focus_editor = true;
                data.scripts[idx].steps.push(step);
            }
            StepOp::Up(_) | StepOp::Down(_) => {}
        }
    }
}

impl eframe::App for App {
    /// Runs even while the window is hidden in the tray, so window management lives here.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.hwnd.is_none() {
            self.hwnd = platform::find_main_window(true);
        }

        let (theme, capture, close_to_tray) = {
            let data = self.core.data();
            (data.settings.theme, data.settings.hide_from_capture, data.settings.close_to_tray)
        };
        if self.applied_theme != Some(theme) {
            self.applied_theme = Some(theme);
            ctx.set_theme(match theme {
                ThemeChoice::System => egui::ThemePreference::System,
                ThemeChoice::Light => egui::ThemePreference::Light,
                ThemeChoice::Dark => egui::ThemePreference::Dark,
            });
        }
        if let Some(hwnd) = self.hwnd
            && self.applied_capture != Some(capture)
        {
            self.applied_capture = Some(capture);
            platform::set_capture_excluded(hwnd, capture);
        }

        if ctx.input(|i| i.viewport().close_requested()) && close_to_tray && !QUITTING.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if let Some(hwnd) = self.hwnd {
                platform::hide_window(hwnd);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let core = Arc::clone(&self.core);
        let mut acts = Vec::new();

        {
            let mut data = core.data();
            self.handle_shortcuts(&ctx, &mut data, &mut acts);
            self.capture_hotkey(&ctx, &mut acts);
            if self.compact {
                self.compact_ui(ui, &mut data, &mut acts);
            } else {
                self.main_ui(ui, &mut data, &mut acts);
            }
            self.settings_window(&ctx, &mut data, &mut acts);
            data.sanitize();
            self.observe_history(&data);
        }
        self.run_acts(&ctx, acts);

        let animating = !matches!(core.status(), Status::Idle);
        if animating {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if core.notice().is_some_and(|n| n.at.elapsed() < Duration::from_secs(10)) {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.core.save_now();
    }
}
