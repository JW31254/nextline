//! Global hotkeys: combo parsing/formatting and a Win32 `RegisterHotKey` listener thread.

use std::sync::mpsc::{self, Sender};
use std::thread;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    Type,
    Skip,
    Back,
    Stop,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 4] = [HotkeyAction::Type, HotkeyAction::Skip, HotkeyAction::Back, HotkeyAction::Stop];

    fn id(self) -> i32 {
        self as i32 + 1
    }

    fn from_id(id: i32) -> Option<HotkeyAction> {
        HotkeyAction::ALL.iter().copied().find(|a| a.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            HotkeyAction::Type => "Type current step",
            HotkeyAction::Skip => "Skip to next step",
            HotkeyAction::Back => "Go back one step",
            HotkeyAction::Stop => "Stop typing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u16,
}

/// Canonical key names, shared by the parser, formatter and the in-app recorder.
fn key_table() -> Vec<(String, u16)> {
    let mut keys: Vec<(String, u16)> = Vec::new();
    for n in 1..=24u16 {
        keys.push((format!("F{n}"), VK_F1 + n - 1));
    }
    for c in b'A'..=b'Z' {
        keys.push(((c as char).to_string(), c as u16));
    }
    for c in b'0'..=b'9' {
        keys.push(((c as char).to_string(), c as u16));
    }
    for n in 0..=9u16 {
        keys.push((format!("Numpad{n}"), VK_NUMPAD0 + n));
    }
    for (name, vk) in [
        ("Space", VK_SPACE),
        ("Insert", VK_INSERT),
        ("Delete", VK_DELETE),
        ("Home", VK_HOME),
        ("End", VK_END),
        ("PageUp", VK_PRIOR),
        ("PageDown", VK_NEXT),
        ("Up", VK_UP),
        ("Down", VK_DOWN),
        ("Left", VK_LEFT),
        ("Right", VK_RIGHT),
        ("Pause", VK_PAUSE),
        ("ScrollLock", VK_SCROLL),
    ] {
        keys.push((name.to_string(), vk));
    }
    keys
}

pub fn vk_from_name(name: &str) -> Option<u16> {
    let name = match name {
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        other => other,
    };
    key_table().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, vk)| vk)
}

fn name_from_vk(vk: u16) -> Option<String> {
    key_table().into_iter().find(|(_, v)| *v == vk).map(|(n, _)| n)
}

impl Combo {
    pub fn parse(text: &str) -> Result<Combo, String> {
        let mut combo = Combo { ctrl: false, alt: false, shift: false, win: false, vk: 0 };
        let mut key = None;
        for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "commandorcontrol" => combo.ctrl = true,
                "alt" => combo.alt = true,
                "shift" => combo.shift = true,
                "win" | "super" | "meta" => combo.win = true,
                _ => {
                    if key.is_some() {
                        return Err(format!("\"{text}\" has more than one main key."));
                    }
                    key = Some(vk_from_name(part).ok_or_else(|| format!("\"{part}\" is not a supported key."))?);
                }
            }
        }
        combo.vk = key.ok_or_else(|| "Choose a key.".to_string())?;
        combo.validate()?;
        Ok(combo)
    }

    /// Bare typing keys would hijack normal typing everywhere, so they need a modifier.
    pub fn validate(&self) -> Result<(), String> {
        let has_modifier = self.ctrl || self.alt || self.win;
        let is_typing_key = matches!(self.vk, 0x30..=0x39 | 0x41..=0x5A) || self.vk == VK_SPACE;
        if is_typing_key && !has_modifier {
            return Err("Letters, numbers and Space need Ctrl, Alt or Win as well.".into());
        }
        Ok(())
    }

    pub fn from_parts(ctrl: bool, alt: bool, shift: bool, win: bool, key_name: &str) -> Result<Combo, String> {
        let vk = vk_from_name(key_name).ok_or_else(|| format!("\"{key_name}\" can't be used as a hotkey."))?;
        let combo = Combo { ctrl, alt, shift, win, vk };
        combo.validate()?;
        Ok(combo)
    }

    fn modifiers(&self) -> u32 {
        let mut m = MOD_NOREPEAT;
        if self.ctrl {
            m |= MOD_CONTROL;
        }
        if self.alt {
            m |= MOD_ALT;
        }
        if self.shift {
            m |= MOD_SHIFT;
        }
        if self.win {
            m |= MOD_WIN;
        }
        m
    }
}

