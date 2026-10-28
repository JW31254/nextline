//! Typing engine: a pure planner that turns text into timed actions, and a Win32 `SendInput` executor.
//!
//! Characters are injected as Unicode key events, so the clipboard is never touched and
//! any character (including emoji) can be typed regardless of keyboard layout.

use crate::model::{AfterKey, NewlineMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub vk: u16,
    pub shift: bool,
    pub ctrl: bool,
    pub extended: bool,
}

const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_DELETE: u16 = 0x2E;

impl Chord {
    const fn plain(vk: u16) -> Chord {
        Chord { vk, shift: false, ctrl: false, extended: false }
    }
    const fn shifted(vk: u16) -> Chord {
        Chord { vk, shift: true, ctrl: false, extended: false }
    }
    const fn ctrl(vk: u16) -> Chord {
        Chord { vk, shift: false, ctrl: true, extended: false }
    }
    const fn nav(vk: u16) -> Chord {
        Chord { vk, shift: false, ctrl: false, extended: true }
    }

    pub fn from_after_key(key: AfterKey) -> Option<Chord> {
        Some(match key {
            AfterKey::None => return None,
            AfterKey::Enter => Chord::plain(VK_RETURN),
            AfterKey::Tab => Chord::plain(VK_TAB),
            AfterKey::ShiftTab => Chord::shifted(VK_TAB),
            AfterKey::Escape => Chord::plain(VK_ESCAPE),
            AfterKey::Space => Chord::plain(VK_SPACE),
            AfterKey::Backspace => Chord::plain(VK_BACK),
            AfterKey::Delete => Chord::nav(VK_DELETE),
            AfterKey::ArrowUp => Chord::nav(VK_UP),
            AfterKey::ArrowDown => Chord::nav(VK_DOWN),
            AfterKey::ArrowLeft => Chord::nav(VK_LEFT),
            AfterKey::ArrowRight => Chord::nav(VK_RIGHT),
            AfterKey::CtrlEnter => Chord::ctrl(VK_RETURN),
            AfterKey::ShiftEnter => Chord::shifted(VK_RETURN),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// UTF-16 code units to inject; `chars` is the number of characters they represent.
    Text { units: Vec<u16>, chars: usize },
    Key(Chord),
    Sleep(Duration),
}

#[derive(Clone, Copy, Debug)]
pub struct PlanConfig {
    /// Characters per second. 0 means instant.
    pub cps: u32,
    pub humanize: bool,
    pub newline: NewlineMode,
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

fn is_clause_end(c: char) -> bool {
    matches!(c, ',' | ';' | ':' | '—' | '–')
}

/// Builds the action list for `text`. Deterministic for a seeded `rng`.
pub fn plan(text: &str, cfg: PlanConfig, rng: &mut fastrand::Rng) -> Vec<Action> {
    let chars: Vec<char> = text.replace("\r\n", "\n").replace('\r', "\n").chars().collect();
    let mut actions = Vec::new();

    let instant = cfg.cps == 0;
    let cps = cfg.cps.max(1) as f32;
    // Timer resolution is ~1-15ms, so batch characters when the per-char interval is tiny.
    let chunk_len = if instant { usize::MAX } else { ((cps * 0.012).ceil() as usize).max(1) };
    let pause_scale = (14.0 / cps).clamp(0.12, 1.2);

    let mut chunk: Vec<char> = Vec::new();
    let flush = |chunk: &mut Vec<char>, actions: &mut Vec<Action>, extra_ms: f32, rng: &mut fastrand::Rng| {
        if chunk.is_empty() {
            return;
        }
        let text: String = chunk.iter().collect();
        let count = chunk.len();
        actions.push(Action::Text { units: text.encode_utf16().collect(), chars: count });
        if !instant {
            let mut ms = count as f32 * 1000.0 / cps;
            if cfg.humanize {
                ms *= 0.6 + rng.f32() * 0.9;
                ms += extra_ms * (0.7 + rng.f32() * 0.6);
            }
            actions.push(Action::Sleep(Duration::from_secs_f32(ms.max(1.0) / 1000.0)));
        }
        chunk.clear();
    };

    for (i, &c) in chars.iter().enumerate() {
        match c {
            '\n' | '\t' => {
                flush(&mut chunk, &mut actions, 0.0, rng);
                let chord = match (c, cfg.newline) {
                    ('\t', _) => Chord::plain(VK_TAB),
                    (_, NewlineMode::ShiftEnter) => Chord::shifted(VK_RETURN),
                    (_, NewlineMode::Enter) => Chord::plain(VK_RETURN),
                };
                actions.push(Action::Key(chord));
                if !instant {
                    let ms = if cfg.humanize && c == '\n' { 320.0 * pause_scale } else { 1000.0 / cps };
                    actions.push(Action::Sleep(Duration::from_secs_f32(ms.max(1.0) / 1000.0)));
                }
            }
            _ => {
                chunk.push(c);
                let next = chars.get(i + 1).copied();
                // Punctuation only pauses when it ends a word ("3.14" and "e.g." mid-word do not).
                let boundary = next.is_none_or(|n| n.is_whitespace());
                let extra = if cfg.humanize && boundary {
                    if is_sentence_end(c) {
                        300.0 * pause_scale
                    } else if is_clause_end(c) {
                        150.0 * pause_scale
                    } else if c.is_whitespace() {
                        45.0 * pause_scale
                    } else {
                        0.0
                    }
                } else if cfg.humanize && c == ' ' {
                    45.0 * pause_scale
                } else {
                    0.0
                };
                if chunk.len() >= chunk_len || (!instant && extra > 0.0) {
                    flush(&mut chunk, &mut actions, extra, rng);
                }
            }
        }
    }
    flush(&mut chunk, &mut actions, 0.0, rng);
    actions
}

pub fn total_chars(actions: &[Action]) -> usize {
    actions
        .iter()
        .map(|a| match a {
            Action::Text { chars, .. } => *chars,
            Action::Key(_) => 1,
            Action::Sleep(_) => 0,
        })
        .sum()
}

#[derive(Debug, PartialEq, Eq)]
pub enum TypeError {
    Cancelled,
    /// Windows rejected the input, typically because the target app runs elevated.
    InputBlocked,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeError::Cancelled => write!(f, "Stopped."),
            TypeError::InputBlocked => write!(
                f,
                "Windows blocked the keystrokes. The target app may be running as administrator - run NextLine as administrator too."
            ),
        }
    }
}

/// Sleeps in short slices so cancellation stays responsive.
pub fn sleep_cancellable(duration: Duration, cancel: &AtomicBool) -> Result<(), TypeError> {
    let end = Instant::now() + duration;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(TypeError::Cancelled);
        }
        let now = Instant::now();
        if now >= end {
            return Ok(());
        }
        std::thread::sleep((end - now).min(Duration::from_millis(15)));
    }
}

