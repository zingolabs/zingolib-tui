//! The voting screen: round summary, the ballot as horizontal option
//! switchers with a trailing Cast vote row, and a read-only ballot review
//! once the vote is in.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use zingolib_tui_core::app::{BallotRow, State, VotingStage};
use zingolib_tui_core::format;

use super::{inset, kv};
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    let area = inset(area);
    let height = usize::from(area.height);
    let v = &state.voting;
    let busy = matches!(
        v.stage,
        VotingStage::Loading | VotingStage::Arming | VotingStage::Casting
    );
    let (headline, style) = match &v.stage {
        VotingStage::Idle => ("No round loaded", t.muted),
        VotingStage::Loading => ("Fetching round", t.muted),
        VotingStage::Ready => ("Ballot open", t.positive),
        VotingStage::Arming => ("Arming session", t.warning),
        VotingStage::Armed | VotingStage::Casting => ("Casting vote", t.warning),
        VotingStage::Finished => ("Vote cast", t.positive),
        VotingStage::Failed(error) => (error.as_str(), t.negative),
    };
    let mut lines = Vec::new();
    let mut headline_span = Span::styled(headline.to_string(), style.add_modifier(Modifier::BOLD));
    if busy {
        headline_span = Span::styled(
            format!(
                "{} {}",
                crate::theme::spinner(state.elapsed_secs()),
                headline
            ),
            style.add_modifier(Modifier::BOLD),
        );
    }
    lines.push(Line::from(headline_span));
    lines.push(Line::from(""));
    if v.stage == VotingStage::Idle {
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    lines.push(kv(t, "Round", format::truncate_middle(&v.round_id, 24)));
    lines.push(kv(t, "Snapshot", format::group(v.snapshot_height)));
    lines.push(kv(
        t,
        "Voting ends",
        format::datetime(u32::try_from(v.vote_end).unwrap_or(0)),
    ));
    lines.push(kv(
        t,
        "Eligible",
        format!("{} ZEC", format::zec(v.eligible_weight_zatoshi)),
    ));
    lines.push(kv(
        t,
        "Vote network",
        format!("{} · {} bundles", v.source_label(), v.bundle_count),
    ));
    lines.push(Line::from(""));

    lines.push(Line::from(""));

    if v.stage == VotingStage::Finished {
        lines.push(Line::styled("Your ballot", t.info));
        for row in &v.ballot {
            lines.push(review_line(row, t));
        }
        if !v.tallies.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled("Recorded tallies", t.info));
            for (proposal_id, choice) in &v.tallies {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {proposal_id}. "), t.muted),
                    Span::styled(format!("choice {}", choice + 1), t.text),
                ]));
            }
        }
    } else {
        let mut flat = 0usize;
        for row in &v.ballot {
            lines.push(Line::from(Span::styled(
                row.title.clone(),
                t.info.add_modifier(Modifier::BOLD),
            )));
            for option in 0..row.options {
                lines.push(option_line(row, option, flat == v.selected, t));
                flat += 1;
            }
            lines.push(skip_line(row, flat == v.selected, t));
            flat += 1;
        }
        lines.push(cast_line(v, t));
    }
    lines.push(Line::from(""));

    let reserved = lines.len();
    if !v.log.is_empty() && height > reserved + 1 {
        let visible = (height - reserved - 1).min(v.log.len());
        let start = v.log.len() - visible;
        for line in &v.log[start..] {
            lines.push(Line::styled(line.clone(), t.muted));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// One option line: the choice's meaning, marked when it is the pick.
fn option_line<'a>(row: &'a BallotRow, option: u32, selected: bool, t: &'a Theme) -> Line<'a> {
    let cursor = if selected { "› " } else { "  " };
    let chosen = row.choice == Some(option + 1);
    let label = row
        .option_labels
        .get(option as usize)
        .cloned()
        .unwrap_or_else(|| format!("Option {}", option + 1));
    let style = if chosen {
        t.positive.add_modifier(Modifier::BOLD)
    } else if selected {
        t.text
    } else {
        t.muted
    };
    let check = if chosen { "✓ " } else { "" };
    Line::from(vec![
        Span::styled(cursor, t.accent),
        Span::styled(format!("{check}{label}"), style),
    ])
}

/// A proposal's trailing Skip line; picking it records an explicit skip.
fn skip_line<'a>(row: &'a BallotRow, selected: bool, t: &'a Theme) -> Line<'a> {
    let cursor = if selected { "› " } else { "  " };
    let skipped = row.choice == Some(0);
    let style = if skipped {
        t.warning.add_modifier(Modifier::BOLD)
    } else if selected {
        t.text
    } else {
        t.muted
    };
    let check = if skipped { "✓ " } else { "" };
    Line::from(vec![
        Span::styled(cursor, t.accent),
        Span::styled(format!("{check}Skip this proposal"), style),
    ])
}

/// The trailing Cast vote row: navigation target and the only place Enter
/// casts. Shows the decision progress and lights up when the ballot is full.
fn cast_line<'a>(v: &'a zingolib_tui_core::app::VotingView, t: &'a Theme) -> Line<'a> {
    let selected = v.on_cast_row();
    let decided = v.decided_count();
    let total = v.ballot.len();
    let cursor = if selected { "› " } else { "  " };
    let label = match v.stage {
        VotingStage::Arming | VotingStage::Armed | VotingStage::Casting => {
            format!("Casting vote… {decided}/{total} decided")
        }
        _ => format!("Cast vote  {decided}/{total} decided"),
    };
    let style = if selected {
        t.selection
    } else if decided == total {
        t.positive
    } else {
        t.muted
    };
    Line::from(vec![
        Span::styled(cursor, t.accent),
        Span::styled(label, style.add_modifier(Modifier::BOLD)),
    ])
}

/// Post-cast review of one proposal: what this wallet voted.
fn review_line<'a>(row: &'a BallotRow, t: &'a Theme) -> Line<'a> {
    let choice = match row.choice {
        None => Span::styled("not voted", t.muted),
        Some(0) => Span::styled("skipped", t.warning),
        Some(n) => Span::styled(format!("option {n}"), t.positive),
    };
    Line::from(vec![
        Span::styled(format!("  {}. ", row.proposal_id), t.muted),
        Span::styled(row.title.clone(), t.text),
        Span::raw("  "),
        choice,
    ])
}
