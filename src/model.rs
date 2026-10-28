//! Script library data model, settings and tolerant JSON import.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Key pressed after a step has been typed. Serialized names match the legacy Electron app.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum AfterKey {
    #[default]
    None,
    Enter,
    Tab,
    ShiftTab,
    Escape,
    Space,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    CtrlEnter,
    ShiftEnter,
}

impl AfterKey {
    pub const ALL: [AfterKey; 14] = [
        AfterKey::None,
        AfterKey::Enter,
        AfterKey::Tab,
        AfterKey::ShiftTab,
        AfterKey::Escape,
        AfterKey::Space,
        AfterKey::Backspace,
        AfterKey::Delete,
        AfterKey::ArrowUp,
        AfterKey::ArrowDown,
        AfterKey::ArrowLeft,
        AfterKey::ArrowRight,
        AfterKey::CtrlEnter,
        AfterKey::ShiftEnter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AfterKey::None => "Nothing",
            AfterKey::Enter => "Enter",
            AfterKey::Tab => "Tab",
            AfterKey::ShiftTab => "Shift + Tab",
            AfterKey::Escape => "Escape",
            AfterKey::Space => "Space",
            AfterKey::Backspace => "Backspace",
            AfterKey::Delete => "Delete",
            AfterKey::ArrowUp => "Arrow up",
            AfterKey::ArrowDown => "Arrow down",
            AfterKey::ArrowLeft => "Arrow left",
            AfterKey::ArrowRight => "Arrow right",
            AfterKey::CtrlEnter => "Ctrl + Enter",
            AfterKey::ShiftEnter => "Shift + Enter",
        }
    }

    fn parse(value: &str) -> AfterKey {
        AfterKey::ALL
            .iter()
            .copied()
            .find(|key| {
                serde_json::to_value(key)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s == value))
                    .unwrap_or(false)
            })
            .unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    pub id: String,
    pub body: String,
    pub after_key: AfterKey,
    /// Private presenter note shown next to the step; never typed.
    pub note: String,
    /// Extra wait before this step starts typing.
    pub delay_ms: u32,
}

impl Default for Step {
    fn default() -> Self {
        Step { id: new_id(), body: String::new(), after_key: AfterKey::None, note: String::new(), delay_ms: 0 }
    }
}

