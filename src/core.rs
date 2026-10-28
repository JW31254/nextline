//! Shared application core. The UI is only a view over this; hotkeys and typing
//! keep working while the window is hidden in the tray.

use crate::hotkey::{Combo, HotkeyAction, HotkeyService};
use crate::model::{AppData, Hotkeys};
use crate::storage;
use crate::typer::{self, PlanConfig, TypeError};
use crate::update::{self, CheckError, UpdateInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Countdown { ends: Instant },
    Typing { done: usize, total: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Hotkey,
    Manual,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub text: String,
    pub is_error: bool,
    pub undoable: bool,
    pub at: Instant,
}

pub struct Core {
    data: Mutex<AppData>,
    status: Mutex<Status>,
    notice: Mutex<Option<Notice>>,
    hotkey_errors: Mutex<[Option<String>; 4]>,
    busy: AtomicBool,
    cancel: AtomicBool,
    dirty: AtomicBool,
    ctx: OnceLock<eframe::egui::Context>,
    hotkeys: OnceLock<HotkeyService>,
    update: Mutex<Option<UpdateInfo>>,
    update_message: Mutex<Option<(String, bool)>>,
    update_checking: AtomicBool,
    update_requested: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn combo_slot(hotkeys: &Hotkeys, action: HotkeyAction) -> &String {
    match action {
        HotkeyAction::Type => &hotkeys.type_step,
        HotkeyAction::Skip => &hotkeys.skip,
        HotkeyAction::Back => &hotkeys.back,
        HotkeyAction::Stop => &hotkeys.stop,
    }
}

fn combo_slot_mut(hotkeys: &mut Hotkeys, action: HotkeyAction) -> &mut String {
    match action {
        HotkeyAction::Type => &mut hotkeys.type_step,
        HotkeyAction::Skip => &mut hotkeys.skip,
        HotkeyAction::Back => &mut hotkeys.back,
        HotkeyAction::Stop => &mut hotkeys.stop,
    }
}

impl Core {
    pub fn new(data: AppData) -> Arc<Core> {
        let core = Arc::new(Core {
            data: Mutex::new(data),
            status: Mutex::new(Status::Idle),
            notice: Mutex::new(None),
            hotkey_errors: Mutex::new(Default::default()),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            ctx: OnceLock::new(),
            hotkeys: OnceLock::new(),
            update: Mutex::new(None),
            update_message: Mutex::new(None),
            update_checking: AtomicBool::new(false),
            update_requested: AtomicBool::new(false),
        });

        let weak = Arc::downgrade(&core);
        let service = HotkeyService::start(move |action| {
            if let Some(core) = weak.upgrade() {
                core.on_hotkey(action);
            }
        });
        let _ = core.hotkeys.set(service);
        core.apply_hotkeys();
        core.spawn_saver();
        core.spawn_updater();
        core
    }

    pub fn attach_ctx(&self, ctx: eframe::egui::Context) {
        let _ = self.ctx.set(ctx);
    }

    fn repaint(&self) {
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }

    pub fn data(&self) -> MutexGuard<'_, AppData> {
        lock(&self.data)
    }

    pub fn status(&self) -> Status {
        lock(&self.status).clone()
    }

    fn set_status(&self, status: Status) {
        *lock(&self.status) = status;
        self.repaint();
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn notice(&self) -> Option<Notice> {
        lock(&self.notice).clone()
    }

    pub fn push_notice(&self, text: impl Into<String>, is_error: bool) {
        *lock(&self.notice) = Some(Notice { text: text.into(), is_error, undoable: false, at: Instant::now() });
        self.repaint();
    }

    /// A notice that offers an Undo button, for destructive edits.
    pub fn push_undoable_notice(&self, text: impl Into<String>) {
        *lock(&self.notice) = Some(Notice { text: text.into(), is_error: false, undoable: true, at: Instant::now() });
        self.repaint();
    }

    pub fn clear_notice(&self) {
        *lock(&self.notice) = None;
    }

    pub fn hotkey_error(&self, action: HotkeyAction) -> Option<String> {
        lock(&self.hotkey_errors)[action as usize].clone()
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Debounced background autosave, so changes made while the window is hidden are kept too.
    fn spawn_saver(self: &Arc<Core>) {
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(400));
                    let Some(core) = weak.upgrade() else { return };
                    if core.dirty.swap(false, Ordering::SeqCst)
                        && let Err(error) = core.save_now()
                    {
                        core.push_notice(format!("Changes could not be saved: {error}. Export your library as a backup."), true);
                    }
                }
            })
            .expect("spawn autosave thread");
    }

    pub fn save_now(&self) -> std::io::Result<()> {
        let snapshot = self.data().clone();
        storage::save(&snapshot)
    }

    // ---------------------------------------------------------------- update check

    /// The newest release if it is newer than this build and not the version the user skipped.
    /// Takes the skipped version as an argument because the UI already holds the data lock; locking again would deadlock.
    pub fn available_update(&self, skipped_version: &str) -> Option<UpdateInfo> {
        update::unless_skipped(lock(&self.update).clone(), skipped_version)
    }

    /// Outcome of the latest check, for the settings screen: (text, is_error).
    pub fn update_message(&self) -> Option<(String, bool)> {
        lock(&self.update_message).clone()
    }

    pub fn is_checking_for_updates(&self) -> bool {
        self.update_checking.load(Ordering::SeqCst)
    }

    /// Runs a check right away (even if automatic checks are off).
    pub fn request_update_check(&self) {
        self.update_requested.store(true, Ordering::SeqCst);
    }

    /// Checks about a day apart, never while typing. Failures are silent and retried later.
    fn spawn_updater(self: &Arc<Core>) {
        const DAY: u64 = 24 * 60 * 60;
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("updates".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(8));
                let mut last_attempt: Option<Instant> = None;
                loop {
                    let Some(core) = weak.upgrade() else { return };
                    let manual = core.update_requested.swap(false, Ordering::SeqCst);
                    let due = {
                        let data = core.data();
                        data.settings.check_updates
                            && update::now_unix().saturating_sub(data.settings.last_update_check) >= DAY
                            && last_attempt.is_none_or(|t| t.elapsed() > Duration::from_secs(30 * 60))
                    };
                    if (manual || due) && !core.is_busy() {
                        last_attempt = Some(Instant::now());
                        core.run_update_check(manual);
                    } else if manual {
                        // Typing in progress: try again shortly.
                        core.update_requested.store(true, Ordering::SeqCst);
                    }
                    drop(core);
                    std::thread::sleep(Duration::from_secs(5));
                }
            })
            .expect("spawn update thread");
    }

    fn run_update_check(&self, manual: bool) {
        self.update_checking.store(true, Ordering::SeqCst);
        self.repaint();
        let result = update::check_latest();
        self.update_checking.store(false, Ordering::SeqCst);

        let message = match &result {
            Ok(Some(info)) => (format!("NextLine {} is available.", info.version), false),
            Ok(None) => (format!("You're up to date (version {}).", update::CURRENT_VERSION), false),
            Err(error) => (error.to_string(), true),
        };
        match result {
            Ok(found) => {
                *lock(&self.update) = found;
                let mut data = self.data();
                data.settings.last_update_check = update::now_unix();
                drop(data);
                self.mark_dirty();
            }
            // A missing release page just means nothing has been published yet.
            Err(CheckError::Unavailable(404)) => {
                *lock(&self.update) = None;
                self.data().settings.last_update_check = update::now_unix();
                self.mark_dirty();
            }
            Err(_) => {}
        }
        // Background checks stay quiet unless an update was found; manual ones always report.
        if manual || lock(&self.update).is_some() {
            *lock(&self.update_message) = Some(message);
        }
        self.repaint();
    }

    fn bindings(hotkeys: &Hotkeys) -> [Option<Combo>; 4] {
        HotkeyAction::ALL.map(|action| Combo::parse(combo_slot(hotkeys, action)).ok())
    }

    /// Registers every configured hotkey and remembers per-key failures for the settings UI.
    pub fn apply_hotkeys(&self) {
        let hotkeys = self.data().settings.hotkeys.clone();
        if let Some(service) = self.hotkeys.get() {
            let results = service.apply(Self::bindings(&hotkeys));
            *lock(&self.hotkey_errors) = results.map(Result::err);
        }
        self.repaint();
    }

    /// Frees every hotkey so the settings recorder can see keys like F2 that are normally swallowed.
    pub fn suspend_hotkeys(&self) {
        if let Some(service) = self.hotkeys.get() {
            let _ = service.apply([None; 4]);
        }
    }

    /// Changes one hotkey. On failure the previous binding is restored and the reason returned.
    pub fn set_hotkey(&self, action: HotkeyAction, combo: Combo) -> Result<(), String> {
        let taken = {
            let data = self.data();
            HotkeyAction::ALL
                .into_iter()
                .find(|&other| other != action && Combo::parse(combo_slot(&data.settings.hotkeys, other)).ok() == Some(combo))
        };
        if let Some(other) = taken {
            return Err(format!("{combo} is already used to \"{}\".", other.label().to_lowercase()));
        }

        let previous = combo_slot(&self.data().settings.hotkeys, action).clone();
        let mut hotkeys = self.data().settings.hotkeys.clone();
        *combo_slot_mut(&mut hotkeys, action) = combo.to_string();

        let results = self.hotkeys.get().ok_or("Hotkey service unavailable.")?.apply(Self::bindings(&hotkeys));
        match &results[action as usize] {
            Ok(()) => {
                self.data().settings.hotkeys = hotkeys;
                *lock(&self.hotkey_errors) = results.map(Result::err);
                self.mark_dirty();
                Ok(())
            }
            Err(error) => {
                let error = error.clone();
                *combo_slot_mut(&mut self.data().settings.hotkeys, action) = previous;
                self.apply_hotkeys();
                Err(error)
            }
        }
    }

    fn on_hotkey(self: &Arc<Core>, action: HotkeyAction) {
        match action {
            HotkeyAction::Type => self.type_current(Source::Hotkey),
            HotkeyAction::Stop => self.stop(),
            HotkeyAction::Skip | HotkeyAction::Back => {
                if self.is_busy() {
                    return;
                }
                {
                    let mut data = self.data();
                    if action == HotkeyAction::Skip {
                        data.advance();
                    } else {
                        data.go_back();
                    }
                }
                self.mark_dirty();
                self.repaint();
            }
        }
    }

    pub fn stop(&self) {
        if self.is_busy() {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Types the selected step into whatever window is focused, then advances.
    pub fn type_current(self: &Arc<Core>, source: Source) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        self.cancel.store(false, Ordering::SeqCst);

        let job = {
            let data = self.data();
            let step = data.selected_step().clone();
            let cfg = PlanConfig {
                cps: data.settings.speed_cps,
                humanize: data.settings.humanize,
                newline: data.settings.newline_mode,
            };
            (data.selected_script.clone(), step, cfg, data.settings.countdown_secs)
        };
        let (script_id, step, cfg, countdown_secs) = job;

        if step.is_empty() {
            self.busy.store(false, Ordering::SeqCst);
            self.push_notice("This step is empty. Add some text or an after-step key first.", true);
            return;
        }

        let core = Arc::clone(self);
        std::thread::Builder::new()
            .name("typing".into())
            .spawn(move || {
                let outcome = core.run_job(source, &script_id, &step, cfg, countdown_secs);
                core.set_status(Status::Idle);
                core.busy.store(false, Ordering::SeqCst);
                match outcome {
                    Ok(message) => core.push_notice(message, false),
                    Err(TypeError::Cancelled) => core.push_notice("Stopped. The step was not advanced.", false),
                    Err(error) => core.push_notice(error.to_string(), true),
                }
            })
            .expect("spawn typing thread");
    }

    fn run_job(
        &self,
        source: Source,
        script_id: &str,
        step: &crate::model::Step,
        cfg: PlanConfig,
        countdown_secs: u32,
    ) -> Result<String, TypeError> {
        match source {
            Source::Manual => {
                let ends = Instant::now() + Duration::from_secs(countdown_secs as u64);
                self.set_status(Status::Countdown { ends });
                typer::sleep_cancellable(ends.saturating_duration_since(Instant::now()), &self.cancel)?;
            }
            Source::Hotkey => {
                typer::wait_for_modifiers_released(Duration::from_millis(1500));
                typer::sleep_cancellable(Duration::from_millis(120), &self.cancel)?;
            }
        }
        if step.delay_ms > 0 {
            typer::sleep_cancellable(Duration::from_millis(step.delay_ms as u64), &self.cancel)?;
        }

        let actions = typer::plan(&step.body, cfg, &mut fastrand::Rng::new());
        let total = typer::total_chars(&actions).max(1);
        self.set_status(Status::Typing { done: 0, total });

        let mut last_paint = Instant::now();
        typer::execute(&actions, step.after_key, &self.cancel, |done| {
            *lock(&self.status) = Status::Typing { done, total };
            if last_paint.elapsed() > Duration::from_millis(33) {
                last_paint = Instant::now();
                self.repaint();
            }
        })?;

        let mut data = self.data();
        let still_current = data.selected_script == script_id && data.selected_step == step.id;
        if !still_current {
            return Ok("Typed.".into());
        }
        let step_number = data.step_index() + 1;
        let finished = data.advance();
        drop(data);
        self.mark_dirty();
        Ok(if finished { format!("Typed step {step_number}. Script complete.") } else { format!("Typed step {step_number}.") })
    }
}
