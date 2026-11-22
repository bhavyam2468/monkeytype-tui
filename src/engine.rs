use std::time::Duration;

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::words::{generate_words, pick_quote};

pub const TIME_OPTIONS: &[u64] = &[15, 30, 60, 120];
pub const WORD_OPTIONS: &[usize] = &[10, 25, 50, 100];
const MAX_EXTRA_CHARS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Time,
    Words,
    Quote,
}

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub mode: Mode,
    pub duration_secs: u64,
    pub word_count: usize,
    pub punctuation: bool,
    pub numbers: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Time,
            duration_secs: 30,
            word_count: 25,
            punctuation: false,
            numbers: false,
        }
    }
}

impl TestConfig {
    pub fn label(&self) -> String {
        let base = match self.mode {
            Mode::Time => format!("time {}s", self.duration_secs),
            Mode::Words => format!("words {}", self.word_count),
            Mode::Quote => "quote".to_string(),
        };
        let mut extras = Vec::new();
        if self.punctuation && self.mode != Mode::Quote {
            extras.push("punct");
        }
        if self.numbers && self.mode != Mode::Quote {
            extras.push("nums");
        }
        if extras.is_empty() {
            base
        } else {
            format!("{base} ({})", extras.join(", "))
        }
    }

    pub fn pb_key(&self) -> String {
        match self.mode {
            Mode::Time => format!("time:{}", self.duration_secs),
            Mode::Words => format!("words:{}", self.word_count),
            Mode::Quote => "quote".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecondSample {
    pub second: u64,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub errors: u32,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
    pub elapsed_secs: f64,
    pub correct_chars: usize,
    pub incorrect_chars: usize,
    pub extra_chars: usize,
    pub missed_chars: usize,
    pub max_combo: u32,
    pub samples: Vec<SecondSample>,
    pub config_label: String,
    pub pb_key: String,
    #[allow(dead_code)]
    pub quote_author: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharOutcome {
    Ignored,
    Correct,
    Error,
    WordCompleted { clean: bool },
}

pub struct TypingEngine {
    pub config: TestConfig,
    rng: StdRng,
    pub words: Vec<String>,
    pub typed_words: Vec<String>,
    pub current_word_idx: usize,
    pub quote_author: String,

    pub started: bool,
    pub finished: bool,
    elapsed_secs: f64,

    pub total_keystrokes: usize,
    pub correct_keystrokes: usize,
    pub combo: u32,
    pub max_combo: u32,

    errors_since_sample: u32,
    last_sampled_second: u64,
    pub samples: Vec<SecondSample>,
}

impl TypingEngine {
    pub fn new(config: TestConfig) -> Self {
        let mut engine = Self {
            config,
            rng: StdRng::from_entropy(),
            words: Vec::new(),
            typed_words: Vec::new(),
            current_word_idx: 0,
            quote_author: String::new(),
            started: false,
            finished: false,
            elapsed_secs: 0.0,
            total_keystrokes: 0,
            correct_keystrokes: 0,
            combo: 0,
            max_combo: 0,
            errors_since_sample: 0,
            last_sampled_second: 0,
            samples: Vec::new(),
        };
        engine.reset(true);
        engine
    }

    pub fn reset(&mut self, new_words: bool) {
        if new_words || self.words.is_empty() {
            match self.config.mode {
                Mode::Quote => {
                    let (w, author) = pick_quote(&mut self.rng);
                    self.words = w;
                    self.quote_author = author;
                }
                Mode::Words => {
                    self.words = generate_words(
                        self.config.word_count,
                        self.config.punctuation,
                        self.config.numbers,
                        &mut self.rng,
                    );
                    self.quote_author.clear();
                }
                Mode::Time => {
                    self.words = generate_words(
                        80,
                        self.config.punctuation,
                        self.config.numbers,
                        &mut self.rng,
                    );
                    self.quote_author.clear();
                }
            }
        }
        self.typed_words = vec![String::new(); self.words.len()];
        self.current_word_idx = 0;
        self.started = false;
        self.finished = false;
        self.elapsed_secs = 0.0;
        self.total_keystrokes = 0;
        self.correct_keystrokes = 0;
        self.combo = 0;
        self.max_combo = 0;
        self.errors_since_sample = 0;
        self.last_sampled_second = 0;
        self.samples.clear();
    }

    fn ensure_time_words(&mut self) {
        if self.config.mode != Mode::Time {
            return;
        }
        if self.words.len().saturating_sub(self.current_word_idx) < 30 {
            let more = generate_words(
                50,
                self.config.punctuation,
                self.config.numbers,
                &mut self.rng,
            );
            let extra_len = more.len();
            self.words.extend(more);
            self.typed_words.extend(std::iter::repeat_n(String::new(), extra_len));
        }
    }

    #[allow(dead_code)]
    pub fn elapsed(&self) -> f64 {
        self.elapsed_secs
    }

    pub fn remaining_secs(&self) -> f64 {
        if self.config.mode != Mode::Time {
            return 0.0;
        }
        (self.config.duration_secs as f64 - self.elapsed_secs).max(0.0)
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        if !self.started || self.finished {
            return false;
        }
        self.elapsed_secs += dt.as_secs_f64();

        let whole_sec = self.elapsed_secs.floor() as u64;
        while whole_sec > self.last_sampled_second {
            self.last_sampled_second += 1;
            let (wpm, raw_wpm) = self.compute_wpm_at(self.last_sampled_second as f64);
            self.samples.push(SecondSample {
                second: self.last_sampled_second,
                wpm,
                raw_wpm,
                errors: self.errors_since_sample,
            });
            self.errors_since_sample = 0;
        }

        if self.config.mode == Mode::Time && self.elapsed_secs >= self.config.duration_secs as f64 {
            self.elapsed_secs = self.config.duration_secs as f64;
            self.finish();
            return true;
        }
        false
    }

    pub fn handle_char(&mut self, ch: char) -> CharOutcome {
        if self.finished {
            return CharOutcome::Ignored;
        }

        if ch == ' ' {
            if !self.started || self.typed_words[self.current_word_idx].is_empty() {
                return CharOutcome::Ignored;
            }
            self.total_keystrokes += 1;
            let clean = self.typed_words[self.current_word_idx] == self.words[self.current_word_idx];
            if clean {
                self.correct_keystrokes += 1;
                self.combo += 1;
                self.max_combo = self.max_combo.max(self.combo);
            } else {
                self.errors_since_sample += 1;
                self.combo = 0;
            }

            if self.current_word_idx + 1 >= self.words.len() {
                self.finish();
            } else {
                self.current_word_idx += 1;
                self.ensure_time_words();
            }
            return CharOutcome::WordCompleted { clean };
        }

        self.started = true;
        let target = &self.words[self.current_word_idx];
        let typed = &mut self.typed_words[self.current_word_idx];
        if typed.chars().count() >= target.chars().count() + MAX_EXTRA_CHARS {
            return CharOutcome::Ignored;
        }

        let char_idx = typed.chars().count();
        self.total_keystrokes += 1;
        let is_correct = target.chars().nth(char_idx) == Some(ch);
        if is_correct {
            self.correct_keystrokes += 1;
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
        } else {
            self.errors_since_sample += 1;
            self.combo = 0;
        }

        typed.push(ch);

        if matches!(self.config.mode, Mode::Words | Mode::Quote)
            && self.current_word_idx + 1 == self.words.len()
            && self.typed_words[self.current_word_idx] == self.words[self.current_word_idx]
        {
            self.finish();
        }

        if is_correct {
            CharOutcome::Correct
        } else {
            CharOutcome::Error
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.finished || !self.started {
            return;
        }
        if !self.typed_words[self.current_word_idx].is_empty() {
            self.typed_words[self.current_word_idx].pop();
            return;
        }
        if self.current_word_idx > 0 {
            let prev = self.current_word_idx - 1;
            if self.typed_words[prev] != self.words[prev] {
                self.current_word_idx = prev;
            }
        }
    }

    pub fn handle_delete_word(&mut self) {
        if self.finished || !self.started {
            return;
        }
        if !self.typed_words[self.current_word_idx].is_empty() {
            self.typed_words[self.current_word_idx].clear();
            return;
        }
        if self.current_word_idx > 0 {
            let prev = self.current_word_idx - 1;
            if self.typed_words[prev] != self.words[prev] {
                self.current_word_idx = prev;
                self.typed_words[prev].clear();
            }
        }
    }

    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.elapsed_secs > 0.0 {
            let last_sec = (self.elapsed_secs.round() as u64).max(1);
            if self.samples.last().map(|s| s.second).unwrap_or(0) < last_sec {
                let (wpm, raw_wpm) = self.compute_wpm_at(self.elapsed_secs);
                self.samples.push(SecondSample {
                    second: last_sec,
                    wpm,
                    raw_wpm,
                    errors: self.errors_since_sample,
                });
                self.errors_since_sample = 0;
            }
        }
    }

    fn char_breakdown(&self) -> (usize, usize, usize, usize, usize) {
        let mut correct = 0usize;
        let mut incorrect = 0usize;
        let mut extra = 0usize;
        let mut missed = 0usize;
        let mut net_chars = 0usize;

        let limit = if self.started {
            self.current_word_idx + 1
        } else {
            0
        };

        for i in 0..limit {
            let target: Vec<char> = self.words[i].chars().collect();
            let typed: Vec<char> = self.typed_words[i].chars().collect();
            let is_completed = i < self.current_word_idx || self.finished;

            if typed.is_empty() && !is_completed {
                continue;
            }

            let mut word_correct = true;
            for (j, &ch) in typed.iter().enumerate() {
                if j < target.len() {
                    if ch == target[j] {
                        correct += 1;
                    } else {
                        incorrect += 1;
                        word_correct = false;
                    }
                } else {
                    extra += 1;
                    word_correct = false;
                }
            }

            if is_completed && typed.len() < target.len() {
                missed += target.len() - typed.len();
                word_correct = false;
            }

            if i < self.current_word_idx {
                if word_correct {
                    net_chars += target.len() + 1;
                }
            } else if word_correct {
                net_chars += typed.len();
            }
        }

        (correct, incorrect, extra, missed, net_chars)
    }

    fn compute_wpm_at(&self, elapsed: f64) -> (f64, f64) {
        if elapsed <= 0.05 {
            return (0.0, 0.0);
        }
        let mins = elapsed / 60.0;
        let (_, _, _, _, net_chars) = self.char_breakdown();
        let typed_chars: usize = (0..=self.current_word_idx)
            .map(|i| self.typed_words[i].chars().count())
            .sum::<usize>()
            + self.current_word_idx;
        ((net_chars as f64 / 5.0) / mins, (typed_chars as f64 / 5.0) / mins)
    }

    pub fn live_wpm(&self) -> f64 {
        self.compute_wpm_at(self.elapsed_secs).0
    }

    pub fn accuracy(&self) -> f64 {
        if self.total_keystrokes == 0 {
            100.0
        } else {
            (self.correct_keystrokes as f64 / self.total_keystrokes as f64) * 100.0
        }
    }

    pub fn consistency(&self) -> f64 {
        let vals: Vec<f64> = self
            .samples
            .iter()
            .map(|s| s.raw_wpm)
            .filter(|&v| v > 0.0)
            .collect();
        if vals.is_empty() {
            return 0.0;
        }
        if vals.len() == 1 {
            return 100.0;
        }
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        if mean <= 0.0 {
            return 0.0;
        }
        let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64;
        let cv = var.sqrt() / mean;
        ((1.0 - cv) * 100.0).clamp(0.0, 100.0)
    }

    pub fn result(&self) -> TestResult {
        let (wpm, raw_wpm) = self.compute_wpm_at(self.elapsed_secs);
        let (correct, incorrect, extra, missed, _) = self.char_breakdown();
        TestResult {
            wpm: (wpm * 10.0).round() / 10.0,
            raw_wpm: (raw_wpm * 10.0).round() / 10.0,
            accuracy: (self.accuracy() * 10.0).round() / 10.0,
            consistency: (self.consistency() * 10.0).round() / 10.0,
            elapsed_secs: (self.elapsed_secs * 100.0).round() / 100.0,
            correct_chars: correct,
            incorrect_chars: incorrect,
            extra_chars: extra,
            missed_chars: missed,
            max_combo: self.max_combo,
            samples: self.samples.clone(),
            config_label: self.config.label(),
            pb_key: self.config.pb_key(),
            quote_author: self.quote_author.clone(),
        }
    }
}
