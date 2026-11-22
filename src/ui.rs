use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::effects::{keystroke_wave, monkey_mascot, scramble_glyph, CaretStyle, CursorEffects};
use crate::engine::{Mode, TestResult, TypingEngine, TIME_OPTIONS, WORD_OPTIONS};
use crate::storage::{AnimSettings, StatsData};
use crate::themes::{lerp_color, Theme};

#[derive(Clone, Debug)]
pub struct WordPlacement {
    pub word_idx: usize,
    pub x: u16,
}

pub struct AppViewState<'a> {
    pub engine: &'a TypingEngine,
    pub theme: &'a Theme,
    pub effects: &'a mut CursorEffects,
    pub stats: &'a StatsData,
    pub settings: &'a AnimSettings,
    pub config_mode: bool,
    pub config_cursor: usize,
    pub settings_open: bool,
    pub settings_cursor: usize,
    pub last_result: Option<&'a TestResult>,
    pub last_is_pb: bool,
    pub result_shown_at: Option<Instant>,
    pub anim_clock: f64,
}

pub fn wrap_words(engine: &TypingEngine, max_width: u16) -> Vec<Vec<WordPlacement>> {
    let mut lines: Vec<Vec<WordPlacement>> = Vec::new();
    let mut current: Vec<WordPlacement> = Vec::new();
    let mut x: u16 = 0;

    for (idx, target) in engine.words.iter().enumerate() {
        let t_len = target.chars().count();
        let typed_len = engine.typed_words[idx].chars().count();
        let w_len = t_len.max(typed_len) as u16;
        let needed = if current.is_empty() { w_len } else { w_len + 1 };

        if !current.is_empty() && x + needed > max_width {
            lines.push(current);
            current = Vec::new();
            x = 0;
        }
        if !current.is_empty() {
            x += 1;
        }
        current.push(WordPlacement { word_idx: idx, x });
        x += w_len;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn compute_caret_screen_pos(
    engine: &TypingEngine,
    area: Rect,
    x_nudge: i16,
) -> (u16, u16) {
    let box_width = area.width.saturating_sub(10).clamp(36, 76);
    let base_x = area.x + area.width.saturating_sub(box_width) / 2;
    let box_x = (base_x as i32 + x_nudge as i32).clamp(1, area.width.saturating_sub(2) as i32) as u16;
    let center_y = (area.y + area.height / 2).max(area.y + 7);

    let lines = wrap_words(engine, box_width);
    let mut active_line = 0usize;
    let mut word_x = 0u16;

    for (li, line) in lines.iter().enumerate() {
        if let Some(wp) = line.iter().find(|wp| wp.word_idx == engine.current_word_idx) {
            active_line = li;
            word_x = wp.x;
            break;
        }
    }

    let start_line = active_line.saturating_sub(1);
    let rel_row = (active_line - start_line) as u16;
    let typed_chars = engine.typed_words[engine.current_word_idx].chars().count() as u16;

    (box_x + word_x + typed_chars, center_y + rel_row)
}

pub fn config_item_labels(
    engine: &TypingEngine,
    theme: &Theme,
    settings: &AnimSettings,
) -> Vec<(String, bool)> {
    let mut items = Vec::new();
    let cfg = &engine.config;

    if cfg.mode != Mode::Quote {
        items.push(("@ punct".to_string(), cfg.punctuation));
        items.push(("# nums".to_string(), cfg.numbers));
    }
    items.push(("time".to_string(), cfg.mode == Mode::Time));
    items.push(("words".to_string(), cfg.mode == Mode::Words));
    items.push(("quote".to_string(), cfg.mode == Mode::Quote));

    match cfg.mode {
        Mode::Time => {
            for &sec in TIME_OPTIONS {
                items.push((sec.to_string(), cfg.duration_secs == sec));
            }
        }
        Mode::Words => {
            for &cnt in WORD_OPTIONS {
                items.push((cnt.to_string(), cfg.word_count == cnt));
            }
        }
        Mode::Quote => {}
    }

    items.push((format!("theme:{}", theme.name), false));
    let any_fx = settings.items().iter().any(|(_, _, on)| *on);
    items.push(("settings".to_string(), any_fx));
    items
}

pub fn draw(frame: &mut Frame, state: &mut AppViewState<'_>) {
    let area = frame.area();
    let theme = state.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg).fg(theme.text)),
        area,
    );

    if area.width < 52 || area.height < 16 {
        let msg = Paragraph::new("Terminal too small (min 52x16)")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.error).bg(theme.bg));
        frame.render_widget(
            msg,
            Rect::new(area.x, area.y + area.height / 2, area.width, 1),
        );
        return;
    }

    draw_header(frame, area, state);

    if state.engine.finished {
        if let Some(res) = state.last_result {
            draw_results(frame, area, state, res);
        }
    } else {
        draw_typing(frame, area, state);
    }

    draw_footer(frame, area, state);

    if state.settings_open {
        draw_settings_modal(frame, area, state);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AppViewState<'_>) {
    let theme = state.theme;
    let engine = state.engine;

    let mascot_w = if state.settings.mascot { 14 } else { 0 };
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(mascot_w),
            Constraint::Min(20),
            Constraint::Length(28),
        ])
        .split(Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 3));

    if state.settings.mascot {
        let mascot_lines = monkey_mascot(
            state.anim_clock,
            engine.started,
            engine.finished,
            engine.live_wpm(),
            engine.combo,
            state.last_is_pb,
        );
        let mascot_color = if (state.last_is_pb && engine.finished) || engine.combo >= 20 {
            theme.accent
        } else {
            theme.muted
        };
        let mascot_para = Paragraph::new(
            mascot_lines
                .iter()
                .map(|&l| Line::from(Span::styled(l, Style::default().fg(mascot_color))))
                .collect::<Vec<_>>(),
        );
        frame.render_widget(mascot_para, top_chunks[0]);
    }

    // Title + streak badge
    let mut title_spans = vec![
        Span::styled(
            "monkeytype",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" tui", Style::default().fg(theme.muted)),
    ];
    if engine.combo >= 15 && !engine.finished {
        let flame = if ((state.anim_clock * 10.0) as usize) % 2 == 0 {
            " ~*"
        } else {
            " *~"
        };
        title_spans.push(Span::styled(
            format!("  {flame} {}x streak", engine.combo),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![Line::from(title_spans)]).alignment(Alignment::Left),
        Rect::new(top_chunks[1].x, top_chunks[1].y + 1, top_chunks[1].width, 1),
    );

    // Right PB & theme info
    let pb_text = state
        .stats
        .personal_bests
        .get(&engine.config.pb_key())
        .map(|pb| format!("pb: {:.0} wpm", pb.wpm))
        .unwrap_or_else(|| "pb: --".to_string());
    let right_para = Paragraph::new(Line::from(vec![
        Span::styled(pb_text, Style::default().fg(theme.text)),
        Span::styled(
            format!("  |  {}", theme.name),
            Style::default().fg(theme.muted),
        ),
    ]))
    .alignment(Alignment::Right);
    frame.render_widget(
        right_para,
        Rect::new(top_chunks[2].x, top_chunks[2].y + 1, top_chunks[2].width, 1),
    );

    // Centered pill bar
    let items = config_item_labels(engine, theme, state.settings);
    let mut spans = Vec::new();
    for (idx, (label, active)) in items.iter().enumerate() {
        let selected = state.config_mode && !state.settings_open && idx == state.config_cursor;
        let style = if selected {
            Style::default()
                .fg(theme.caret_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if *active {
            Style::default()
                .fg(theme.accent)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted).bg(theme.surface)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::styled(" ", Style::default().bg(theme.bg)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        Rect::new(area.x + 2, area.y + 4, area.width.saturating_sub(4), 1),
    );

    if state.config_mode && !state.settings_open {
        let hint = Paragraph::new("config: <-/-> move  Enter toggle  Esc/Tab resume  q quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.text));
        frame.render_widget(
            hint,
            Rect::new(area.x + 2, area.y + 5, area.width.saturating_sub(4), 1),
        );
    }
}

fn compute_ghost_pos(engine: &TypingEngine, pace_wpm: f64) -> Option<(usize, usize)> {
    if !engine.started || pace_wpm <= 1.0 {
        return None;
    }
    let mut remaining = ((pace_wpm * 5.0 * engine.elapsed()) / 60.0).floor() as usize;
    for (w_idx, word) in engine.words.iter().enumerate() {
        let w_len = word.chars().count();
        if remaining <= w_len {
            return Some((w_idx, remaining));
        }
        remaining -= w_len + 1; // word + space
    }
    None
}

fn draw_typing(frame: &mut Frame, area: Rect, state: &mut AppViewState<'_>) {
    let theme = state.theme;
    let engine = state.engine;
    let settings = state.settings;
    let box_width = area.width.saturating_sub(10).clamp(36, 76);
    let base_x = area.x + area.width.saturating_sub(box_width) / 2;
    let nudge = state.effects.typo_nudge_offset(settings.typo_nudge);
    let box_x = (base_x as i32 + nudge as i32).clamp(1, area.width.saturating_sub(2) as i32) as u16;
    let center_y = (area.y + area.height / 2).max(area.y + 7);

    let pace_wpm = state
        .stats
        .personal_bests
        .get(&engine.config.pb_key())
        .map(|pb| pb.wpm)
        .filter(|&w| w >= 10.0)
        .unwrap_or(60.0);

    // Status row above words
    let left_status = match engine.config.mode {
        Mode::Time => {
            if engine.started {
                format!("{}", engine.remaining_secs().ceil() as u64)
            } else {
                format!("{}", engine.config.duration_secs)
            }
        }
        Mode::Words | Mode::Quote => {
            format!(
                "{}/{}",
                (engine.current_word_idx + 1).min(engine.words.len()),
                engine.words.len()
            )
        }
    };
    let right_status = if engine.started {
        if settings.pace_caret {
            format!(
                "pace {:.0}  |  {:.0} wpm   {:.0}% acc",
                pace_wpm,
                engine.live_wpm(),
                engine.accuracy()
            )
        } else {
            format!("{:.0} wpm   {:.0}% acc", engine.live_wpm(), engine.accuracy())
        }
    } else {
        "start typing...".to_string()
    };

    let status_rect = Rect::new(box_x, center_y.saturating_sub(2), box_width, 1);
    frame.render_widget(
        Paragraph::new(Span::styled(
            left_status,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        status_rect,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(right_status, Style::default().fg(theme.muted)))
            .alignment(Alignment::Right),
        status_rect,
    );

    // Combo aura breathing hairline above typing box when streak >= 25
    if settings.combo_aura && engine.combo >= 25 {
        let pulse = ((state.anim_clock * 5.0).sin() * 0.5 + 0.5) as f32;
        let aura_color = lerp_color(theme.accent_dim, theme.accent, pulse);
        let fill = ((engine.combo.min(100) as usize * box_width as usize) / 100).max(4);
        let pad = (box_width as usize - fill) / 2;
        let bar = format!("{}{}", " ".repeat(pad), "─".repeat(fill));
        frame.render_widget(
            Paragraph::new(bar).style(Style::default().fg(aura_color)),
            Rect::new(box_x, center_y.saturating_sub(1), box_width, 1),
        );
    }

    // 3-line rolling viewport
    let lines = wrap_words(engine, box_width);
    let active_line = lines
        .iter()
        .position(|line| line.iter().any(|wp| wp.word_idx == engine.current_word_idx))
        .unwrap_or(0);
    let start_line = active_line.saturating_sub(1);
    state
        .effects
        .observe_start_line(start_line, settings.line_fade);
    let line_fade = if settings.line_fade {
        state.effects.line_fade_progress()
    } else {
        1.0
    };
    let scramble_p = state.effects.scramble_progress(settings.restart_scramble);
    let ghost_pos = if settings.pace_caret {
        compute_ghost_pos(engine, pace_wpm)
    } else {
        None
    };

    let (caret_x, caret_y) = compute_caret_screen_pos(engine, area, nudge);

    let buf = frame.buffer_mut();

    for row_off in 0..3u16 {
        let y = center_y + row_off;
        for x in box_x..(box_x + box_width) {
            if let Some(trail_bg) =
                state
                    .effects
                    .trail_bg_at(x, y, theme, settings.soft_trail)
            {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(trail_bg);
                }
            }
        }
    }

    for (row_off, line) in lines.iter().skip(start_line).take(3).enumerate() {
        let y = center_y + row_off as u16;
        let row_fade = if row_off == 2 { line_fade } else { 1.0 };
        for wp in line {
            draw_word_cells(
                buf,
                y,
                box_x + wp.x,
                wp.word_idx,
                engine,
                theme,
                row_fade,
                scramble_p,
                state.anim_clock,
                box_x,
                box_width,
            );
            // Render Pace Caret (Ghost Racer) if on this word
            if let Some((gw, gc)) = ghost_pos {
                if gw == wp.word_idx {
                    let gx = box_x + wp.x + gc as u16;
                    if (gx != caret_x || y != caret_y) && gx < box_x + box_width {
                        if let Some(cell) = buf.cell_mut((gx, y)) {
                            let ghost_bg = lerp_color(theme.bg, theme.accent_dim, 0.55);
                            cell.set_bg(ghost_bg);
                        }
                    }
                }
            }
        }
    }

    // Position the real terminal hardware cursor so Kitty's native GPU cursor_trail triggers
    if !state.config_mode && !state.settings_open {
        frame.set_cursor_position((caret_x, caret_y));
        if settings.soft_trail {
            draw_caret(frame.buffer_mut(), caret_x, caret_y, state.effects, theme);
        }
    }

    // Render floating ASCII sparks
    if settings.sparks {
        let buf = frame.buffer_mut();
        let now = Instant::now();
        for spark in &state.effects.sparks {
            let sx = spark.x.round() as i32;
            let sy = spark.y.round() as i32;
            if sx >= area.x as i32
                && sx < (area.x + area.width) as i32
                && sy >= (area.y + 6) as i32
                && sy < (area.y + area.height.saturating_sub(3)) as i32
            {
                let age = now.duration_since(spark.born).as_secs_f32() / 0.38;
                let base = if spark.is_error {
                    theme.error
                } else {
                    theme.accent
                };
                let fg = lerp_color(base, theme.bg, age);
                if let Some(cell) = buf.cell_mut((sx as u16, sy as u16)) {
                    if cell.symbol() == " " {
                        let mut tmp = [0u8; 4];
                        cell.set_symbol(spark.ch.encode_utf8(&mut tmp));
                        cell.set_fg(fg);
                    }
                }
            }
        }
    }

    // Below typing box: quote author or animated equalizer wave
    let cursor_y = center_y + 4;
    let footer_limit = area.y + area.height.saturating_sub(2);

    if engine.config.mode == Mode::Quote && !engine.quote_author.is_empty() && cursor_y < footer_limit {
        frame.render_widget(
            Paragraph::new(format!("- {}", engine.quote_author))
                .style(Style::default().fg(theme.muted)),
            Rect::new(box_x, cursor_y, box_width, 1),
        );
    } else if settings.wave && cursor_y < footer_limit {
        let wave = keystroke_wave(state.anim_clock, state.effects.wave_energy, 24);
        frame.render_widget(
            Paragraph::new(wave)
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.accent_dim)),
            Rect::new(box_x, cursor_y, box_width, 1),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_word_cells(
    buf: &mut Buffer,
    y: u16,
    x: u16,
    word_idx: usize,
    engine: &TypingEngine,
    theme: &Theme,
    row_fade: f32,
    scramble_p: f32,
    anim_clock: f64,
    box_x: u16,
    box_width: u16,
) {
    let target: Vec<char> = engine.words[word_idx].chars().collect();
    let typed: Vec<char> = engine.typed_words[word_idx].chars().collect();
    let max_len = target.len().max(typed.len());

    for i in 0..max_len {
        let cx = x + i as u16;
        let (mut ch, mut fg, underline) = if i < typed.len() {
            if i < target.len() {
                if typed[i] == target[i] {
                    (target[i], theme.text, false)
                } else {
                    (target[i], theme.error, true)
                }
            } else {
                (typed[i], theme.extra, true)
            }
        } else {
            (target[i], theme.muted, false)
        };

        // Restart glitch-decode sweep (left-to-right across the box)
        if scramble_p < 1.0 && typed.is_empty() {
            let rel_x = cx.saturating_sub(box_x) as f32 / box_width.max(1) as f32;
            if rel_x > scramble_p {
                ch = scramble_glyph(word_idx, i, anim_clock);
                fg = theme.accent_dim;
            }
        }

        if row_fade < 1.0 && typed.is_empty() {
            fg = lerp_color(theme.bg, fg, row_fade);
        }

        if let Some(cell) = buf.cell_mut((cx, y)) {
            let mut tmp = [0u8; 4];
            cell.set_symbol(ch.encode_utf8(&mut tmp));
            cell.set_fg(fg);
            if underline {
                cell.set_style(Style::default().fg(fg).add_modifier(Modifier::UNDERLINED));
            }
        }
    }
}

fn draw_caret(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    effects: &CursorEffects,
    theme: &Theme,
) {
    let caret_bg = effects.caret_color(theme);
    if let Some(cell) = buf.cell_mut((x, y)) {
        match effects.caret_style {
            CaretStyle::Block => {
                cell.set_fg(theme.caret_fg);
                cell.set_bg(caret_bg);
            }
            CaretStyle::Beam => {
                if cell.symbol() == " " {
                    cell.set_symbol("▎");
                }
                cell.set_fg(caret_bg);
                cell.set_style(
                    Style::default()
                        .fg(caret_bg)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                );
            }
            CaretStyle::Underline => {
                cell.set_fg(caret_bg);
                cell.set_style(
                    Style::default()
                        .fg(caret_bg)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
                );
            }
        }
    }
}

fn draw_results(frame: &mut Frame, area: Rect, state: &AppViewState<'_>, res: &TestResult) {
    let theme = state.theme;

    // Subtle PB confetti rain in background when combo_aura is enabled
    if state.settings.combo_aura && state.last_is_pb {
        let buf = frame.buffer_mut();
        for p in &state.effects.confetti {
            let px = p.x.round() as u16;
            let py = p.y.round() as u16;
            if px > area.x && px < area.x + area.width - 1 && py > area.y + 5 && py < area.y + area.height - 1 {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    let mut tmp = [0u8; 4];
                    cell.set_symbol(p.ch.encode_utf8(&mut tmp));
                    cell.set_fg(lerp_color(theme.accent_dim, theme.accent, p.accent_mix));
                }
            }
        }
    }

    let box_w = area.width.saturating_sub(8).min(78);
    let left_x = area.x + area.width.saturating_sub(box_w) / 2;
    let content_h = 14u16;
    let avail_top = area.y + 6;
    let avail_h = area.height.saturating_sub(7);
    let top_y = avail_top + avail_h.saturating_sub(content_h) / 2;

    let reveal = state
        .result_shown_at
        .map(|t| (t.elapsed().as_secs_f32() / 0.55).clamp(0.0, 1.0))
        .unwrap_or(1.0);

    let left_col_w = 13u16;
    let gap = 2u16;
    let wpm_shown = res.wpm * reveal as f64;
    let acc_shown = res.accuracy * reveal as f64;

    let mut left_lines = vec![
        Line::from(Span::styled("wpm", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            format!("{wpm_shown:.0}"),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("acc", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            format!("{acc_shown:.0}%"),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
    ];
    if state.last_is_pb {
        let sparkle = if ((state.anim_clock * 6.0) as usize) % 2 == 0 {
            "✦ NEW PB ✦"
        } else {
            "⋆ NEW PB ⋆"
        };
        left_lines.push(Line::from(""));
        left_lines.push(Line::from(Span::styled(
            sparkle,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(
        Paragraph::new(left_lines).alignment(Alignment::Center),
        Rect::new(left_x, top_y, left_col_w, 7),
    );

    let chart_x = left_x + left_col_w + gap;
    let chart_w = box_w.saturating_sub(left_col_w + gap);
    draw_wpm_chart(frame.buffer_mut(), chart_x, top_y, chart_w, 7, res, reveal, theme);

    let det_y = top_y + 9;
    let cols = [
        ("test type", res.config_label.clone()),
        ("raw", format!("{:.0}", res.raw_wpm)),
        ("consistency", format!("{:.0}%", res.consistency)),
        ("max combo", format!("{}x", res.max_combo)),
        (
            "chars",
            format!(
                "{}/{}/{}/{}",
                res.correct_chars, res.incorrect_chars, res.extra_chars, res.missed_chars
            ),
        ),
        ("time", format!("{:.1}s", res.elapsed_secs)),
    ];
    let col_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, cols.len() as u32); cols.len()])
        .split(Rect::new(left_x, det_y, box_w, 2));

    for (chunk, (label, val)) in col_chunks.iter().zip(cols.iter()) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(*label, Style::default().fg(theme.muted))),
                Line::from(Span::styled(val.as_str(), Style::default().fg(theme.text))),
            ])
            .alignment(Alignment::Center),
            *chunk,
        );
    }

    let actions = "[Tab / Enter] new test   [r] repeat   [F2 / t] theme   [F3] settings   [Esc / q] quit";
    frame.render_widget(
        Paragraph::new(actions)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.muted)),
        Rect::new(left_x, det_y + 4, box_w, 1),
    );
}

fn draw_settings_modal(frame: &mut Frame, area: Rect, state: &AppViewState<'_>) {
    let theme = state.theme;
    let items = state.settings.items();
    let modal_w = area.width.saturating_sub(6).min(64);
    let modal_h = (items.len() as u16 + 6).min(area.height.saturating_sub(2));
    let mx = area.x + area.width.saturating_sub(modal_w) / 2;
    let my = area.y + area.height.saturating_sub(modal_h) / 2;
    let rect = Rect::new(mx, my, modal_w, modal_h);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(" Animation & Visual Settings ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.surface).fg(theme.text));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    for (idx, (name, _, enabled)) in items.iter().enumerate() {
        let selected = idx == state.settings_cursor;
        let check = if *enabled { "[x]" } else { "[ ]" };
        let row_style = if selected {
            Style::default()
                .fg(theme.caret_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if *enabled {
            Style::default().fg(theme.text).bg(theme.surface)
        } else {
            Style::default().fg(theme.muted).bg(theme.surface)
        };
        let status = if *enabled { "ON " } else { "OFF" };
        let pad = inner.width.saturating_sub(name.len() as u16 + 11) as usize;
        lines.push(Line::from(Span::styled(
            format!(" {check}  {name}{}{status} ", " ".repeat(pad)),
            row_style,
        )));
    }

    lines.push(Line::from(""));
    let desc = items
        .get(state.settings_cursor)
        .map(|(_, d, _)| *d)
        .unwrap_or("");
    lines.push(Line::from(Span::styled(
        format!(" {desc}"),
        Style::default().fg(theme.accent),
    )));
    lines.push(Line::from(Span::styled(
        " Up/Down: select | Space/Enter: toggle | a: all on | n: zen | Esc/F3: close",
        Style::default().fg(theme.muted),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

#[allow(clippy::too_many_arguments)]
fn draw_wpm_chart(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    res: &TestResult,
    reveal: f32,
    theme: &Theme,
) {
    if res.samples.is_empty() || width < 14 || height < 4 {
        return;
    }

    let plot_w = (width - 5) as usize;
    let n = res.samples.len();

    let mut net_series = Vec::with_capacity(plot_w);
    let mut raw_series = Vec::with_capacity(plot_w);
    let mut err_series = vec![false; plot_w];

    if n == 1 {
        net_series.resize(plot_w, res.samples[0].wpm);
        raw_series.resize(plot_w, res.samples[0].raw_wpm);
        if res.samples[0].errors > 0 {
            err_series[plot_w / 2] = true;
        }
    } else {
        for i in 0..plot_w {
            let pos = (i as f64 / (plot_w - 1).max(1) as f64) * (n - 1) as f64;
            let lo = (pos.floor() as usize).min(n - 1);
            let hi = (lo + 1).min(n - 1);
            let frac = pos - lo as f64;
            let net = res.samples[lo].wpm + (res.samples[hi].wpm - res.samples[lo].wpm) * frac;
            let raw =
                res.samples[lo].raw_wpm + (res.samples[hi].raw_wpm - res.samples[lo].raw_wpm) * frac;
            net_series.push(net);
            raw_series.push(raw);
        }
        for (idx, s) in res.samples.iter().enumerate() {
            if s.errors > 0 {
                let col = ((idx as f64 / (n - 1) as f64) * (plot_w - 1) as f64).round() as usize;
                err_series[col.min(plot_w - 1)] = true;
            }
        }
    }

    let max_val = net_series
        .iter()
        .chain(raw_series.iter())
        .copied()
        .fold(20.0_f64, f64::max);

    let plot_rows = (height - 1) as usize;
    let put = |buf: &mut Buffer, cx: u16, cy: u16, s: &str, fg: ratatui::style::Color| {
        if let Some(cell) = buf.cell_mut((cx, cy)) {
            cell.set_symbol(s);
            cell.set_fg(fg);
        }
    };

    let top_lbl = format!("{max_val:3.0} |");
    for (i, ch) in top_lbl.chars().enumerate() {
        let mut tmp = [0u8; 4];
        put(buf, x + i as u16, y, ch.encode_utf8(&mut tmp), theme.muted);
    }
    for r in 1..plot_rows {
        put(buf, x + 4, y + r as u16, "|", theme.muted);
    }
    for (i, ch) in "  0 +".chars().enumerate() {
        let mut tmp = [0u8; 4];
        put(
            buf,
            x + i as u16,
            y + plot_rows as u16,
            ch.encode_utf8(&mut tmp),
            theme.muted,
        );
    }

    for i in 0..plot_w {
        let cx = x + 5 + i as u16;
        let net_h = ((net_series[i] / max_val) as f32 * plot_rows as f32) * reveal;
        let raw_h = ((raw_series[i] / max_val) as f32 * plot_rows as f32) * reveal;

        for r in 0..plot_rows {
            let level = (plot_rows - 1 - r) as f32;
            let cy = y + r as u16;
            if net_h >= level + 0.75 {
                put(buf, cx, cy, "█", theme.accent);
            } else if net_h >= level + 0.25 {
                put(buf, cx, cy, "▄", theme.accent);
            } else if raw_h >= level + 0.5 {
                put(buf, cx, cy, "·", theme.muted);
            }
        }

        if err_series[i] {
            put(buf, cx, y + plot_rows as u16, "×", theme.error);
        } else {
            put(buf, cx, y + plot_rows as u16, "─", theme.muted);
        }
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &AppViewState<'_>) {
    if state.engine.finished {
        return;
    }
    let hint = "Tab: restart | Ctrl+R: repeat | Ctrl+W: del word | F2: theme | F3: settings | Esc: config";
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(state.theme.muted)),
        Rect::new(
            area.x + 1,
            area.y + area.height.saturating_sub(2),
            area.width.saturating_sub(2),
            1,
        ),
    );
}
