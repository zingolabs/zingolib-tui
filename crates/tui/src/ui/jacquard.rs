struct Glyph {
    ch: char,
    advance: usize,
    rows: [&'static str; SPAN],
}

const SPAN: usize = 16;
pub const HEIGHT: usize = SPAN / 2;

const GLYPHS: &[Glyph] = &[
    Glyph {
        ch: '0',
        advance: 8,
        rows: [
            "........", "........", "........", "........", "..#####.", ".##.###.", ".##..##.",
            ".##..##.", ".##..##.", ".##..##.", ".##..##.", ".###.#..", "..###...", "........",
            "........", "........",
        ],
    },
    Glyph {
        ch: '1',
        advance: 4,
        rows: [
            "....", "....", "....", "....", ".###", ".###", "..##", "..##", "..##", "..##", "..##",
            "..#.", "....", "....", "....", "....",
        ],
    },
    Glyph {
        ch: '2',
        advance: 8,
        rows: [
            "........", "........", "........", "........", "..####..", ".##.###.", ".#...##.",
            ".....##.", "....##..", "...##...", "..##....", ".######.", ".#####..", "........",
            "........", "........",
        ],
    },
    Glyph {
        ch: '3',
        advance: 7,
        rows: [
            ".......", ".......", ".......", ".......", ".####..", ".#####.", "...#...", "..##...",
            "..##...", "...###.", "....##.", "....##.", "....##.", "....##.", ".##....", "..#....",
        ],
    },
    Glyph {
        ch: '4',
        advance: 9,
        rows: [
            ".........",
            ".........",
            ".........",
            ".........",
            "......#..",
            "....###..",
            "...####..",
            "..##.##..",
            "..#..##..",
            ".#...##..",
            ".#######.",
            ".....##..",
            ".....##..",
            ".....##..",
            ".....##..",
            ".....#...",
        ],
    },
    Glyph {
        ch: '5',
        advance: 7,
        rows: [
            ".......", ".......", ".......", ".......", "..####.", ".###...", ".#.....", ".#####.",
            ".#####.", "....##.", "....##.", "....##.", ".##.##.", ".##.##.", ".###...", ".##....",
        ],
    },
    Glyph {
        ch: '6',
        advance: 7,
        rows: [
            "....#..", "...###.", "..#.##.", ".##.#..", ".##....", ".####..", ".##.##.", ".##.###",
            ".##.###", ".##.###", ".##.###", ".#####.", "..##...", ".......", ".......", ".......",
        ],
    },
    Glyph {
        ch: '7',
        advance: 9,
        rows: [
            ".........",
            ".........",
            ".........",
            ".........",
            "..######.",
            ".######..",
            "....##...",
            "...#.....",
            "..##.....",
            "..##.....",
            "..##.....",
            "..##.....",
            "..##.....",
            "..##.....",
            "..##.....",
            "..#......",
        ],
    },
    Glyph {
        ch: '8',
        advance: 8,
        rows: [
            "...##...", "..####..", ".##.###.", ".##..##.", ".###.##.", "..####..", "..#####.",
            ".#..###.", ".#..###.", ".##..##.", ".###.##.", "..####..", "...#....", "........",
            "........", "........",
        ],
    },
    Glyph {
        ch: '9',
        advance: 7,
        rows: [
            ".......", ".......", ".......", ".......", "..####.", ".##.###", ".##.###", ".##.###",
            ".##.###", ".##.###", "..#####", "....###", "....###", "..#.##.", ".####..", "..##...",
        ],
    },
    Glyph {
        ch: ',',
        advance: 3,
        rows: [
            "...", "...", "...", "...", "...", "...", "...", "...", "...", "...", "...", ".##",
            ".##", "..#", ".#.", "...",
        ],
    },
    Glyph {
        ch: '.',
        advance: 3,
        rows: [
            "...", "...", "...", "...", "...", "...", "...", "...", "...", "...", "...", ".##",
            ".##", "...", "...", "...",
        ],
    },
    Glyph {
        ch: ':',
        advance: 3,
        rows: [
            "...", "...", ".##", ".##", ".##", "...", "...", "...", "...", "...", "...", ".##",
            ".##", "...", "...", "...",
        ],
    },
];

fn glyph(ch: char) -> Option<&'static Glyph> {
    GLYPHS.iter().find(|g| g.ch == ch)
}

pub fn covers(text: &str) -> bool {
    text.chars().all(|c| c == ' ' || glyph(c).is_some())
}

pub fn render(text: &str) -> Option<Vec<String>> {
    if !covers(text) {
        return None;
    }
    let width: usize = text
        .chars()
        .map(|c| {
            if c == ' ' {
                4
            } else {
                glyph(c).map_or(0, |g| g.advance)
            }
        })
        .sum();
    let mut pixels = vec![vec![false; width]; SPAN];
    let mut x = 0;
    for c in text.chars() {
        if c == ' ' {
            x += 4;
            continue;
        }
        let g = glyph(c)?;
        for (y, row) in g.rows.iter().enumerate() {
            for (i, cell) in row.bytes().enumerate() {
                if cell == b'#' && x + i < width {
                    pixels[y][x + i] = true;
                }
            }
        }
        x += g.advance;
    }
    Some(
        (0..HEIGHT)
            .map(|r| {
                (0..width)
                    .map(|c| match (pixels[2 * r][c], pixels[2 * r + 1][c]) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_spans_the_full_height_and_its_advance() {
        for g in GLYPHS {
            assert_eq!(g.rows.len(), SPAN, "{}", g.ch);
            for row in g.rows {
                assert!(
                    row.len() <= g.advance,
                    "{} row wider than its advance",
                    g.ch
                );
                assert!(row.bytes().all(|b| b == b'#' || b == b'.'), "{}", g.ch);
            }
            assert!(g.rows.iter().any(|r| r.contains('#')), "{} is blank", g.ch);
        }
    }

    #[test]
    fn digits_render_eight_rows_and_differ() {
        let a = render("2,604,500").unwrap();
        assert_eq!(a.len(), HEIGHT);
        assert!(a.iter().any(|r| r.contains('█') || r.contains('▀')));
        assert_ne!(render("1").unwrap(), render("7").unwrap());
        assert!(render("x").is_none());
        assert!(covers("12:34"));
    }
}