impl Step {
    pub fn new(body: impl Into<String>) -> Step {
        Step { body: body.into(), ..Step::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.body.trim().is_empty() && self.after_key == AfterKey::None
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Script {
    pub id: String,
    pub title: String,
    pub category: String,
    pub steps: Vec<Step>,
}

impl Default for Script {
    fn default() -> Self {
        Script { id: new_id(), title: String::new(), category: String::new(), steps: vec![Step::default()] }
    }
}

impl Script {
    pub fn duplicate(&self) -> Script {
        let mut copy = self.clone();
        copy.id = new_id();
        copy.title = format!("{} copy", if copy.title.trim().is_empty() { "Untitled script" } else { copy.title.trim() });
        for step in &mut copy.steps {
            step.id = new_id();
        }
        copy
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.title.to_lowercase().contains(&query)
            || self.category.to_lowercase().contains(&query)
            || self.steps.iter().any(|s| s.body.to_lowercase().contains(&query) || s.note.to_lowercase().contains(&query))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum NewlineMode {
    /// Safe for chat boxes where Enter submits the message.
    #[default]
    ShiftEnter,
    Enter,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub type_step: String,
    pub skip: String,
    pub back: String,
    pub stop: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys { type_step: "F2".into(), skip: "F3".into(), back: "Shift+F3".into(), stop: "Shift+F2".into() }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkeys: Hotkeys,
    /// Typing speed in characters per second; 0 types the whole step instantly.
    pub speed_cps: u32,
    pub humanize: bool,
    pub newline_mode: NewlineMode,
    pub wrap_at_end: bool,
    pub close_to_tray: bool,
    pub hide_from_capture: bool,
    pub theme: ThemeChoice,
    pub countdown_secs: u32,
    /// Check GitHub for a newer release about once a day.
    pub check_updates: bool,
    /// Unix seconds of the last completed update check (0 = never).
    pub last_update_check: u64,
    /// A release version the user chose to skip, e.g. "0.2.0".
    pub skipped_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkeys: Hotkeys::default(),
            speed_cps: 25,
            humanize: true,
            newline_mode: NewlineMode::default(),
            wrap_at_end: true,
            close_to_tray: true,
            hide_from_capture: false,
            theme: ThemeChoice::default(),
            countdown_secs: 3,
            check_updates: true,
            last_update_check: 0,
            skipped_version: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct AppData {
    pub version: u32,
    pub scripts: Vec<Script>,
    pub selected_script: String,
    pub selected_step: String,
    pub settings: Settings,
}

impl Default for AppData {
    fn default() -> Self {
        let scripts = sample_scripts();
        AppData {
            version: 3,
            selected_script: scripts[0].id.clone(),
            selected_step: scripts[0].steps[0].id.clone(),
            scripts,
            settings: Settings::default(),
        }
    }
}

impl AppData {
    /// Repairs selection pointers and guarantees at least one script with one step.
    pub fn sanitize(&mut self) {
        if self.scripts.is_empty() {
            self.scripts.push(Script { title: "My first script".into(), category: "General".into(), ..Script::default() });
        }
        for script in &mut self.scripts {
            if script.steps.is_empty() {
                script.steps.push(Step::default());
            }
        }
        if !self.scripts.iter().any(|s| s.id == self.selected_script) {
            self.selected_script = self.scripts[0].id.clone();
        }
        let script = self.scripts.iter().find(|s| s.id == self.selected_script).expect("selected script exists");
        if !script.steps.iter().any(|s| s.id == self.selected_step) {
            self.selected_step = script.steps[0].id.clone();
        }
        // Speeds above "Fast" (and the removed "Instant" = 0) were unreliable, so saved values snap to a preset.
        const SPEEDS: [u32; 3] = [12, 25, 50];
        let saved = self.settings.speed_cps;
        self.settings.speed_cps =
            if saved == 0 || saved > 50 { 50 } else { SPEEDS.into_iter().min_by_key(|s| s.abs_diff(saved)).unwrap_or(25) };
        self.settings.countdown_secs = self.settings.countdown_secs.clamp(1, 15);
    }

    pub fn script_index(&self) -> usize {
        self.scripts.iter().position(|s| s.id == self.selected_script).unwrap_or(0)
    }

    pub fn selected_script(&self) -> &Script {
        &self.scripts[self.script_index()]
    }

    pub fn step_index(&self) -> usize {
        let script = self.selected_script();
        script.steps.iter().position(|s| s.id == self.selected_step).unwrap_or(0)
    }

    pub fn selected_step(&self) -> &Step {
        &self.selected_script().steps[self.step_index()]
    }

    pub fn select_script(&mut self, id: &str) {
        if let Some(script) = self.scripts.iter().find(|s| s.id == id) {
            self.selected_script = script.id.clone();
            self.selected_step = script.steps[0].id.clone();
        }
    }

    pub fn select_step_index(&mut self, index: usize) {
        let script = &self.scripts[self.script_index()];
        if let Some(step) = script.steps.get(index) {
            self.selected_step = step.id.clone();
        }
    }

    /// Moves to the next step. Returns `true` if the end of the script was passed.
    pub fn advance(&mut self) -> bool {
        let index = self.step_index();
        let len = self.selected_script().steps.len();
        if index + 1 < len {
            self.select_step_index(index + 1);
            false
        } else {
            if self.settings.wrap_at_end {
                self.select_step_index(0);
            }
            true
        }
    }

    pub fn go_back(&mut self) {
        let index = self.step_index();
        if index > 0 {
            self.select_step_index(index - 1);
        } else if self.settings.wrap_at_end {
            let last = self.selected_script().steps.len() - 1;
            self.select_step_index(last);
        }
    }

    pub fn categories(&self) -> Vec<String> {
        let mut categories: Vec<String> =
            self.scripts.iter().map(|s| s.category.trim().to_string()).filter(|c| !c.is_empty()).collect();
        categories.sort_by_key(|c| c.to_lowercase());
        categories.dedup();
        categories
    }
}

pub fn sample_scripts() -> Vec<Script> {
    vec![
        Script {
            id: new_id(),
            title: "Chatbot Product Demo".into(),
            category: "Chatbots".into(),
            steps: vec![
                Step {
                    note: "Open with the comparison - this shows the assistant using the plan data.".into(),
                    ..Step::new("Can you help me compare the Basic and Premium support plans for a 2,000-person company?")
                        .with_key(AfterKey::Enter)
                },
                Step::new("Summarize the recommendation in three bullets and include the biggest tradeoff.")
                    .with_key(AfterKey::Enter),
                Step::new("Draft a customer-ready response that explains the recommended plan and next steps.")
                    .with_key(AfterKey::Enter),
            ],
        },
        Script {
            id: new_id(),
            title: "Lead Intake Form".into(),
            category: "Forms".into(),
            steps: vec![
                Step::new("Avery Stone").with_key(AfterKey::Tab),
                Step::new("Harbor & Pine Co.").with_key(AfterKey::Tab),
                Step::new("Interested in a guided product demo for customer service automation."),
            ],
        },
    ]
}

impl Step {
    fn with_key(mut self, key: AfterKey) -> Step {
        self.after_key = key;
        self
    }
}

fn str_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| value.get(*name).and_then(Value::as_str))
}

/// Tolerant parser that also understands the legacy `items` / `name` / `text` field names.
pub fn normalize_script(raw: &Value) -> Option<Script> {
    if !raw.is_object() {
        return None;
    }

    let raw_steps = raw.get("steps").or_else(|| raw.get("items")).and_then(Value::as_array);
    let mut steps: Vec<Step> = raw_steps
        .map(|steps| {
            steps
                .iter()
                .filter(|s| s.is_object())
                .map(|s| Step {
                    id: str_field(s, &["id"]).filter(|id| !id.is_empty()).map(str::to_string).unwrap_or_else(new_id),
                    body: str_field(s, &["body", "text"]).unwrap_or_default().to_string(),
                    after_key: str_field(s, &["afterKey"]).map(AfterKey::parse).unwrap_or_default(),
                    note: str_field(s, &["note"]).unwrap_or_default().to_string(),
                    delay_ms: s.get("delayMs").and_then(Value::as_u64).unwrap_or(0).min(60_000) as u32,
                })
                .collect()
        })
        .unwrap_or_default();
    if steps.is_empty() {
        steps.push(Step::default());
    }

    let non_empty = |names: &[&str]| str_field(raw, names).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    Some(Script {
        id: str_field(raw, &["id"]).filter(|id| !id.is_empty()).map(str::to_string).unwrap_or_else(new_id),
        title: non_empty(&["title", "name"]).unwrap_or_else(|| "Imported script".into()),
        category: non_empty(&["category"]).unwrap_or_else(|| "Imported".into()),
        steps,
    })
}

/// Parses an import payload (`{ "scripts": [...] }` or a bare array). Imported scripts always get fresh IDs.
pub fn parse_import(json: &str) -> Result<Vec<Script>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("Not valid JSON: {e}"))?;
    let list = match &value {
        Value::Array(list) => list.as_slice(),
        Value::Object(_) => match value.get("scripts").and_then(Value::as_array) {
            Some(list) => list.as_slice(),
            None => std::slice::from_ref(&value),
        },
        _ => &[],
    };

    let mut scripts: Vec<Script> = list.iter().filter_map(normalize_script).collect();
    if scripts.is_empty() {
        return Err("No valid scripts found in that file.".into());
    }
    for script in &mut scripts {
        script.id = new_id();
        for step in &mut script.steps {
            step.id = new_id();
        }
    }
    Ok(scripts)
}

pub fn export_json(scripts: &[Script]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ "scripts": scripts })).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_legacy_fields_and_assigns_fresh_ids() {
        let json = r#"{"scripts":[{"id":"keep","name":" Old ","items":[{"id":"s","text":"hi","afterKey":"shiftTab"},{"text":"bye","afterKey":"bogus"}]}]}"#;
        let scripts = parse_import(json).unwrap();
        assert_eq!(scripts[0].title, "Old");
        assert_eq!(scripts[0].category, "Imported");
        assert_ne!(scripts[0].id, "keep");
        assert_eq!(scripts[0].steps[0].body, "hi");
        assert_eq!(scripts[0].steps[0].after_key, AfterKey::ShiftTab);
        assert_eq!(scripts[0].steps[1].after_key, AfterKey::None);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_import("not json").is_err());
        assert!(parse_import("{\"scripts\": []}").is_err());
    }

    #[test]
    fn export_round_trips() {
        let scripts = sample_scripts();
        let again = parse_import(&export_json(&scripts)).unwrap();
        assert_eq!(again.len(), scripts.len());
        assert_eq!(again[0].steps[0].body, scripts[0].steps[0].body);
        assert_eq!(again[0].steps[0].after_key, AfterKey::Enter);
        assert_eq!(again[0].steps[0].note, scripts[0].steps[0].note);
    }

    #[test]
    fn advance_wraps_or_stays() {
        let mut data = AppData::default();
        let last = data.selected_script().steps.len() - 1;
        data.select_step_index(last);
        assert!(data.advance());
        assert_eq!(data.step_index(), 0);
        data.settings.wrap_at_end = false;
        data.select_step_index(last);
        assert!(data.advance());
        assert_eq!(data.step_index(), last);
    }

    #[test]
    fn skill_examples_import_cleanly() {
        for json in [
            include_str!("../skills/nextline-scripts/examples/chatbot-demo.nextline.json"),
            include_str!("../skills/nextline-scripts/examples/form-demo.nextline.json"),
        ] {
            let scripts = parse_import(json).expect("example imports");
            assert!(!scripts.is_empty());
            assert!(scripts.iter().all(|s| !s.title.is_empty() && s.steps.iter().any(|st| !st.is_empty())));
        }
    }

    #[test]
    fn sanitize_snaps_speed_to_presets() {
        for (saved, expected) in [(0, 50), (120, 50), (13, 12), (30, 25), (50, 50), (25, 25)] {
            let mut data = AppData::default();
            data.settings.speed_cps = saved;
            data.sanitize();
            assert_eq!(data.settings.speed_cps, expected, "saved {saved}");
        }
    }

    #[test]
    fn sanitize_repairs_selection() {
        let mut data = AppData { scripts: vec![], selected_script: "x".into(), selected_step: "y".into(), ..AppData::default() };
        data.sanitize();
        assert_eq!(data.scripts.len(), 1);
        assert_eq!(data.selected_script, data.scripts[0].id);
        assert_eq!(data.selected_step, data.scripts[0].steps[0].id);
    }
}
