use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{bignum, inset, jacquard, transactions};
use zingolib_tui_core::app::State;
use zingolib_tui_core::format;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    let area = inset(area);
    let width = usize::from(area.width);
    let b = &state.balance;

    let mut top = Vec::new();
    if !state.data_loaded {
        top.push(Line::styled("unknown until the wallet connects", t.muted));
    } else {
        let amount = format::zec(b.total());

        let tall = area.height >= 30;
        let big: Option<(Vec<String>, usize)> = tall
            .then(|| jacquard::render(&amount))
            .flatten()
            .map(|rows| (rows, jacquard::HEIGHT - 2))
            .or_else(|| bignum::render(&amount).map(|rows| (rows.to_vec(), 2)))
            .filter(|(rows, _)| {
                rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) + 5 <= width
            });
        match big {
            Some((rows, baseline)) => {
                for (i, row) in rows.into_iter().enumerate() {
                    let mut spans = vec![Span::styled(row, t.title)];
                    if i == baseline {
                        spans.push(Span::styled("  ZEC", t.muted));
                    }
                    top.push(Line::from(spans));
                }
            }
            None => top.push(Line::styled(format!("{amount} ZEC"), t.title)),
        }
        if b.pending() > 0 {
            top.push(Line::styled(
                format!("{} ZEC pending", format::zec(b.pending())),
                t.warning,
            ));
        }
        top.push(Line::from(""));
        top.extend(pool_lines(state, t));
    }

    let [top_area, _, title_area, list_area] = Layout::vertical([
        Constraint::Length(top.len() as u16),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(top), top_area);
    frame.render_widget(
        Paragraph::new(Line::styled("Recent transactions", t.muted)),
        title_area,
    );
    if state.txs.is_empty() {
        let message = if !state.data_loaded {
            "Nothing known yet. The wallet is still connecting."
        } else if state.sync.sessions_done == 0 {
            "No transactions yet. The first sync is still running."
        } else {
            "No transactions yet."
        };
        frame.render_widget(Paragraph::new(Line::styled(message, t.muted)), list_area);
    } else {
        transactions::draw_list(frame, list_area, state, t, false);
    }
}

pub fn pool_lines(state: &State, t: &Theme) -> Vec<Line<'static>> {
    let b = &state.balance;
    let mut lines = Vec::new();
    for (pool, balance) in b.pools() {
        if balance.total() == 0 {
            continue;
        }
        let mut spans = vec![
            Span::styled(
                format!("{:<12}", zingolib_tui_core::app::pool_label(pool)),
                t.pool(pool),
            ),
            Span::styled(format!("{} ZEC", format::zec(balance.total())), t.text),
        ];
        if balance.pending > 0 {
            spans.push(Span::styled(
                format!(" · {} pending", format::zec(balance.pending)),
                t.warning,
            ));
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::styled("no funds yet", t.muted));
    }
    lines
}
