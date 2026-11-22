use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use rand::Rng;
use ratatui::style::Color;

use crate::storage::AnimSettings;
use crate::themes::{lerp_color, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretStyle {
    Block,
    Beam,
    Underline,
}

impl CaretStyle {
    pub fn next(self) -> Self {
        match self {
            Self::Block => Self::Beam,
            Self::Beam => Self::Underline,
            Self::Underline => Self::Block,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Beam => "beam",
            Self::Underline => "under",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrailCell {
    pub x: u16,
    pub y: u16,
    pub born: Instant,
}

#[derive(Debug, Clone)]
pub struct Spark {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub ch: char,
    pub born: Instant,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct KeyFlash {
    pub born: Instant,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct ConfettiPiece {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub ch: char,
    pub accent_mix: f32,
}

pub struct CursorEffects {
    pub caret_style: CaretStyle,
    pub trail: VecDeque<TrailCell>,
    pub sparks: Vec<Spark>,
    pub confetti: Vec<ConfettiPiece>,
    pub key_flashes: HashMap<char, KeyFlash>,

    cur_fx: f32,
    cur_fy: f32,
    initialized: bool,

    pub error_flash_until: Option<Instant>,
    pub typo_nudge_at: Option<Instant>,
    pub scramble_started_at: Option<Instant>,
    pub line_shift_at: Option<Instant>,
    pub last_start_line: usize,

    pub wave_energy: f32,
    pub start_instant: Instant,
}

impl CursorEffects {
    pub fn new() -> Self {
        Self {
            caret_style: CaretStyle::Block,
            trail: VecDeque::with_capacity(64),
            sparks: Vec::new(),
            confetti: Vec::new(),
            key_flashes: HashMap::new(),
            cur_fx: 0.0,
            cur_fy: 0.0,
            initialized: false,
            error_flash_until: None,
            typo_nudge_at: None,
            scramble_started_at: None,
            line_shift_at: None,
            last_start_line: 0,
            wave_energy: 0.0,
            start_instant: Instant::now(),
        }
    }

    pub fn reset_positions(&mut self, trigger_scramble: bool) {
        self.trail.clear();
        self.sparks.clear();
        self.confetti.clear();
        self.initialized = false;
        self.error_flash_until = None;
        self.typo_nudge_at = None;
        self.line_shift_at = None;
        self.last_start_line = 0;
        self.wave_energy = 0.0;
        if trigger_scramble {
            self.scramble_started_at = Some(Instant::now());
        }
    }

    pub fn trigger_keystroke(
        &mut self,
        ch: char,
        x: u16,
        y: u16,
        is_error: bool,
        word_done: bool,
        settings: &AnimSettings,
    ) {
        let now = Instant::now();
        self.wave_energy = (self.wave_energy + 0.45).min(1.0);

        if settings.mini_keyboard {
            self.key_flashes.insert(
                ch.to_ascii_lowercase(),
                KeyFlash {
                    born: now,
                    is_error,
                },
            );
        }

        if is_error {
            self.error_flash_until = Some(now + Duration::from_millis(160));
            if settings.typo_nudge {
                self.typo_nudge_at = Some(now);
            }
            if settings.sparks {
                for &(vx, vy, sym) in &[(-0.4, -0.35, '×'), (0.4, -0.35, '×')] {
                    self.sparks.push(Spark {
                        x: x as f32,
                        y: y as f32,
                        vx,
                        vy,
                        ch: sym,
                        born: now,
                        is_error: true,
                    });
                }
            }
        } else if word_done && settings.sparks {
            for &(vx, vy, sym) in &[(-0.5, -0.4, '✦'), (0.0, -0.5, '⋆'), (0.5, -0.4, '·')] {
                self.sparks.push(Spark {
                    x: x as f32,
                    y: y as f32,
                    vx,
                    vy,
                    ch: sym,
                    born: now,
                    is_error: false,
                });
            }
        }
    }

    pub fn spawn_pb_confetti(&mut self, width: u16, height: u16) {
        self.confetti.clear();
        let mut rng = rand::thread_rng();
        let glyphs = ['✦', '⋆', '·', '*', '•'];
        let count = (width as usize / 3).clamp(18, 42);
        for _ in 0..count {
            self.confetti.push(ConfettiPiece {
                x: rng.gen_range(2.0..(width.saturating_sub(2).max(4) as f32)),
                y: rng.gen_range(5.0..(height.saturating_sub(2).max(8) as f32)),
                vx: rng.gen_range(-2.2..2.2),
                vy: rng.gen_range(2.5..6.5),
                ch: glyphs[rng.gen_range(0..glyphs.len())],
                accent_mix: rng.gen_range(0.35..1.0),
            });
        }
    }

    pub fn observe_start_line(&mut self, start_line: usize, enabled: bool) {
        if start_line != self.last_start_line {
            if enabled && start_line > self.last_start_line {
                self.line_shift_at = Some(Instant::now());
            }
            self.last_start_line = start_line;
        }
    }

    pub fn line_fade_progress(&self) -> f32 {
        match self.line_shift_at {
            Some(t) => (t.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0),
            None => 1.0,
        }
    }

    pub fn typo_nudge_offset(&self, enabled: bool) -> i16 {
        if !enabled {
            return 0;
        }
        let Some(t) = self.typo_nudge_at else {
            return 0;
        };
        let ms = t.elapsed().as_millis();
        if ms < 28 {
            -1
        } else if ms < 56 {
            1
        } else {
            0
        }
    }

    pub fn scramble_progress(&self, enabled: bool) -> f32 {
        if !enabled {
            return 1.0;
        }
        match self.scramble_started_at {
            Some(t) => (t.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0),
            None => 1.0,
        }
    }

    pub fn update(
        &mut self,
        target_x: u16,
        target_y: u16,
        area_w: u16,
        area_h: u16,
        dt_secs: f32,
        settings: &AnimSettings,
    ) {
        let now = Instant::now();
        self.wave_energy = (self.wave_energy - dt_secs * 2.2).max(0.0);

        if !self.initialized {
            self.cur_fx = target_x as f32;
            self.cur_fy = target_y as f32;
            self.initialized = true;
        } else {
            let tx = target_x as f32;
            let ty = target_y as f32;
            let dist = ((tx - self.cur_fx).powi(2) + (ty - self.cur_fy).powi(2)).sqrt();

            if dist > 0.1 {
                let steps = (dist.ceil() as usize).clamp(1, 16);
                for s in 0..steps {
                    let frac = (s + 1) as f32 / steps as f32;
                    let ix = (self.cur_fx + (tx - self.cur_fx) * frac).round() as u16;
                    let iy = (self.cur_fy + (ty - self.cur_fy) * frac).round() as u16;
                    if settings.soft_trail {
                        self.trail.push_back(TrailCell {
                            x: ix,
                            y: iy,
                            born: now,
                        });
                    }
                }
            }

            let speed = (dt_secs * 28.0).clamp(0.15, 1.0);
            self.cur_fx += (tx - self.cur_fx) * speed;
            self.cur_fy += (ty - self.cur_fy) * speed;
            if (tx - self.cur_fx).abs() < 0.05 && (ty - self.cur_fy).abs() < 0.05 {
                self.cur_fx = tx;
                self.cur_fy = ty;
            }
        }

        // Expire software trail cells
        let trail_ttl = Duration::from_millis(300);
        while let Some(front) = self.trail.front() {
            if now.duration_since(front.born) > trail_ttl {
                self.trail.pop_front();
            } else {
                break;
            }
        }

        // Update & expire sparks
        let spark_ttl = Duration::from_millis(380);
        for spark in &mut self.sparks {
            spark.x += spark.vx * dt_secs * 18.0;
            spark.y += spark.vy * dt_secs * 12.0;
        }
        self.sparks
            .retain(|s| now.duration_since(s.born) <= spark_ttl);

        // Expire keyboard flashes
        let key_ttl = Duration::from_millis(360);
        self.key_flashes
            .retain(|_, f| now.duration_since(f.born) <= key_ttl);

        // Animate PB confetti rain
        if settings.combo_aura && !self.confetti.is_empty() {
            let max_x = area_w.saturating_sub(2).max(4) as f32;
            let max_y = area_h.saturating_sub(2).max(8) as f32;
            for p in &mut self.confetti {
                p.x += p.vx * dt_secs;
                p.y += p.vy * dt_secs;
                if p.y >= max_y {
                    p.y = 5.5;
                }
                if p.x < 2.0 {
                    p.x = max_x - 1.0;
                } else if p.x >= max_x {
                    p.x = 2.0;
                }
            }
        }
    }

    pub fn trail_bg_at(&self, x: u16, y: u16, theme: &Theme, enabled: bool) -> Option<Color> {
        if !enabled {
            return None;
        }
        let now = Instant::now();
        let mut best_strength: f32 = 0.0;
        for cell in &self.trail {
            if cell.x == x && cell.y == y {
                let age = now.duration_since(cell.born).as_secs_f32() / 0.30;
                let strength = (1.0 - age).clamp(0.0, 1.0);
                if strength > best_strength {
                    best_strength = strength;
                }
            }
        }
        if best_strength > 0.03 {
            let base = if self
                .error_flash_until
                .map(|t| now < t)
                .unwrap_or(false)
            {
                theme.error
            } else {
                theme.accent
            };
            Some(lerp_color(theme.bg, base, best_strength * 0.55))
        } else {
            None
        }
    }

    pub fn key_color(&self, key: char, theme: &Theme) -> Color {
        if let Some(flash) = self.key_flashes.get(&key.to_ascii_lowercase()) {
            let age = (flash.born.elapsed().as_secs_f32() / 0.36).clamp(0.0, 1.0);
            let peak = if flash.is_error {
                theme.error
            } else {
                theme.accent
            };
            lerp_color(peak, theme.muted, age)
        } else {
            theme.muted
        }
    }

    pub fn caret_color(&self, theme: &Theme) -> Color {
        let now = Instant::now();
        if self.error_flash_until.map(|t| now < t).unwrap_or(false) {
            return theme.error;
        }
        let elapsed = now.duration_since(self.start_instant).as_secs_f32();
        let pulse = (elapsed * 4.2).sin() * 0.5 + 0.5;
        lerp_color(theme.caret_bg, theme.accent, pulse * 0.35)
    }
}

/// Deterministic scramble glyph for the restart decode animation.
pub fn scramble_glyph(word_idx: usize, char_idx: usize, anim_clock: f64) -> char {
    const GLYPHS: &[char] = &['#', '@', '%', '&', '*', '+', '=', '~', '?', '/', '!'];
    let tick = (anim_clock * 32.0) as usize;
    GLYPHS[(word_idx * 17 + char_idx * 31 + tick) % GLYPHS.len()]
}

/// Returns a 3-line animated ASCII monkey mascot reacting to typing state & speed.
pub fn monkey_mascot(
    elapsed_secs: f64,
    started: bool,
    finished: bool,
    wpm: f64,
    combo: u32,
    is_pb: bool,
) -> [&'static str; 3] {
    let tick = (elapsed_secs * 6.0) as usize;
    if finished {
        if is_pb {
            return match tick % 2 {
                0 => ["  \\o/  *PB*", " ( ^.^ )  ", "  /===\\   "],
                _ => [" *\\o/* PB!", " ( ^o^ )  ", "  /===\\   "],
            };
        }
        return ["  (o.o)   ", "  /( )\\   ", "  [___]   "];
    }

    if !started {
        if tick % 18 == 0 {
            return ["  (-.-)   ", "  /( )\\   ", "  [===]   "];
        }
        return ["  (o.o)   ", "  /( )\\   ", "  [===]   "];
    }

    let fast_tick = if wpm > 80.0 {
        (elapsed_secs * 14.0) as usize
    } else {
        (elapsed_secs * 8.0) as usize
    };

    if combo >= 20 {
        return match fast_tick % 3 {
            0 => [" ~(>.<)~* ", " d(   )p  ", "  [===]   "],
            1 => [" *(^.^)*~ ", " q(   )b  ", "  [===]   "],
            _ => [" ~(o.o)~* ", " /(   )\\  ", "  [===]   "],
        };
    }

    match fast_tick % 2 {
        0 => ["  (o.o)   ", " d(   )p  ", "  [===]   "],
        _ => ["  (o.o)   ", " q(   )b  ", "  [===]   "],
    }
}

/// Animated ASCII keystroke wave bar.
pub fn keystroke_wave(elapsed_secs: f64, energy: f32, width: usize) -> String {
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut out = String::with_capacity(width);
    for i in 0..width {
        let phase = elapsed_secs * 7.5 + (i as f64) * 0.45;
        let wave = ((phase.sin() * 0.5 + 0.5) as f32) * (0.15 + energy * 0.85);
        let idx = ((wave * (bars.len() - 1) as f32).round() as usize).min(bars.len() - 1);
        out.push(bars[idx]);
    }
    out
}
