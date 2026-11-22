mod effects;
mod engine;
mod storage;
mod themes;
mod ui;
mod words;

use std::io;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::effects::CursorEffects;
use crate::engine::{
    CharOutcome, Mode, TestConfig, TestResult, TypingEngine, TIME_OPTIONS, WORD_OPTIONS,
};
use crate::storage::{
    load_settings, load_stats, record_result, save_settings, AnimSettings,
};
use crate::themes::{
    build_theme_list, dms_colors_mtime, find_theme_index, load_dms_theme, Theme,
};
use crate::ui::{compute_caret_screen_pos, config_item_labels, draw, AppViewState};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Time,
    Words,
    Quote,
}

#[derive(Parser, Debug)]
#[command(
    name = "monkeytype-tui",
    version,
    about = "A fast, minimalist terminal typing test with cursor animations and DMS theme sync"
)]
struct Cli {
    #[arg(short, long, value_enum, default_value_t = CliMode::Time)]
    mode: CliMode,
    #[arg(short, long, default_value_t = 30)]
    time: u64,
    #[arg(short, long, default_value_t = 25)]
    words: usize,
    #[arg(short, long)]
    punctuation: bool,
    #[arg(short, long)]
    numbers: bool,
    #[arg(long, default_value = "dms_sync")]
    theme: String,
    #[arg(long)]
    stats: bool,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if cli.stats {
        let stats = load_stats(None);
        println!("Tests completed: {}", stats.tests_completed);
        if !stats.personal_bests.is_empty() {
            println!("\nPersonal Bests:");
            let mut entries: Vec<_> = stats.personal_bests.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (k, v) in entries {
                println!("  {:<12} {:>6.1} wpm  ({:.1}% acc)", k, v.wpm, v.accuracy);
            }
        }
        return Ok(());
    }

    let config = TestConfig {
        mode: match cli.mode {
            CliMode::Time => Mode::Time,
            CliMode::Words => Mode::Words,
            CliMode::Quote => Mode::Quote,
        },
        duration_secs: if TIME_OPTIONS.contains(&cli.time) {
            cli.time
        } else {
            30
        },
        word_count: if WORD_OPTIONS.contains(&cli.words) {
            cli.words
        } else {
            25
        },
        punctuation: cli.punctuation,
        numbers: cli.numbers,
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let themes = build_theme_list();
    let initial_theme_idx = find_theme_index(&themes, &cli.theme);
    let res = run_app(&mut terminal, config, themes, initial_theme_idx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: TestConfig,
    mut themes: Vec<Theme>,
    mut theme_idx: usize,
) -> io::Result<()> {
    let mut engine = TypingEngine::new(config);
    let mut effects = CursorEffects::new();
    let mut stats = load_stats(None);
    let mut settings = load_settings();

    let mut last_dms_mtime = dms_colors_mtime();
    let mut last_dms_check = Instant::now();

    let mut fade_from: Option<Theme> = None;
    let mut fade_started: Option<Instant> = None;
    let mut active_rendered_theme = themes[theme_idx].clone();

    let mut config_mode = false;
    let mut config_cursor = 0usize;
    let mut settings_open = false;
    let mut settings_cursor = 0usize;

    let mut last_result: Option<TestResult> = None;
    let mut last_is_pb = false;
    let mut result_shown_at: Option<Instant> = None;
    let mut recorded_current = false;

    let app_start = Instant::now();
    let mut last_frame = Instant::now();

    let start_theme_fade = |from_theme: &Theme,
                            enabled: bool,
                            fade_from: &mut Option<Theme>,
                            fade_started: &mut Option<Instant>| {
        if enabled {
            *fade_from = Some(from_theme.clone());
            *fade_started = Some(Instant::now());
        } else {
            *fade_from = None;
            *fade_started = None;
        }
    };

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame);
        last_frame = now;

        let size = terminal.size()?;
        let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);

