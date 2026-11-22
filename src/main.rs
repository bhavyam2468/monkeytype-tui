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
use crate::engine::{CharOutcome, Mode, TestConfig, TestResult, TypingEngine, TIME_OPTIONS, WORD_OPTIONS};
use crate::storage::{load_stats, record_result};
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
#[command(name = "monkeytype-tui", version, about = "A fast, minimalist terminal typing test with cursor animations and DMS theme sync")]
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
        duration_secs: if TIME_OPTIONS.contains(&cli.time) { cli.time } else { 30 },
        word_count: if WORD_OPTIONS.contains(&cli.words) { cli.words } else { 25 },
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
    let mut last_dms_mtime = dms_colors_mtime();
    let mut last_dms_check = Instant::now();

    let mut config_mode = false;
    let mut config_cursor = 0usize;
    let mut last_result: Option<TestResult> = None;
    let mut last_is_pb = false;
    let mut result_shown_at: Option<Instant> = None;
    let mut recorded_current = false;

    let app_start = Instant::now();
    let mut last_frame = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame);
        last_frame = now;

        // Live-sync with DMS theme changes every ~1s
        if now.duration_since(last_dms_check) >= Duration::from_secs(1) {
            last_dms_check = now;
            let cur_mtime = dms_colors_mtime();
            if cur_mtime != last_dms_mtime {
                last_dms_mtime = cur_mtime;
                if let Some(dms_theme) = load_dms_theme() {
                    if let Some(pos) = themes.iter().position(|t| t.name == "dms_sync") {
                        themes[pos] = dms_theme;
                    } else {
                        themes.insert(0, dms_theme);
                        theme_idx += 1;
                    }
                }
            }
        }

        if engine.tick(dt) || (engine.finished && !recorded_current) {
            if engine.finished && !recorded_current {
                let res = engine.result();
                last_is_pb = record_result(&res, None);
                stats = load_stats(None);
                last_result = Some(res);
                result_shown_at = Some(Instant::now());
                recorded_current = true;
                config_mode = false;
            }
        }

        let size = terminal.size()?;
        let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
        let (cx, cy) = compute_caret_screen_pos(&engine, area);
        effects.update(cx, cy, dt.as_secs_f32());

        terminal.draw(|frame| {
            let mut view = AppViewState {
                engine: &engine,
                theme: &themes[theme_idx],
                effects: &mut effects,
                stats: &stats,
                config_mode,
                config_cursor,
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

                // Global F2 or Alt+T -> cycle theme (avoids Kitty's Ctrl+T new-tab shortcut)
                if key.code == KeyCode::F(2)
                    || (key.modifiers.contains(KeyModifiers::ALT)
                        && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T')))
                {
                    theme_idx = (theme_idx + 1) % themes.len();
                    continue;
                }

                // Global Ctrl+K -> cycle caret style
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
                    effects.caret_style = effects.caret_style.next();
                    continue;
                }

                // Global Ctrl+L -> toggle cursor trail
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
                    effects.trail_enabled = !effects.trail_enabled;
                    continue;
                }

                // Global Ctrl+R -> repeat same words
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
                    engine.reset(false);
                    effects.reset_positions();
                    last_result = None;
                    last_is_pb = false;
                    result_shown_at = None;
                    recorded_current = false;
                    config_mode = false;
                    continue;
                }

                if engine.finished {
                    match key.code {
                        KeyCode::Tab | KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('N') => {
                            engine.reset(true);
                            effects.reset_positions();
                            last_result = None;
                            last_is_pb = false;
                            result_shown_at = None;
                            recorded_current = false;
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            engine.reset(false);
                            effects.reset_positions();
                            last_result = None;
                            last_is_pb = false;
                            result_shown_at = None;
                            recorded_current = false;
                        }
                        KeyCode::Char('t') | KeyCode::Char('T') => {
                            theme_idx = (theme_idx + 1) % themes.len();
                        }
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        _ => {}
                    }
                    continue;
                }

                if config_mode {
                    let labels = config_item_labels(&engine, &themes[theme_idx], &effects);
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
                            apply_config_action(
                                label,
                                &mut engine,
                                &mut effects,
                                &mut theme_idx,
                                themes.len(),
                            );
                            last_result = None;
                            last_is_pb = false;
                            recorded_current = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                // Typing mode
                match key.code {
                    KeyCode::Tab => {
                        engine.reset(true);
                        effects.reset_positions();
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
                        let (pre_x, pre_y) = compute_caret_screen_pos(&engine, area);
                        match engine.handle_char(ch) {
                            CharOutcome::Correct => {
                                effects.trigger_keystroke(pre_x, pre_y, false, false);
                            }
                            CharOutcome::Error => {
                                effects.trigger_keystroke(pre_x, pre_y, true, false);
                            }
                            CharOutcome::WordCompleted { clean } => {
                                effects.trigger_keystroke(pre_x, pre_y, !clean, clean);
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
    theme_idx: &mut usize,
    theme_count: usize,
) {
    match label {
        "@ punct" => {
            engine.config.punctuation = !engine.config.punctuation;
            engine.reset(true);
            effects.reset_positions();
        }
        "# nums" => {
            engine.config.numbers = !engine.config.numbers;
            engine.reset(true);
            effects.reset_positions();
        }
        "time" => {
            engine.config.mode = Mode::Time;
            engine.reset(true);
            effects.reset_positions();
        }
        "words" => {
            engine.config.mode = Mode::Words;
            engine.reset(true);
            effects.reset_positions();
        }
        "quote" => {
            engine.config.mode = Mode::Quote;
            engine.reset(true);
            effects.reset_positions();
        }
        s if s.starts_with("caret:") => {
            effects.caret_style = effects.caret_style.next();
        }
        s if s.starts_with("trail:") => {
            effects.trail_enabled = !effects.trail_enabled;
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
                effects.reset_positions();
            }
        }
    }
}
