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
