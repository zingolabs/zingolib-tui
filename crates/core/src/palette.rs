#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dither {
    Off,
    Blocks,
    Braille,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
}

impl ColorDepth {
    pub fn detect() -> Self {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let term = std::env::var("TERM").unwrap_or_default();
        let truecolor = matches!(colorterm.as_str(), "truecolor" | "24bit")
            || term.ends_with("-direct")
            || std::env::var_os("WT_SESSION").is_some();
        if truecolor {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Ansi256
        }
    }

    pub fn from_flag(value: &str) -> Option<Self> {
        match value {
            "24bit" | "truecolor" => Some(ColorDepth::TrueColor),
            "256" => Some(ColorDepth::Ansi256),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Rgb,
    pub surface: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub border: Rgb,
    pub accent: Rgb,
    pub positive: Rgb,
    pub negative: Rgb,
    pub warning: Rgb,
    pub info: Rgb,
    pub orchard: Rgb,
    pub sapling: Rgb,
}

const fn rgb(hex: u32) -> Rgb {
    Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Clone, Copy, Debug)]
pub enum ThemeKind {
    Palette(Palette),
    Terminal,
    Mono,
}

pub struct ThemeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ThemeKind,
}

pub const THEMES: &[ThemeDef] = &[
    ThemeDef {
        id: "srcery",
        name: "Srcery",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x1c1b19),
            surface: rgb(0x3b3935),
            fg: rgb(0xfce8c3),
            muted: rgb(0x918175),
            border: rgb(0x312f2c),
            accent: rgb(0xfbb829),
            positive: rgb(0x98bc37),
            negative: rgb(0xf75341),
            warning: rgb(0xff8700),
            info: rgb(0x68a8e4),
            orchard: rgb(0xff5c8f),
            sapling: rgb(0xff5f00),
        }),
    },
    ThemeDef {
        id: "lightwallet",
        name: "Lightwallet",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x181818),
            surface: rgb(0x332e24),
            fg: rgb(0xe8e3d3),
            muted: rgb(0x8f8a7c),
            border: rgb(0x2c2c28),
            accent: rgb(0xd3a050),
            positive: rgb(0x5fd08a),
            negative: rgb(0xf2606e),
            warning: rgb(0xc98443),
            info: rgb(0xa8d5d5),
            orchard: rgb(0xd3a050),
            sapling: rgb(0xa0a344),
        }),
    },
    ThemeDef {
        id: "nord",
        name: "Nord",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x2e3440),
            surface: rgb(0x434c5e),
            fg: rgb(0xd8dee9),
            muted: rgb(0x616e88),
            border: rgb(0x4c566a),
            accent: rgb(0x88c0d0),
            positive: rgb(0xa3be8c),
            negative: rgb(0xbf616a),
            warning: rgb(0xebcb8b),
            info: rgb(0x81a1c1),
            orchard: rgb(0xb48ead),
            sapling: rgb(0xd08770),
        }),
    },
    ThemeDef {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0xeff1f5),
            surface: rgb(0xd2d4dc),
            fg: rgb(0x4c4f69),
            muted: rgb(0x6c6f85),
            border: rgb(0x9ca0b0),
            accent: rgb(0x8839ef),
            positive: rgb(0x40a02b),
            negative: rgb(0xd20f39),
            warning: rgb(0xdf8e1d),
            info: rgb(0x1e66f5),
            orchard: rgb(0xea76cb),
            sapling: rgb(0xfe640b),
        }),
    },
    ThemeDef {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x303446),
            surface: rgb(0x494e63),
            fg: rgb(0xc6d0f5),
            muted: rgb(0xa5adce),
            border: rgb(0x737994),
            accent: rgb(0xca9ee6),
            positive: rgb(0xa6d189),
            negative: rgb(0xe78284),
            warning: rgb(0xe5c890),
            info: rgb(0x8caaee),
            orchard: rgb(0xf4b8e4),
            sapling: rgb(0xef9f76),
        }),
    },
    ThemeDef {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x24273a),
            surface: rgb(0x404459),
            fg: rgb(0xcad3f5),
            muted: rgb(0xa5adcb),
            border: rgb(0x6e738d),
            accent: rgb(0xc6a0f6),
            positive: rgb(0xa6da95),
            negative: rgb(0xed8796),
            warning: rgb(0xeed49f),
            info: rgb(0x8aadf4),
            orchard: rgb(0xf5bde6),
            sapling: rgb(0xf5a97f),
        }),
    },
    ThemeDef {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x1e1e2e),
            surface: rgb(0x3b3d4f),
            fg: rgb(0xcdd6f4),
            muted: rgb(0xa6adc8),
            border: rgb(0x6c7086),
            accent: rgb(0xcba6f7),
            positive: rgb(0xa6e3a1),
            negative: rgb(0xf38ba8),
            warning: rgb(0xf9e2af),
            info: rgb(0x89b4fa),
            orchard: rgb(0xf5c2e7),
            sapling: rgb(0xfab387),
        }),
    },
    ThemeDef {
        id: "vitesse-dark",
        name: "Vitesse Dark",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x121212),
            surface: rgb(0x353535),
            fg: rgb(0xcecabe),
            muted: rgb(0x858480),
            border: rgb(0x52514f),
            accent: rgb(0x4d9375),
            positive: rgb(0x80a665),
            negative: rgb(0xcb7676),
            warning: rgb(0xe6cc77),
            info: rgb(0x6394bf),
            orchard: rgb(0xd9739f),
            sapling: rgb(0xbd976a),
        }),
    },
    ThemeDef {
        id: "vitesse-light",
        name: "Vitesse Light",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0xffffff),
            surface: rgb(0xdcdcdc),
            fg: rgb(0x393a34),
            muted: rgb(0x8f908c),
            border: rgb(0xc1c1bf),
            accent: rgb(0x1c6b48),
            positive: rgb(0x59873a),
            negative: rgb(0xab5959),
            warning: rgb(0xbda437),
            info: rgb(0x296aa3),
            orchard: rgb(0xa13865),
            sapling: rgb(0xb07d48),
        }),
    },
    ThemeDef {
        id: "vitesse-black",
        name: "Vitesse Black",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x000000),
            surface: rgb(0x252525),
            fg: rgb(0xafaca2),
            muted: rgb(0x7d7c78),
            border: rgb(0x464543),
            accent: rgb(0x4d9375),
            positive: rgb(0x80a665),
            negative: rgb(0xcb7676),
            warning: rgb(0xe6cc77),
            info: rgb(0x6394bf),
            orchard: rgb(0xd9739f),
            sapling: rgb(0xbd976a),
        }),
    },
    ThemeDef {
        id: "vercel-dark",
        name: "Vercel Dark",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x000000),
            surface: rgb(0x292929),
            fg: rgb(0xededed),
            muted: rgb(0xa0a0a0),
            border: rgb(0x454545),
            accent: rgb(0x50a8ff),
            positive: rgb(0x00ca52),
            negative: rgb(0xff5e63),
            warning: rgb(0xff9900),
            info: rgb(0x50a8ff),
            orchard: rgb(0xc472fb),
            sapling: rgb(0xff518d),
        }),
    },
    ThemeDef {
        id: "vercel-light",
        name: "Vercel Light",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0xffffff),
            surface: rgb(0xebebeb),
            fg: rgb(0x171717),
            muted: rgb(0x4d4d4d),
            border: rgb(0xc9c9c9),
            accent: rgb(0x0064e2),
            positive: rgb(0x107d32),
            negative: rgb(0xd60020),
            warning: rgb(0xa64f00),
            info: rgb(0x0064e2),
            orchard: rgb(0x7c00c9),
            sapling: rgb(0xc41562),
        }),
    },
    ThemeDef {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0x282828),
            surface: rgb(0x504945),
            fg: rgb(0xebdbb2),
            muted: rgb(0xa89984),
            border: rgb(0x665c54),
            accent: rgb(0xfe8019),
            positive: rgb(0xb8bb26),
            negative: rgb(0xfb4934),
            warning: rgb(0xfabd2f),
            info: rgb(0x83a598),
            orchard: rgb(0xd3869b),
            sapling: rgb(0xd65d0e),
        }),
    },
    ThemeDef {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        kind: ThemeKind::Palette(Palette {
            bg: rgb(0xfbf1c7),
            surface: rgb(0xd5c4a1),
            fg: rgb(0x3c3836),
            muted: rgb(0x7c6f64),
            border: rgb(0xbdae93),
            accent: rgb(0xaf3a03),
            positive: rgb(0x79740e),
            negative: rgb(0x9d0006),
            warning: rgb(0xb57614),
            info: rgb(0x076678),
            orchard: rgb(0x8f3f71),
            sapling: rgb(0xd65d0e),
        }),
    },
    ThemeDef {
        id: "terminal",
        name: "Terminal colours",
        kind: ThemeKind::Terminal,
    },
    ThemeDef {
        id: "mono",
        name: "Monochrome",
        kind: ThemeKind::Mono,
    },
];

