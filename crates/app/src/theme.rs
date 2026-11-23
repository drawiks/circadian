use circadian_core::model::Theme as ThemeSetting;
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{px, rgb, App, Hsla};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub surface: Hsla,
    pub surface2: Hsla,
    pub surface3: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub cyan: Hsla,
    pub amber: Hsla,
    pub green: Hsla,
    pub rose: Hsla,
    pub primary: Hsla,
}

pub fn col(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn palette(cx: &App) -> Palette {
    if cx.theme().is_dark() {
        Palette {
            bg: col(0x080c16),
            surface: col(0x0f1524),
            surface2: col(0x141d30),
            surface3: col(0x1a243a),
            border: col(0x263149),
            text: col(0xe9eef8),
            muted: col(0x8b97b4),
            accent: col(0xa78bfa),
            cyan: col(0x38bdf8),
            amber: col(0xfbbf24),
            green: col(0x34d399),
            rose: col(0xfb7185),
            primary: col(0x8b5cf6),
        }
    } else {
        Palette {
            bg: col(0xf4f6fb),
            surface: col(0xffffff),
            surface2: col(0xeef1f8),
            surface3: col(0xe6ebf5),
            border: col(0xdbe2ee),
            text: col(0x121a2b),
            muted: col(0x68738c),
            accent: col(0x7c3aed),
            cyan: col(0x0284c7),
            amber: col(0xd97706),
            green: col(0x059669),
            rose: col(0xe11d48),
            primary: col(0x7c3aed),
        }
    }
}

pub fn apply(cx: &mut App, setting: ThemeSetting) {
    match setting {
        ThemeSetting::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ThemeSetting::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemeSetting::System => Theme::sync_system_appearance(None, cx),
    }
    let p = palette(cx);
    Theme::update(cx, |theme| {
        theme.font_family = "Inter Variable".into();
        theme.mono_font_family = "JetBrains Mono".into();
        theme.font_size = px(15.);
        theme.mono_font_size = px(13.);
        theme.radius = px(12.);
        theme.radius_lg = px(18.);
        theme.shadow = true;

        theme.colors.background = p.bg;
        theme.colors.foreground = p.text;
        theme.colors.border = p.border;
        theme.colors.muted = p.surface2;
        theme.colors.muted_foreground = p.muted;
        theme.colors.popover = p.surface;
        theme.colors.popover_foreground = p.text;
        theme.colors.primary = p.primary;
        theme.colors.primary_hover = p.accent;
        theme.colors.primary_active = p.accent;
        theme.colors.primary_foreground = col(0xffffff);
        theme.colors.secondary = p.surface2;
        theme.colors.secondary_hover = p.surface3;
        theme.colors.secondary_active = p.surface3;
        theme.colors.secondary_foreground = p.text;
        theme.colors.accent = p.surface2;
        theme.colors.accent_foreground = p.text;
        theme.colors.input = p.border;
        theme.colors.ring = p.accent;
        theme.colors.selection = p.accent.opacity(0.28);
        theme.colors.sidebar = p.surface;
        theme.colors.sidebar_foreground = p.text;
        theme.colors.sidebar_border = p.border;
        theme.colors.sidebar_accent = p.surface2;
        theme.colors.sidebar_accent_foreground = p.text;
        theme.colors.button = p.surface2;
        theme.colors.button_hover = p.surface3;
        theme.colors.button_active = p.surface3;
        theme.colors.button_foreground = p.text;
        theme.colors.button_primary = p.primary;
        theme.colors.button_primary_hover = p.accent;
        theme.colors.button_primary_active = p.accent;
        theme.colors.button_primary_foreground = col(0xffffff);
        theme.colors.button_secondary = p.surface2;
        theme.colors.button_secondary_hover = p.surface3;
        theme.colors.button_secondary_active = p.surface3;
        theme.colors.button_secondary_foreground = p.text;
        theme.colors.button_danger = p.rose;
        theme.colors.button_danger_hover = p.rose;
        theme.colors.button_danger_active = p.rose;
        theme.colors.button_danger_foreground = col(0xffffff);
        theme.colors.danger = p.rose;
        theme.colors.danger_hover = p.rose;
        theme.colors.danger_active = p.rose;
        theme.colors.warning = p.amber;
        theme.colors.warning_hover = p.amber;
        theme.colors.warning_active = p.amber;
        theme.colors.success = p.green;
        theme.colors.success_hover = p.green;
        theme.colors.success_active = p.green;
        theme.colors.chart_1 = p.accent;
        theme.colors.chart_2 = p.cyan;
        theme.colors.chart_3 = p.amber;
        theme.colors.chart_4 = p.green;
        theme.colors.chart_5 = p.rose;
        theme.colors.chart_grid = p.border;
    });
}
