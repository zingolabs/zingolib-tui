use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{inset, sync_style};
use pepper_sync::wallet::SyncMode;
use zingolib_tui_core::app::{Stage, State, SyncView};
use zingolib_tui_core::format;
use crate::theme::Theme;

const MAP_ROWS: usize = 6;
const MIN_CELLS: usize = 12;

pub fn draw(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    let area = inset(area);
    let (width, height) = (usize::from(area.width), usize::from(area.height));
    let s = &state.sync;
    let current =
        s.mode == SyncMode::NotRunning && s.error.is_none() && s.auto && s.sessions_done > 0;
    let headline = match s.mode {
        SyncMode::Running => "Syncing",
        SyncMode::Paused => "Paused",
        SyncMode::Shutdown => "Stopping",
        SyncMode::NotRunning if s.error.is_some() => "Sync failed",
        SyncMode::NotRunning if !s.auto => "Paused",
        SyncMode::NotRunning if current => "Up to date",
        SyncMode::NotRunning => "Connecting",
    };
    let headline = Span::styled(headline, sync_style(state, t).add_modifier(Modifier::BOLD));
    let figure = match s.tip {
        Some(tip) if current => {
            Span::styled(format!("block {}", format::group(u64::from(tip))), t.muted)
        }
        Some(_) => Span::styled(
            format!("{:.0}%", s.fraction() * 100.0),
            t.text.add_modifier(Modifier::BOLD),
        ),
        None => Span::raw(""),
    };

    let mut below = Vec::new();
    match s.mode {
        _ if current => below.push(Line::styled("Watching for new blocks.", t.muted)),
        SyncMode::Running => {
            if let Some(eta) = s.eta_secs() {
                below.push(Line::styled(
                    format!("about {} left", format::duration(eta)),
                    t.muted,
                ));
            }
        }
        SyncMode::Paused => below.push(paused(t)),
        SyncMode::NotRunning if s.error.is_none() && !s.auto => below.push(paused(t)),
        _ => {}
    }
    if let Some(err) = &s.error {
        below.push(Line::styled(err.clone(), t.negative));
        if let Some(at) = s.next_launch.filter(|at| *at > state.now) {
            let wait = at.duration_since(state.now).as_secs().max(1);
            below.push(Line::styled(
                format!("trying again in {}", format::duration(wait)),
                t.muted,
            ));
        }
    }
    if let Some(wallet) = state.wallet.as_ref() {
        below.push(Line::from(vec![
            Span::styled("Indexer  ", t.muted),
            Span::styled(wallet.server.clone(), t.text),
        ]));
    }
    let found = s.last_found.as_ref().map(|f| {
        Line::from(vec![
            Span::styled("Last found  ", t.muted),
            Span::styled(format::truncate_middle(&f.txid, 24), t.positive),
            Span::styled(
                format!(
                    "  {} · {} ago",
                    if f.confirmed { "confirmed" } else { "pending" },
                    format::duration(state.now.saturating_duration_since(f.at).as_secs())
                ),
                t.muted,
            ),
        ])
    });

    let reserved = 3 + below.len() + if found.is_some() { 2 } else { 0 };
    let phase = (s.mode == SyncMode::Running).then_some(state.ticks as usize);
    let chain = state
        .sync_map
        .then(|| map(state, width, height.saturating_sub(reserved), phase, t))
        .flatten()
        .or_else(|| strip(s, width, phase, t))
        .unwrap_or_default();

    let gap = width.saturating_sub(headline.width() + figure.width());
    let mut lines = vec![
        Line::from(vec![headline, Span::raw(" ".repeat(gap)), figure]),
        Line::from(""),
    ];
    if !chain.is_empty() {
        lines.extend(chain);
        lines.push(Line::from(""));
    }
    lines.extend(below);
    if let Some(found) = found {
        lines.resize(height.max(lines.len() + 1) - 1, Line::from(""));
        lines.push(found);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn paused(t: &Theme) -> Line<'static> {
    Line::styled("Nothing is scanned until you resume.", t.muted)
}

fn stages(s: &SyncView, birthday: u32, tip: u32, cells: usize, fraction: f64) -> Vec<Stage> {
    let span = u64::from(tip - birthday) + 1;
    let at = |i: usize| birthday + u32::try_from(span * i as u64 / cells as u64).unwrap_or(0);
    let (filled, _) = format::bar_split(fraction, cells);
    (0..cells)
        .map(|i| {
            let (start, end) = (at(i), at(i + 1));
            s.stage(start, end.max(start + 1)).unwrap_or(if i < filled {
                Stage::Scanned
            } else {
                Stage::NotYet
            })
        })
        .collect()
}

const LIVE_EVERY: usize = 5;
const IDLE_EVERY: usize = 12;

fn lit(index: usize, phase: Option<usize>, every: usize) -> bool {
    phase.is_some_and(|phase| index % every == phase % every)
}

fn strip(
    s: &SyncView,
    width: usize,
    phase: Option<usize>,
    t: &Theme,
) -> Option<Vec<Line<'static>>> {
    let (birthday, tip) = (s.birthday?, s.tip?);
    if tip < birthday || width < MIN_CELLS {
        return None;
    }
    let stages = stages(s, birthday, tip, width, s.fraction());
    let idle = !stages.contains(&Stage::Live);
    let cells: Vec<Span> = stages
        .into_iter()
        .enumerate()
        .map(|(i, stage)| match stage {
            Stage::Live if lit(i, phase, LIVE_EVERY) => Span::styled("╍", t.accent),
            Stage::Live => Span::styled("━", t.accent),
            _ if idle && lit(i, phase, IDLE_EVERY) => Span::styled("╌", t.accent),
            Stage::Scanned => Span::styled("━", t.text),
            Stage::NotYet => Span::styled("─", t.border),
        })
        .collect();
    let (from, to) = (
        format::group(u64::from(birthday)),
        format::group(u64::from(tip)),
    );
    let gap = width.saturating_sub(from.len() + to.len());
    Some(vec![
        Line::from(cells),
        Line::from(vec![
            Span::styled(from, t.muted),
            Span::raw(" ".repeat(gap)),
            Span::styled(to, t.muted),
        ]),
    ])
}