        // Live-sync with DMS theme changes every ~1s
        if now.duration_since(last_dms_check) >= Duration::from_secs(1) {
            last_dms_check = now;
            let cur_mtime = dms_colors_mtime();
            if cur_mtime != last_dms_mtime {
                last_dms_mtime = cur_mtime;
                if let Some(dms_theme) = load_dms_theme() {
                    let was_on_dms = themes[theme_idx].name == "dms_sync";
                    if was_on_dms {
                        start_theme_fade(
                            &active_rendered_theme,
                            settings.theme_fade,
                            &mut fade_from,
                            &mut fade_started,
                        );
                    }
                    if let Some(pos) = themes.iter().position(|t| t.name == "dms_sync") {
                        themes[pos] = dms_theme;
                    } else {
                        themes.insert(0, dms_theme);
                        theme_idx += 1;
                    }
                }
            }
        }

        // Compute cross-faded theme if transitioning
        active_rendered_theme = if let (Some(ref from), Some(started)) = (&fade_from, fade_started)
        {
            let t = (started.elapsed().as_secs_f32() / 0.26).clamp(0.0, 1.0);
            if t >= 1.0 {
                fade_from = None;
                fade_started = None;
                themes[theme_idx].clone()
            } else {
                from.lerp(&themes[theme_idx], t)
            }
        } else {
            themes[theme_idx].clone()
        };

        if engine.tick(dt) || (engine.finished && !recorded_current) {
            if engine.finished && !recorded_current {
                let res = engine.result();
                last_is_pb = record_result(&res, None);
                stats = load_stats(None);
                last_result = Some(res);
                result_shown_at = Some(Instant::now());
                recorded_current = true;
                config_mode = false;
                if last_is_pb && settings.combo_aura {
                    effects.spawn_pb_confetti(area.width, area.height);
                }
            }
        }

        let nudge = effects.typo_nudge_offset(settings.typo_nudge);
        let (cx, cy) = compute_caret_screen_pos(&engine, area, nudge);
        effects.update(cx, cy, area.width, area.height, dt.as_secs_f32(), &settings);

        terminal.draw(|frame| {
            let mut view = AppViewState {
                engine: &engine,
                theme: &active_rendered_theme,
                effects: &mut effects,
                stats: &stats,
                settings: &settings,
                config_mode,
                config_cursor,
                settings_open,
                settings_cursor,
                last_result: last_result.as_ref(),
                last_is_pb,
                result_shown_at,
                anim_clock: now.duration_since(app_start).as_secs_f64(),
            };
            draw(frame, &mut view);
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                // Ctrl+C quits anywhere
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }

                // Global F3 -> toggle Settings modal
                if key.code == KeyCode::F(3) {
                    settings_open = !settings_open;
                    continue;
                }

                // Global F2 or Alt+T -> cycle theme
                if key.code == KeyCode::F(2)
                    || (key.modifiers.contains(KeyModifiers::ALT)
                        && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T')))
                {
                    start_theme_fade(
                        &active_rendered_theme,
                        settings.theme_fade,
                        &mut fade_from,
                        &mut fade_started,
                    );
                    theme_idx = (theme_idx + 1) % themes.len();
                    continue;
                }

                // Settings modal input
                if settings_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Tab | KeyCode::Char('q') | KeyCode::Char('Q') => {
                            settings_open = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            settings_cursor =
                                (settings_cursor + AnimSettings::COUNT - 1) % AnimSettings::COUNT;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            settings_cursor = (settings_cursor + 1) % AnimSettings::COUNT;
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            settings.toggle(settings_cursor);
                            save_settings(&settings);
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            settings.set_all_animations(true);
                            save_settings(&settings);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            settings.set_all_animations(false);
                            save_settings(&settings);
                        }
                        _ => {}
                    }
                    continue;
                }

