use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use zcash_protocol::PoolType;
use zingolib_tui_core::palette::{
    self, ColorDepth, Palette, Rgb, ThemeKind, ansi256, lerp, qr_colours,
};

pub struct Theme {
    pub base: Style,
    pub text: Style,
    pub muted: Style,
    pub border: Style,
    pub accent: Style,
    pub title: Style,
    pub selection: Style,
    pub positive: Style,
    pub negative: Style,
    pub warning: Style,
    pub info: Style,
    pub cursor: Style,
    pools: [Style; 4],
    pub rgb: Option<(Rgb, Rgb)>,
    pub qr: Option<Style>,
}

fn color(rgb: Rgb, depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColorDepth::Ansi256 => Color::Indexed(ansi256(rgb)),
    }
}

impl Theme {
    pub fn new(index: usize, depth: ColorDepth) -> Self {
        let def = palette::THEMES.get(index).unwrap_or(&palette::THEMES[0]);
        match def.kind {
            ThemeKind::Palette(p) => Self::from_palette(p, depth),
            ThemeKind::Terminal => Self::terminal(),
            ThemeKind::Mono => Self::mono(),
        }
    }

    pub fn pool(&self, pool: PoolType) -> Style {
        match pool {
            PoolType::IRONWOOD => self.pools[0],
            PoolType::ORCHARD => self.pools[1],
            PoolType::SAPLING => self.pools[2],
            PoolType::TRANSPARENT => self.pools[3],
        }
    }

    fn from_palette(p: Palette, depth: ColorDepth) -> Self {
        let c = |rgb| color(rgb, depth);
        let fg = |rgb| Style::new().fg(c(rgb));
        let qr = qr_colours(&p, depth)
            .map(|(ink, paper)| Style::new().fg(color(ink, depth)).bg(color(paper, depth)));
        Self {
            base: Style::new().bg(c(p.bg)).fg(c(p.fg)),
            text: fg(p.fg),
            muted: fg(p.muted),
            border: fg(p.border),
            accent: fg(p.accent),
            title: fg(p.accent).add_modifier(Modifier::BOLD),
            selection: Style::new()
                .bg(c(p.surface))
                .fg(c(p.fg))
                .add_modifier(Modifier::BOLD),
            positive: fg(p.positive),
            negative: fg(p.negative),
            warning: fg(p.warning),
            info: fg(p.info),
            cursor: Style::new().bg(c(p.accent)).fg(c(p.bg)),
            pools: [fg(p.info), fg(p.orchard), fg(p.sapling), fg(p.fg)],
            rgb: Some((p.accent, p.bg)),
            qr,
        }
    }

    fn terminal() -> Self {
        let fg = |color| Style::new().fg(color);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            base: Style::new().bg(Color::Reset).fg(Color::Reset),
            text: Style::new(),
            muted: dim,
            border: dim,
            accent: fg(Color::Cyan),
            title: fg(Color::Cyan).add_modifier(Modifier::BOLD),
            selection: Style::new().add_modifier(Modifier::REVERSED),
            positive: fg(Color::Green),
            negative: fg(Color::Red),
            warning: fg(Color::Yellow),
            info: fg(Color::Blue),
            cursor: Style::new().add_modifier(Modifier::REVERSED),
            pools: [
                fg(Color::Cyan),
                fg(Color::Magenta),
                fg(Color::LightRed),
                Style::new(),
            ],
            rgb: None,
            qr: Some(Style::new().fg(Color::Black).bg(Color::White)),
        }
    }

    fn mono() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            base: Style::new().bg(Color::Reset).fg(Color::Reset),
            text: Style::new(),
            muted: dim,
            border: dim,
            accent: bold,
            title: bold,
            selection: Style::new().add_modifier(Modifier::REVERSED),
            positive: Style::new(),
            negative: bold,
            warning: Style::new(),
            info: Style::new(),
            cursor: Style::new().add_modifier(Modifier::REVERSED),
            rgb: None,
            pools: [Style::new(); 4],
            qr: None,
        }
    }
}

pub fn swatch(index: usize, depth: ColorDepth) -> Option<(Color, Vec<Color>)> {
    match palette::THEMES.get(index)?.kind {
        ThemeKind::Palette(p) => Some((
            color(p.bg, depth),
            [
                p.accent, p.positive, p.negative, p.warning, p.info, p.orchard, p.sapling,
            ]
            .into_iter()
            .map(|rgb| color(rgb, depth))
            .collect(),
        )),
        ThemeKind::Terminal => Some((
            Color::Reset,
            vec![
                Color::Cyan,
                Color::Green,
                Color::Red,
                Color::Yellow,
                Color::Blue,
                Color::Magenta,
                Color::Cyan,
                Color::LightRed,
            ],
        )),
        ThemeKind::Mono => None,
    }
}

fn pulse(secs: f32, period: f32) -> f32 {
    0.5 - 0.5 * (secs / period * std::f32::consts::TAU).cos()
}

pub fn shimmer(t: &Theme, text: &str, secs: f32, depth: ColorDepth) -> Vec<Span<'static>> {
    let Some((accent, bg)) = t.rgb else {
        return vec![Span::styled(text.to_string(), t.title)];
    };
    let dim = lerp(accent, bg, 0.5);
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let wave = pulse(secs * 1.1 + i as f32 * 0.13, 1.6);
            Span::styled(
                ch.to_string(),
                Style::new()
                    .fg(color(lerp(dim, accent, wave), depth))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

pub fn spinner(secs: f32) -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    FRAMES[(secs * 12.0) as usize % FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use zingolib_tui_core::palette::{DEFAULT_THEME, index_of};

    #[test]
    fn palettes_resolve_at_both_depths() {
        for (i, def) in palette::THEMES.iter().enumerate() {
            if !matches!(def.kind, ThemeKind::Palette(_)) {
                continue;
            }
            let truecolor = Theme::new(i, ColorDepth::TrueColor);
            let indexed = Theme::new(i, ColorDepth::Ansi256);
            assert!(matches!(truecolor.base.bg, Some(Color::Rgb(..))), "{}", def.id);
            assert!(
                matches!(indexed.base.bg, Some(Color::Indexed(_))),
                "{} must not emit 24-bit colour at 256 colours",
                def.id
            );
        }
    }

    #[test]
    fn the_terminal_theme_uses_the_terminals_black_and_white() {
        let qr = Theme::new(index_of("terminal").unwrap(), ColorDepth::TrueColor)
            .qr
            .unwrap();
        assert_eq!((qr.fg, qr.bg), (Some(Color::Black), Some(Color::White)));
    }

    #[test]
    fn monochrome_uses_no_colour() {
        let t = Theme::new(index_of("mono").unwrap(), ColorDepth::TrueColor);
        for style in [
            t.text,
            t.muted,
            t.border,
            t.accent,
            t.title,
            t.selection,
            t.positive,
            t.negative,
            t.warning,
            t.info,
            t.cursor,
        ] {
            assert!(style.fg.is_none() && style.bg.is_none(), "{style:?}");
        }
        assert!(t.qr.is_none());
        assert!(swatch(index_of(DEFAULT_THEME).unwrap(), ColorDepth::TrueColor).is_some());
    }
}