/// Runs a plan against the foreground window, reporting characters typed so far.
pub fn execute(
    actions: &[Action],
    after_key: AfterKey,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize),
) -> Result<(), TypeError> {
    let _timer = win::HighResTimer::begin();
    let mut done = 0usize;

    for action in actions {
        if cancel.load(Ordering::Relaxed) {
            return Err(TypeError::Cancelled);
        }
        match action {
            Action::Text { units, chars } => {
                for part in units.chunks(200) {
                    win::send_units(part)?;
                }
                done += chars;
                progress(done);
            }
            Action::Key(chord) => {
                win::send_chord(*chord)?;
                done += 1;
                progress(done);
            }
            Action::Sleep(duration) => sleep_cancellable(*duration, cancel)?,
        }
    }

    if let Some(chord) = Chord::from_after_key(after_key) {
        sleep_cancellable(Duration::from_millis(120), cancel)?;
        win::send_chord(chord)?;
    }
    Ok(())
}

/// Blocks until Ctrl/Alt/Shift/Win are physically released (e.g. after a modified hotkey).
pub fn wait_for_modifiers_released(timeout: Duration) {
    let end = Instant::now() + timeout;
    while win::modifiers_down() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(15));
    }
}

mod win {
    use super::{Chord, TypeError};
    use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    pub struct HighResTimer;
    impl HighResTimer {
        pub fn begin() -> HighResTimer {
            unsafe { timeBeginPeriod(1) };
            HighResTimer
        }
    }
    impl Drop for HighResTimer {
        fn drop(&mut self) {
            unsafe { timeEndPeriod(1) };
        }
    }

    fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        }
    }

    fn send(inputs: &[INPUT]) -> Result<(), TypeError> {
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() { Ok(()) } else { Err(TypeError::InputBlocked) }
    }

    pub fn send_units(units: &[u16]) -> Result<(), TypeError> {
        let mut inputs = Vec::with_capacity(units.len() * 2);
        for &unit in units {
            inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
            inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        send(&inputs)
    }

    pub fn send_chord(chord: Chord) -> Result<(), TypeError> {
        let ext = if chord.extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
        let mut inputs = Vec::new();
        if chord.ctrl {
            inputs.push(key_input(VK_CONTROL, 0, 0));
        }
        if chord.shift {
            inputs.push(key_input(VK_SHIFT, 0, 0));
        }
        inputs.push(key_input(chord.vk, 0, ext));
        inputs.push(key_input(chord.vk, 0, ext | KEYEVENTF_KEYUP));
        if chord.shift {
            inputs.push(key_input(VK_SHIFT, 0, KEYEVENTF_KEYUP));
        }
        if chord.ctrl {
            inputs.push(key_input(VK_CONTROL, 0, KEYEVENTF_KEYUP));
        }
        send(&inputs)
    }

    pub fn modifiers_down() -> bool {
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|&vk| unsafe { GetAsyncKeyState(vk as i32) } < 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(cps: u32, humanize: bool) -> PlanConfig {
        PlanConfig { cps, humanize, newline: NewlineMode::ShiftEnter }
    }

    fn typed_text(actions: &[Action]) -> String {
        let units: Vec<u16> = actions
            .iter()
            .filter_map(|a| if let Action::Text { units, .. } = a { Some(units.clone()) } else { None })
            .flatten()
            .collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn plan_preserves_text_including_emoji() {
        let text = "Hello, world! Caf\u{e9} \u{1F680} done.";
        for cps in [0, 8, 28, 120] {
            let actions = plan(text, cfg(cps, true), &mut fastrand::Rng::with_seed(1));
            assert_eq!(typed_text(&actions), text, "cps {cps}");
        }
    }

    #[test]
    fn instant_has_no_sleeps() {
        let actions = plan("one two three", cfg(0, true), &mut fastrand::Rng::with_seed(1));
        assert!(actions.iter().all(|a| !matches!(a, Action::Sleep(_))));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn newlines_and_tabs_become_keys() {
        let actions = plan("a\r\nb\tc", cfg(0, false), &mut fastrand::Rng::with_seed(1));
        let keys: Vec<Chord> = actions.iter().filter_map(|a| if let Action::Key(k) = a { Some(*k) } else { None }).collect();
        assert_eq!(keys, vec![Chord::shifted(VK_RETURN), Chord::plain(VK_TAB)]);
        assert_eq!(typed_text(&actions), "abc");
    }

    #[test]
    fn enter_mode_uses_plain_enter() {
        let config = PlanConfig { cps: 0, humanize: false, newline: NewlineMode::Enter };
        let actions = plan("a\nb", config, &mut fastrand::Rng::with_seed(1));
        assert!(actions.contains(&Action::Key(Chord::plain(VK_RETURN))));
    }

    #[test]
    fn decimals_do_not_pause_mid_number() {
        let actions = plan("3.14", cfg(14, true), &mut fastrand::Rng::with_seed(1));
        let sleeps = actions.iter().filter(|a| matches!(a, Action::Sleep(_))).count();
        assert!(sleeps <= 4);
        assert_eq!(typed_text(&actions), "3.14");
    }

    #[test]
    fn sentence_end_adds_pause() {
        let slow = |text: &str| -> Duration {
            plan(text, cfg(14, false), &mut fastrand::Rng::with_seed(7))
                .iter()
                .filter_map(|a| if let Action::Sleep(d) = a { Some(*d) } else { None })
                .sum()
        };
        let human: Duration = plan("Hi. There", cfg(14, true), &mut fastrand::Rng::with_seed(7))
            .iter()
            .filter_map(|a| if let Action::Sleep(d) = a { Some(*d) } else { None })
            .sum();
        assert!(human > slow("Hi. There") / 2);
    }

    #[test]
    fn counts_characters() {
        let actions = plan("ab\ncd", cfg(0, false), &mut fastrand::Rng::with_seed(1));
        assert_eq!(total_chars(&actions), 5);
    }

    #[test]
    fn cancel_interrupts_sleep() {
        let cancel = AtomicBool::new(true);
        assert_eq!(sleep_cancellable(Duration::from_secs(5), &cancel), Err(TypeError::Cancelled));
    }

    #[test]
    fn after_key_chords() {
        assert_eq!(Chord::from_after_key(AfterKey::None), None);
        assert_eq!(Chord::from_after_key(AfterKey::ShiftTab), Some(Chord::shifted(VK_TAB)));
        assert!(Chord::from_after_key(AfterKey::ArrowDown).unwrap().extended);
    }
}
