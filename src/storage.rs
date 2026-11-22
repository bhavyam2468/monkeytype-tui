use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::TestResult;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonalBest {
    pub wpm: f64,
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub mode: String,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsData {
    pub tests_completed: u64,
    pub personal_bests: HashMap<String, PersonalBest>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimSettings {
    pub line_fade: bool,
    pub pace_caret: bool,
    pub restart_scramble: bool,
    pub mini_keyboard: bool,
    pub combo_aura: bool,
    pub typo_nudge: bool,
    pub theme_fade: bool,
    pub sparks: bool,
    pub mascot: bool,
    pub wave: bool,
    pub soft_trail: bool,
}

impl Default for AnimSettings {
    fn default() -> Self {
        Self {
            line_fade: true,
            pace_caret: true,
            restart_scramble: true,
            mini_keyboard: true,
            combo_aura: true,
            typo_nudge: true,
            theme_fade: true,
            sparks: true,
            mascot: true,
            wave: true,
            soft_trail: false,
        }
    }
}

impl AnimSettings {
    pub const COUNT: usize = 11;

    pub fn items(&self) -> [(&'static str, &'static str, bool); Self::COUNT] {
        [
            ("Line Scroll Fade", "Smoothly fade incoming lines when scrolling", self.line_fade),
            ("Pace Caret (Ghost)", "Race a dimmed ghost cursor at your PB WPM", self.pace_caret),
            ("Restart Scramble", "Glitch-decode animation when starting fresh words", self.restart_scramble),
            ("Mini Keyboard", "Live QWERTY heatmap reacting to every keypress", self.mini_keyboard),
            ("Combo Aura & Confetti", "Breathing streak bar and PB confetti rain", self.combo_aura),
            ("Typo Micro-Shake", "Subtle 1-cell horizontal nudge on missed keys", self.typo_nudge),
            ("Theme Cross-Fade", "Smooth RGB transition on F2 or DMS theme sync", self.theme_fade),
            ("Word Sparks", "Floating ASCII sparkles on clean words and typos", self.sparks),
            ("ASCII Mascot", "Reactive typing monkey mascot in the header", self.mascot),
            ("Keystroke Wave", "Animated equalizer bar below the typing area", self.wave),
            ("Software RGB Trail", "Extra cell-bg trail (off by default for Kitty)", self.soft_trail),
        ]
    }

    pub fn toggle(&mut self, index: usize) {
        match index {
            0 => self.line_fade = !self.line_fade,
            1 => self.pace_caret = !self.pace_caret,
            2 => self.restart_scramble = !self.restart_scramble,
            3 => self.mini_keyboard = !self.mini_keyboard,
            4 => self.combo_aura = !self.combo_aura,
            5 => self.typo_nudge = !self.typo_nudge,
            6 => self.theme_fade = !self.theme_fade,
            7 => self.sparks = !self.sparks,
            8 => self.mascot = !self.mascot,
            9 => self.wave = !self.wave,
            10 => self.soft_trail = !self.soft_trail,
            _ => {}
        }
    }

    pub fn set_all_animations(&mut self, enabled: bool) {
        self.line_fade = enabled;
        self.pace_caret = enabled;
        self.restart_scramble = enabled;
        self.mini_keyboard = enabled;
        self.combo_aura = enabled;
        self.typo_nudge = enabled;
        self.theme_fade = enabled;
        self.sparks = enabled;
        self.mascot = enabled;
        self.wave = enabled;
    }
}

pub fn default_settings_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg)
                .join("monkeytype-tui")
                .join("settings.json");
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("monkeytype-tui")
        .join("settings.json")
}

pub fn load_settings() -> AnimSettings {
    let target = default_settings_path();
    match fs::read_to_string(&target) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AnimSettings::default(),
    }
}

pub fn save_settings(settings: &AnimSettings) {
    let target = default_settings_path();
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(&target, json);
    }
}

pub fn default_stats_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg)
                .join("monkeytype-tui")
                .join("stats.json");
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("monkeytype-tui")
        .join("stats.json")
}

pub fn load_stats(path: Option<&Path>) -> StatsData {
    let target = path.map(PathBuf::from).unwrap_or_else(default_stats_path);
    match fs::read_to_string(&target) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => StatsData::default(),
    }
}

pub fn record_result(result: &TestResult, path: Option<&Path>) -> bool {
    let target = path.map(PathBuf::from).unwrap_or_else(default_stats_path);
    let mut stats = load_stats(Some(&target));
    stats.tests_completed += 1;

    let prev = stats
        .personal_bests
        .get(&result.pb_key)
        .map(|pb| pb.wpm)
        .unwrap_or(-1.0);
    let is_pb = result.wpm > prev && result.wpm > 0.0;

    if is_pb {
        stats.personal_bests.insert(
            result.pb_key.clone(),
            PersonalBest {
                wpm: result.wpm,
                accuracy: result.accuracy,
            },
        );
    }

    stats.history.push(HistoryEntry {
        mode: result.config_label.clone(),
        wpm: result.wpm,
        raw_wpm: result.raw_wpm,
        accuracy: result.accuracy,
        consistency: result.consistency,
    });
    if stats.history.len() > 100 {
        let drain_count = stats.history.len() - 100;
        stats.history.drain(0..drain_count);
    }

    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&stats) {
        let _ = fs::write(&target, json);
    }
    is_pb
}
