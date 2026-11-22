use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: Cow<'static, str>,
    pub bg: Color,
    pub surface: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub text: Color,
    pub muted: Color,
    pub error: Color,
    pub extra: Color,
    pub caret_fg: Color,
    pub caret_bg: Color,
}

pub fn builtin_themes() -> Vec<Theme> {
    vec![
        Theme {
            name: Cow::Borrowed("serika_dark"),
            bg: Color::Rgb(50, 52, 55),
            surface: Color::Rgb(44, 46, 49),
            accent: Color::Rgb(226, 183, 20),
            accent_dim: Color::Rgb(140, 114, 24),
            text: Color::Rgb(209, 208, 197),
            muted: Color::Rgb(100, 102, 105),
            error: Color::Rgb(202, 71, 84),
            extra: Color::Rgb(126, 42, 51),
            caret_fg: Color::Rgb(35, 36, 38),
            caret_bg: Color::Rgb(226, 183, 20),
        },
        Theme {
            name: Cow::Borrowed("catppuccin"),
            bg: Color::Rgb(30, 30, 46),
            surface: Color::Rgb(24, 24, 37),
            accent: Color::Rgb(203, 166, 247),
            accent_dim: Color::Rgb(116, 97, 150),
            text: Color::Rgb(205, 214, 244),
            muted: Color::Rgb(108, 112, 134),
            error: Color::Rgb(243, 139, 168),
            extra: Color::Rgb(166, 78, 102),
            caret_fg: Color::Rgb(30, 30, 46),
            caret_bg: Color::Rgb(245, 194, 231),
        },
        Theme {
            name: Cow::Borrowed("dracula"),
            bg: Color::Rgb(40, 42, 54),
            surface: Color::Rgb(33, 34, 44),
            accent: Color::Rgb(189, 147, 249),
            accent_dim: Color::Rgb(108, 86, 150),
            text: Color::Rgb(248, 248, 242),
            muted: Color::Rgb(98, 114, 164),
            error: Color::Rgb(255, 85, 85),
            extra: Color::Rgb(160, 50, 50),
            caret_fg: Color::Rgb(40, 42, 54),
            caret_bg: Color::Rgb(255, 121, 198),
        },
        Theme {
            name: Cow::Borrowed("nord"),
            bg: Color::Rgb(46, 52, 64),
            surface: Color::Rgb(39, 44, 54),
            accent: Color::Rgb(136, 192, 208),
            accent_dim: Color::Rgb(76, 110, 125),
            text: Color::Rgb(236, 239, 244),
            muted: Color::Rgb(97, 110, 136),
            error: Color::Rgb(191, 97, 106),
            extra: Color::Rgb(130, 62, 70),
            caret_fg: Color::Rgb(46, 52, 64),
            caret_bg: Color::Rgb(136, 192, 208),
        },
        Theme {
            name: Cow::Borrowed("gruvbox"),
            bg: Color::Rgb(40, 40, 40),
            surface: Color::Rgb(29, 32, 33),
            accent: Color::Rgb(250, 189, 47),
            accent_dim: Color::Rgb(155, 115, 32),
            text: Color::Rgb(235, 219, 178),
            muted: Color::Rgb(146, 131, 116),
            error: Color::Rgb(251, 73, 52),
            extra: Color::Rgb(160, 45, 35),
            caret_fg: Color::Rgb(40, 40, 40),
            caret_bg: Color::Rgb(254, 128, 25),
        },
        Theme {
            name: Cow::Borrowed("matrix"),
            bg: Color::Rgb(10, 15, 12),
            surface: Color::Rgb(6, 10, 8),
            accent: Color::Rgb(0, 255, 102),
            accent_dim: Color::Rgb(0, 130, 52),
            text: Color::Rgb(150, 255, 180),
            muted: Color::Rgb(34, 90, 52),
            error: Color::Rgb(255, 68, 68),
            extra: Color::Rgb(150, 30, 30),
            caret_fg: Color::Rgb(10, 15, 12),
            caret_bg: Color::Rgb(0, 255, 102),
        },
    ]
}

#[derive(Deserialize)]
struct DmsColorsFile {
    mode: String,
    colors: DmsColorModes,
}

#[derive(Deserialize)]
struct DmsColorModes {
    dark: DmsPalette,
    light: DmsPalette,
}

#[derive(Deserialize)]
struct DmsPalette {
    background: String,
    surface_container: String,
    primary: String,
    primary_container: String,
    on_background: String,
    outline: String,
    error: String,
    error_container: String,
    on_primary: String,
}

pub fn dms_colors_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("dms").join("dms-colors.json");
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".cache")
        .join("dms")
        .join("dms-colors.json")
}

pub fn dms_colors_mtime() -> Option<SystemTime> {
    fs::metadata(dms_colors_path()).ok()?.modified().ok()
}

pub fn load_dms_theme() -> Option<Theme> {
    let raw = fs::read_to_string(dms_colors_path()).ok()?;
    let parsed: DmsColorsFile = serde_json::from_str(&raw).ok()?;
    let pal = if parsed.mode == "light" {
        &parsed.colors.light
    } else {
        &parsed.colors.dark
    };

    Some(Theme {
        name: Cow::Borrowed("dms_sync"),
        bg: parse_hex(&pal.background)?,
        surface: parse_hex(&pal.surface_container)?,
        accent: parse_hex(&pal.primary)?,
        accent_dim: parse_hex(&pal.primary_container)?,
        text: parse_hex(&pal.on_background)?,
        muted: parse_hex(&pal.outline)?,
        error: parse_hex(&pal.error)?,
        extra: parse_hex(&pal.error_container)?,
        caret_fg: parse_hex(&pal.on_primary)?,
        caret_bg: parse_hex(&pal.primary)?,
    })
}

fn parse_hex(s: &str) -> Option<Color> {
    let hex = s.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

pub fn build_theme_list() -> Vec<Theme> {
    let mut list = builtin_themes();
    if let Some(dms) = load_dms_theme() {
        list.insert(0, dms);
    }
    list
}

pub fn find_theme_index(themes: &[Theme], name: &str) -> usize {
    if name == "dms" || name == "dms_sync" {
        return themes
            .iter()
            .position(|t| t.name == "dms_sync")
            .unwrap_or(0);
    }
    themes.iter().position(|t| t.name == name).unwrap_or(0)
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (a, b) {
        let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t).round() as u8;
        let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t).round() as u8;
        let bl = (b1 as f32 + (b2 as f32 - b1 as f32) * t).round() as u8;
        Color::Rgb(r, g, bl)
    } else {
        b
    }
}

impl Theme {
    pub fn lerp(&self, target: &Theme, t: f32) -> Theme {
        Theme {
            name: target.name.clone(),
            bg: lerp_color(self.bg, target.bg, t),
            surface: lerp_color(self.surface, target.surface, t),
            accent: lerp_color(self.accent, target.accent, t),
            accent_dim: lerp_color(self.accent_dim, target.accent_dim, t),
            text: lerp_color(self.text, target.text, t),
            muted: lerp_color(self.muted, target.muted, t),
            error: lerp_color(self.error, target.error, t),
            extra: lerp_color(self.extra, target.extra, t),
            caret_fg: lerp_color(self.caret_fg, target.caret_fg, t),
            caret_bg: lerp_color(self.caret_bg, target.caret_bg, t),
        }
    }
}
