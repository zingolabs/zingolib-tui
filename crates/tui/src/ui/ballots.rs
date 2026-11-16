//! The Ballots screen: previous voting rounds, compact by default; Enter
//! opens one round's details, Left goes back to the voting screen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use zingolib_tui_core::app::{State, VotingStage};
use zingolib_tui_core::format;

use super::inset;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    let area = inset(area);
    let height = usize::from(area.height);
    let v = &state.voting;
    let loading = matches!(v.stage, VotingStage::Loading);
    let mut lines = Vec::new();
    let headline = if loading {
        Span::styled(
            format!(
                "{} Loading ballots",
                crate::theme::spinner(state.elapsed_secs())
            ),
            t.muted.add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("Previous ballots", t.info.add_modifier(Modifier::BOLD))
    };
    lines.push(Line::from(vec![
        headline,
        Span::styled(format!("  · {}", v.source_label()), t.muted),
        Span::styled("  j/k move · enter details · ← voting", t.muted),
    ]));
    lines.push(Line::from(""));

    if let Some(detail) = v.history_detail.and_then(|i| v.history.get(i)) {
        // A document: scroll the whole thing, clamped to its length.
        lines.extend(detail_lines(detail, t));
        let max = lines.len().saturating_sub(1) as u16;
        let scroll = v.scroll.min(max);
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
        return;
    }

    // The list keeps the selected row in view, grouped: this wallet's
    // ballots first, then the rest of the network's rounds.
    let header = lines.len();
    let rows = list_lines(v, t);
    let visible = height.saturating_sub(header + 1).max(1);
    let mut offset = v.scroll as usize;
    offset = offset.min(rows.len().saturating_sub(1));
    let selected = v.history_selected;
    if selected < offset {
        offset = selected;
    }
    if selected + 1 > offset + visible {
        offset = selected + 1 - visible;
    }
    lines.extend(rows.iter().skip(offset).take(visible).cloned());
    lines.push(Line::from(""));
    let reserved = lines.len();
    if !v.log.is_empty() && height > reserved + 1 {
        let log_visible = (height - reserved - 1).min(v.log.len());
        let start = v.log.len() - log_visible;
        for line in &v.log[start..] {
            lines.push(Line::styled(line.clone(), t.muted));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The compact list: one line per round, nothing else.
fn list_lines<'a>(v: &'a zingolib_tui_core::app::VotingView, t: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    if v.history.is_empty() {
        lines.push(Line::styled(
            "  none yet — rounds this wallet votes in are recorded in its sidecar",
            t.muted,
        ));
        return lines;
    }
    let mut shown_mine = false;
    let mut shown_rest = false;
    for (index, row) in v.history.iter().enumerate() {
        let mine = !row.my_votes.is_empty();
        if mine && !shown_mine {
            lines.push(Line::styled("Your ballots", t.info));
            shown_mine = true;
        }
        if !mine && !shown_rest {
            if shown_mine {
                lines.push(Line::from(""));
            }
            lines.push(Line::styled("Other network rounds", t.info));
            shown_rest = true;
        }
        let selected = index == v.history_selected;
        let cursor = if selected { "› " } else { "  " };
        let voted = match row.my_votes.first().map(|(_, choice)| *choice) {
            None => Span::styled("not voted", t.muted),
            Some(0) => Span::styled("skipped", t.warning),
            Some(n) => Span::styled(format!("you: option {}", n + 1), t.positive),
        };
        // Titles are never shortened; long ones run and the pane clips.
        let title = row
            .proposals
            .first()
            .map(|proposal| proposal.title.clone())
            .unwrap_or_else(|| format::truncate_middle(&row.round_id, 16));
        let mut spans = vec![
            Span::styled(cursor, t.accent),
            Span::styled(if row.my_votes.is_empty() { "" } else { "✓ " }, t.positive),
            Span::styled(title, if selected { t.text } else { t.muted }),
            Span::raw("  "),
            Span::styled(row.status_label(), t.info),
            Span::raw("  "),
            voted,
        ];
        // Endorser org names, comma separated, one color per org.
        if !row.endorsers.is_empty() {
            spans.push(Span::styled("  ", t.muted));
            for (index, org) in row.endorsers.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(", ", t.muted));
                }
                spans.push(Span::styled(org.clone(), endorser_style(org, t)));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// One round opened with enter: what was decided, in plain language.
fn detail_lines<'a>(row: &'a zingolib_tui_core::app::HistoryRow, t: &'a Theme) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let status = match row.status {
        1 => Span::styled("open for voting", t.positive),
        2 => Span::styled("being tallied", t.warning),
        3 => Span::styled("final result", t.info),
        _ => Span::styled("closed", t.muted),
    };
    let closed = if row.end_time > 0 {
        format::datetime(u32::try_from(row.end_time).unwrap_or(0))
    } else {
        "unknown".to_string()
    };
    lines.push(Line::from(vec![
        status,
        Span::styled(format!(" · closed {closed}"), t.muted),
    ]));
    lines.push(Line::from(""));

    if row.proposals.is_empty() {
        lines.push(Line::styled(
            "this wallet voted in a round whose details are no longer published",
            t.muted,
        ));
    }
    for proposal in &row.proposals {
        lines.push(Line::from(vec![Span::styled(
            proposal.title.clone(),
            t.text.add_modifier(Modifier::BOLD),
        )]));

        let my_choice = row
            .my_votes
            .iter()
            .find(|(id, _)| *id == proposal.id)
            .map(|(_, choice)| *choice);
        let winner = row
            .tallies
            .iter()
            .filter(|(id, _, _)| *id == proposal.id)
            .max_by_key(|(_, _, zats)| *zats)
            .map(|(_, decision, _)| *decision);

        for (index, label) in proposal.options.iter().enumerate() {
            let weight = row
                .tallies
                .iter()
                .find(|(id, decision, _)| *id == proposal.id && *decision == index as u32)
                .map(|(_, _, zats)| *zats);
            let mine = my_choice == Some(index as u32);
            let mut spans = vec![Span::styled(if mine { " › " } else { "   " }, t.accent)];
            spans.push(Span::styled(label.clone(), {
                let base = if mine || winner == Some(index as u32) {
                    t.text
                } else {
                    t.muted
                };
                if mine {
                    base.add_modifier(Modifier::BOLD)
                } else {
                    base
                }
            }));
            if mine {
                spans.push(Span::styled("  your vote", t.positive));
            }
            if let Some(zats) = weight {
                let winner_style = if winner == Some(index as u32) {
                    t.positive.add_modifier(Modifier::BOLD)
                } else {
                    t.muted
                };
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("{} ZEC", format::zec(zats)),
                    winner_style,
                ));
            }
            lines.push(Line::from(spans));
        }
        if my_choice.is_none() && !row.my_votes.is_empty() {
            lines.push(Line::styled("   skipped this proposal", t.muted));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::styled("← back", t.muted));
    lines
}

/// One stable color per endorsing org, cycled from the theme's accents so
/// names stay distinguishable without hardcoding RGB.
fn endorser_style(org: &str, t: &Theme) -> ratatui::style::Style {
    const CYCLE: [fn(&Theme) -> ratatui::style::Style; 5] = [
        |t| t.positive,
        |t| t.warning,
        |t| t.info,
        |t| t.accent,
        |t| t.title,
    ];
    let hash: usize = org.bytes().map(|b| b as usize).sum();
    CYCLE[hash % CYCLE_LEN](t)
}

const CYCLE_LEN: usize = 5;