impl std::fmt::Display for Combo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", name_from_vk(self.vk).unwrap_or_else(|| format!("0x{:02X}", self.vk)))
    }
}

type Bindings = [Option<Combo>; 4];

enum Command {
    Apply(Bindings, Sender<[Result<(), String>; 4]>),
}

pub struct HotkeyService {
    commands: Sender<Command>,
    thread_id: u32,
}

impl HotkeyService {
    /// Starts the listener thread. `on_hotkey` runs on that thread and must not block.
    pub fn start(on_hotkey: impl Fn(HotkeyAction) + Send + 'static) -> HotkeyService {
        let (commands, command_rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();

        thread::Builder::new()
            .name("hotkeys".into())
            .spawn(move || unsafe {
                let mut msg: MSG = std::mem::zeroed();
                // Touching the queue first guarantees PostThreadMessage can reach this thread.
                PeekMessageW(&mut msg, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
                let _ = ready_tx.send(GetCurrentThreadId());

                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    match msg.message {
                        WM_HOTKEY => {
                            if let Some(action) = HotkeyAction::from_id(msg.wParam as i32) {
                                on_hotkey(action);
                            }
                        }
                        WM_APP => {
                            while let Ok(Command::Apply(bindings, reply)) = command_rx.try_recv() {
                                let mut results: [Result<(), String>; 4] = [Ok(()), Ok(()), Ok(()), Ok(())];
                                for action in HotkeyAction::ALL {
                                    UnregisterHotKey(std::ptr::null_mut(), action.id());
                                }
                                for action in HotkeyAction::ALL {
                                    let slot = action as usize;
                                    if let Some(combo) = bindings[slot] {
                                        let ok = RegisterHotKey(
                                            std::ptr::null_mut(),
                                            action.id(),
                                            combo.modifiers(),
                                            combo.vk as u32,
                                        );
                                        if ok == 0 {
                                            results[slot] = Err(format!("{combo} is already in use by another app."));
                                        }
                                    }
                                }
                                let _ = reply.send(results);
                            }
                        }
                        _ => {}
                    }
                }
            })
            .expect("spawn hotkey thread");

        let thread_id = ready_rx.recv().expect("hotkey thread started");
        HotkeyService { commands, thread_id }
    }

    /// Replaces all registrations; each slot reports its own success so one conflict doesn't disable the rest.
    pub fn apply(&self, bindings: Bindings) -> [Result<(), String>; 4] {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.commands.send(Command::Apply(bindings, reply_tx)).is_ok() {
            unsafe { PostThreadMessageW(self.thread_id, WM_APP, 0, 0) };
            if let Ok(results) = reply_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                return results;
            }
        }
        std::array::from_fn(|_| Err("Hotkey service is not responding.".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        let combo = Combo::parse("shift + f3").unwrap();
        assert!(combo.shift && !combo.ctrl);
        assert_eq!(combo.to_string(), "Shift+F3");
        assert_eq!(Combo::parse("CommandOrControl+Alt+D").unwrap().to_string(), "Ctrl+Alt+D");
        assert_eq!(Combo::parse("Ctrl+ArrowUp").unwrap().to_string(), "Ctrl+Up");
    }

    #[test]
    fn rejects_bad_combos() {
        assert!(Combo::parse("A").is_err());
        assert!(Combo::parse("Shift+5").is_err());
        assert!(Combo::parse("Ctrl+").is_err());
        assert!(Combo::parse("F2+F3").is_err());
        assert!(Combo::parse("Ctrl+Banana").is_err());
        assert!(Combo::parse("F2").is_ok());
    }
}