                // Global Ctrl+K -> cycle software caret style
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
                    effects.caret_style = effects.caret_style.next();
                    continue;
                }

                // Global Ctrl+L -> toggle software RGB trail
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
                    settings.soft_trail = !settings.soft_trail;
                    save_settings(&settings);
                    continue;
                }

                // Global Ctrl+R -> repeat same words
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
                    engine.reset(false);
                    effects.reset_positions(settings.restart_scramble);
                    last_result = None;
                    last_is_pb = false;
                    result_shown_at = None;
                    recorded_current = false;
                    config_mode = false;
                    continue;
                }

                if engine.finished {
                    match key.code {
                        KeyCode::Tab | KeyCode::Enter => {
                            engine.reset(true);
                            effects.reset_positions(settings.restart_scramble);
                            last_result = None;
                            last_is_pb = false;
                            result_shown_at = None;
                            recorded_current = false;
                        }
                        KeyCode::Esc => break,
                        _ => {}
                    }
                    continue;
                }

                if config_mode {
                    let labels = config_item_labels(&engine, &themes[theme_idx], &settings);
                    let len = labels.len();
                    config_cursor = config_cursor.min(len.saturating_sub(1));
                    match key.code {
                        KeyCode::Esc | KeyCode::Tab => config_mode = false,
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Left | KeyCode::Char('h') => {
                            config_cursor = (config_cursor + len - 1) % len;
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            config_cursor = (config_cursor + 1) % len;
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            let label = labels[config_cursor].0.as_str();
                            if label == "settings" {
                                settings_open = true;
                                config_mode = false;
                            } else {
                                if label.starts_with("theme:") {
                                    start_theme_fade(
                                        &active_rendered_theme,
                                        settings.theme_fade,
                                        &mut fade_from,
                                        &mut fade_started,
                                    );
                                }
                                apply_config_action(
                                    label,
                                    &mut engine,
                                    &mut effects,
                                    &settings,
                                    &mut theme_idx,
                                    themes.len(),
                                );
                                last_result = None;
                                last_is_pb = false;
                                recorded_current = false;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Typing mode
                match key.code {
                    KeyCode::Tab => {
                        engine.reset(true);
                        effects.reset_positions(settings.restart_scramble);
                        last_result = None;
                        last_is_pb = false;
                        recorded_current = false;
                    }
                    KeyCode::Esc => {
                        config_mode = true;
                        config_cursor = 0;
                    }
                    KeyCode::Backspace => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            || key.modifiers.contains(KeyModifiers::ALT)
                        {
                            engine.handle_delete_word();
                        } else {
                            engine.handle_backspace();
                        }
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        engine.handle_delete_word();
                    }
                    KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let (pre_x, pre_y) = compute_caret_screen_pos(&engine, area, nudge);
                        match engine.handle_char(ch) {
                            CharOutcome::Correct => {
                                effects.trigger_keystroke(pre_x, pre_y, false, false, &settings);
                            }
                            CharOutcome::Error => {
                                effects.trigger_keystroke(pre_x, pre_y, true, false, &settings);
                            }
                            CharOutcome::WordCompleted { clean } => {
                                effects.trigger_keystroke(pre_x, pre_y, !clean, clean, &settings);
                            }
                            CharOutcome::Ignored => {}
                        }
                        if engine.finished && !recorded_current {
                            let res = engine.result();
                            last_is_pb = record_result(&res, None);
                            stats = load_stats(None);
                            last_result = Some(res);
                            result_shown_at = Some(Instant::now());
                            recorded_current = true;
                            if last_is_pb && settings.combo_aura {
                                effects.spawn_pb_confetti(area.width, area.height);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn apply_config_action(
    label: &str,
    engine: &mut TypingEngine,
    effects: &mut CursorEffects,
    settings: &AnimSettings,
    theme_idx: &mut usize,
    theme_count: usize,
) {
    match label {
        "@ punct" => {
            engine.config.punctuation = !engine.config.punctuation;
            engine.reset(true);
            effects.reset_positions(settings.restart_scramble);
        }
        "# nums" => {
            engine.config.numbers = !engine.config.numbers;
            engine.reset(true);
            effects.reset_positions(settings.restart_scramble);
        }
        "time" => {
            engine.config.mode = Mode::Time;
            engine.reset(true);
            effects.reset_positions(settings.restart_scramble);
        }
        "words" => {
            engine.config.mode = Mode::Words;
            engine.reset(true);
            effects.reset_positions(settings.restart_scramble);
        }
        "quote" => {
            engine.config.mode = Mode::Quote;
            engine.reset(true);
            effects.reset_positions(settings.restart_scramble);
        }
        s if s.starts_with("theme:") => {
            *theme_idx = (*theme_idx + 1) % theme_count;
        }
        num_str => {
            if let Ok(val) = num_str.parse::<u64>() {
                match engine.config.mode {
                    Mode::Time => engine.config.duration_secs = val,
                    Mode::Words => engine.config.word_count = val as usize,
                    Mode::Quote => {}
                }
                engine.reset(true);
                effects.reset_positions(settings.restart_scramble);
            }
        }
    }
}