fn map(
    state: &State,
    width: usize,
    rows: usize,
    phase: Option<usize>,
    t: &Theme,
) -> Option<Vec<Line<'static>>> {
    let s = &state.sync;
    let (birthday, tip) = (s.birthday?, s.tip?);
    let rows = MAP_ROWS.min(rows.checked_sub(2)?);
    let label = format::group(u64::from(tip)).len() + 2;
    let per_row = width.saturating_sub(label).div_ceil(2);
    if tip < birthday || rows < 2 || per_row < MIN_CELLS {
        return None;
    }
    let total = per_row * rows;
    let span = u64::from(tip - birthday) + 1;
    let cell_of = |height: u32| (u64::from(height - birthday) * total as u64 / span) as usize;
    let mut found = vec![false; total];
    for tx in state
        .txs
        .iter()
        .filter(|tx| (birthday..=tip).contains(&tx.height))
    {
        found[cell_of(tx.height)] = true;
    }
    let stages = stages(s, birthday, tip, total, s.fraction());
    let idle = !stages.contains(&Stage::Live);

    let mut lines = Vec::with_capacity(rows + 2);
    for row in 0..rows {
        let first =
            birthday + u32::try_from(span * (row * per_row) as u64 / total as u64).unwrap_or(0);
        let mut spans = vec![Span::styled(
            format!("{:>w$}  ", format::group(u64::from(first)), w = label - 2),
            t.muted,
        )];
        for i in row * per_row..(row + 1) * per_row {
            if i > row * per_row {
                spans.push(Span::raw(" "));
            }
            spans.push(match stages[i] {
                Stage::Live if lit(i, phase, LIVE_EVERY) => Span::styled("□", t.accent),
                Stage::Live => Span::styled("■", t.accent),
                _ if found[i] => Span::styled("◆", t.positive),
                _ if idle && lit(i, phase, IDLE_EVERY) => Span::styled("□", t.accent),
                Stage::Scanned => Span::styled("■", t.muted),
                Stage::NotYet => Span::styled("·", t.border),
            });
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    let key = |glyph: &'static str, style: Style, what: &'static str| {
        [Span::styled(glyph, style), Span::styled(what, t.muted)]
    };
    let mut legend = vec![Span::raw(" ".repeat(label))];
    legend.extend(key("■", t.muted, " scanned    "));
    legend.extend(key("■", t.accent, " scanning    "));
    legend.extend(key("·", t.border, " not yet    "));
    legend.extend(key("◆", t.positive, " transaction"));
    lines.push(Line::from(legend));
    Some(lines)
}

pub fn draw_rescan_confirm(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    let birthday = state
        .wallet
        .as_ref()
        .map(|w| format::group(u64::from(w.birthday)))
        .unwrap_or_else(|| "the birthday".into());
    let lines = vec![
        Line::styled("Rescan the wallet?", t.title),
        Line::from(""),
        Line::styled(
            format!(
                "This discards everything scanned so far and starts again from block {birthday}."
            ),
            t.text,
        ),
        Line::styled("Addresses and the viewing key are kept.", t.text),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", t.accent),
            Span::styled(" to rescan, ", t.muted),
            Span::styled("n", t.accent),
            Span::styled(" to cancel", t.muted),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inset(area));
}