pub const DEFAULT_THEME: &str = "srcery";
pub const NO_COLOR_THEME: &str = "mono";

pub fn index_of(id: &str) -> Option<usize> {
    THEMES.iter().position(|t| t.id == id)
}

pub fn ids() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|t| t.id)
}

pub fn choose(flag: Option<&str>, saved: Option<&str>, no_color: bool) -> usize {
    flag.and_then(index_of)
        .or_else(|| saved.and_then(index_of))
        .or_else(|| no_color.then(|| index_of(NO_COLOR_THEME)).flatten())
        .or_else(|| index_of(DEFAULT_THEME))
        .unwrap_or(0)
}

pub fn luminance(Rgb(r, g, b): Rgb) -> f64 {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

pub fn mix(from: Rgb, to: Rgb, t: f64) -> Rgb {
    let step = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
    Rgb(step(from.0, to.0), step(from.1, to.1), step(from.2, to.2))
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

pub fn ansi256(Rgb(r, g, b): Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest = |v: u8| {
        (0..LEVELS.len())
            .min_by_key(|&i| (i32::from(v) - i32::from(LEVELS[i])).abs())
            .unwrap_or(0)
    };
    let dist = |(x, y, z): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(x, r) + d(y, g) + d(z, b)
    };
    let (ri, gi, bi) = (nearest(r), nearest(g), nearest(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let cube_index = 16 + 36 * ri + 6 * gi + bi;

    let average = (u32::from(r) + u32::from(g) + u32::from(b)) / 3;
    let step = ((average.saturating_sub(8) + 5) / 10).min(23);
    let level = (8 + 10 * step) as u8;
    let grey_index = 232 + step as usize;

    let index = if dist(cube) <= dist((level, level, level)) {
        cube_index
    } else {
        grey_index
    };
    index as u8
}

pub fn ansi256_rgb(index: u8) -> Rgb {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match index {
        16..=231 => {
            let i = usize::from(index - 16);
            Rgb(LEVELS[i / 36], LEVELS[(i / 6) % 6], LEVELS[i % 6])
        }
        232..=255 => {
            let level = 8 + 10 * (index - 232);
            Rgb(level, level, level)
        }
        0 => Rgb(0, 0, 0),
        15 => Rgb(255, 255, 255),
        _ => Rgb(128, 128, 128),
    }
}

pub const QR_CONTRAST: f64 = 12.0;

pub fn qr_colours(p: &Palette, depth: ColorDepth) -> Option<(Rgb, Rgb)> {
    const BLACK: Rgb = Rgb(0, 0, 0);
    const WHITE: Rgb = Rgb(255, 255, 255);
    let shown = |rgb: Rgb| match depth {
        ColorDepth::TrueColor => rgb,
        ColorDepth::Ansi256 => ansi256_rgb(ansi256(rgb)),
    };
    let (mut ink, mut paper) = if luminance(p.bg) < luminance(p.fg) {
        (p.bg, p.fg)
    } else {
        (p.fg, p.bg)
    };
    for _ in 0..60 {
        if contrast(shown(ink), shown(paper)) >= QR_CONTRAST {
            break;
        }
        ink = mix(ink, BLACK, 0.08);
        paper = mix(paper, WHITE, 0.08);
    }
    Some((ink, paper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_names_are_unique() {
        for (i, a) in THEMES.iter().enumerate() {
            for b in &THEMES[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.name, b.name);
            }
        }
        assert!(index_of(DEFAULT_THEME).is_some());
        assert!(index_of(NO_COLOR_THEME).is_some());
    }

    #[test]
    fn precedence_is_flag_then_saved_then_no_color_then_default() {
        let at = |id| index_of(id).unwrap();
        assert_eq!(choose(Some("nord"), Some("vitesse-dark"), true), at("nord"));
        assert_eq!(choose(None, Some("vitesse-dark"), true), at("vitesse-dark"));
        assert_eq!(choose(None, None, true), at(NO_COLOR_THEME));
        assert_eq!(choose(None, None, false), at(DEFAULT_THEME));
        assert_eq!(choose(None, Some("solarized"), false), at(DEFAULT_THEME));
        assert_eq!(choose(Some("bogus"), Some("nord"), false), at("nord"));
    }

    #[test]
    fn ansi256_hits_known_entries() {
        assert_eq!(ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(ansi256(Rgb(255, 0, 0)), 196);
        assert_eq!(ansi256(Rgb(0, 0, 255)), 21);
        assert_eq!(ansi256(Rgb(128, 128, 128)), 244);
        assert_eq!(ansi256(Rgb(0x28, 0x28, 0x28)), 235);
    }

    #[test]
    fn ansi256_rgb_inverts_ansi256() {
        for index in 16..=255u8 {
            assert_eq!(ansi256(ansi256_rgb(index)), index, "index {index}");
        }
    }

    #[test]
    fn contrast_matches_wcag_reference_points() {
        assert!((contrast(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 0.01);
        assert!((contrast(Rgb(119, 119, 119), Rgb(255, 255, 255)) - 4.48).abs() < 0.01);
        assert!((contrast(Rgb(9, 9, 9), Rgb(9, 9, 9)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn qr_colours_follow_each_theme_and_always_scan() {
        let shown = |rgb: Rgb, depth: ColorDepth| match depth {
            ColorDepth::TrueColor => rgb,
            ColorDepth::Ansi256 => ansi256_rgb(ansi256(rgb)),
        };
        for def in THEMES {
            let ThemeKind::Palette(p) = def.kind else {
                continue;
            };
            for depth in [ColorDepth::TrueColor, ColorDepth::Ansi256] {
                let (ink, paper) =
                    qr_colours(&p, depth).expect("palette themes colour QR codes");
                let (ink, paper) = (shown(ink, depth), shown(paper, depth));
                assert!(
                    contrast(ink, paper) >= QR_CONTRAST,
                    "{} at {depth:?}: {:.1}:1",
                    def.id,
                    contrast(ink, paper)
                );
                assert!(luminance(ink) < luminance(paper), "{}", def.id);
            }
        }
    }
}
