use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};

use super::diagram::{Flow, Part, Piece};
use super::{inset, kv, kv_styled, note, tabs};
use zingolib::wallet::output::SpendStatus;
use zingolib::wallet::summary::data::{SendType, TransactionKind};
use zingo_status::confirmation_status::ConfirmationStatus;
use zingolib_tui_core::app::{
    DetailMode, OutputRole, OutputRow, State, TxRow, pool_label, spend_describe, status_flag,
    tx_kind_label, tx_kind_sign,
};
use zingolib_tui_core::format;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, state: &State, t: &Theme) {
    if state.txs.is_empty() {
        let msg = if state.sync.sessions_done == 0 {
            "No transactions yet. The first sync is still running."
        } else {
            "No transactions yet."
        };
        note(frame, area, msg, t);
        return;
    }
    draw_list(frame, inset(area), state, t, true);
}

pub fn draw_list(frame: &mut Frame, area: Rect, state: &State, t: &Theme, with_time: bool) {
    let kind_width = state
        .txs
        .iter()
        .map(|tx| tx_kind_label(tx.kind).chars().count())
        .max()
        .unwrap_or(0) as u16;
    let rows = state.txs.iter().map(|tx| {
        let memo = tx
            .first_memo()
            .map(|memo| memo.replace('\n', " "))
            .unwrap_or_default();
        let when = if with_time {
            format::datetime(tx.datetime)
        } else {
            format::date(tx.datetime)
        };
        let style = kind_style(tx.kind, t);
        Row::new(vec![
            Cell::from(Span::styled(when, t.muted)),
            Cell::from(Span::styled(
                tx_kind_label(tx.kind),
                style.add_modifier(Modifier::BOLD),
            )),
            Cell::from(Line::from(Span::styled(amount(tx), style)).alignment(Alignment::Right)),
            Cell::from(Span::styled(memo, t.text.add_modifier(Modifier::ITALIC))),
            Cell::from(
                Line::from(Span::styled(status_flag(tx.status), status_style(tx.status, t)))
                    .alignment(Alignment::Right),
            ),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(if with_time { 16 } else { 10 }),
            Constraint::Length(kind_width),
            Constraint::Length(14),
            Constraint::Fill(1),
            Constraint::Length(7),
        ],
    )
    .column_spacing(2)
    .row_highlight_style(t.selection);
    let mut ts = TableState::default().with_selected(Some(state.tx_selected));
    frame.render_stateful_widget(table, area, &mut ts);
}

fn amount(tx: &TxRow) -> String {
    format!("{}{}", tx_kind_sign(tx.kind), format::zec(tx.value))
}

fn kind_style(kind: TransactionKind, t: &Theme) -> Style {
    match kind {
        TransactionKind::Received => t.positive,
        TransactionKind::Sent(SendType::Send) => t.negative,
        TransactionKind::Sent(_) => t.info,
    }
}

fn status_style(status: ConfirmationStatus, t: &Theme) -> Style {
    if status.is_confirmed() {
        t.muted
    } else if status.is_failed() {
        t.negative
    } else {
        t.warning
    }
}

fn event_style(event: &zingolib_tui_core::app::WalletEventRow, t: &Theme) -> Style {
    if event.is_received() {
        t.positive
    } else if event.is_sent() {
        t.negative
    } else {
        t.info
    }
}

pub fn draw_detail(frame: &mut Frame, area: Rect, state: &State, t: &Theme, mode: DetailMode) {
    let Some(tx) = state.selected_tx() else {
        note(frame, area, "No transaction selected.", t);
        return;
    };
    let area = inset(area);
    let width = usize::from(area.width);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{:<14}", "amount"), t.muted),
            Span::styled(
                format!("{} ZEC", signed_full(tx)),
                kind_style(tx.kind, t).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", tx_kind_label(tx.kind)), t.muted),
        ]),
        kv_styled(
            t,
            "status",
            if tx.status.is_confirmed() {
                format!(
                    "confirmed in block {}",
                    format::group(u64::from(zcash_protocol::consensus::BlockHeight::from_u32(
                        u32::from(tx.status.get_height())
                    )))
                )
            } else if tx.status.is_failed() {
                "failed".into()
            } else {
                "pending".into()
            },
            if tx.status.is_confirmed() {
                t.text
            } else {
                status_style(tx.status, t)
            },
        ),
        kv(t, "time", format::datetime(tx.datetime)),
    ];
    if let Some(fee) = tx.fee {
        lines.push(kv(t, "fee", format!("{} ZEC", format::zec_full(fee))));
    }
    lines.push(kv_styled(t, "txid", tx.txid.clone(), t.info));
    lines.push(Line::from(""));
    lines.push(tabs(
        &["Inputs and outputs", "Wallet events"],
        match mode {
            DetailMode::InputsOutputs => 0,
            DetailMode::WalletEvents => 1,
        },
        t,
        false,
    ));
    lines.push(Line::from(""));

    match mode {
        DetailMode::InputsOutputs => {
            lines.extend(inputs_outputs_flow(tx, width, t));
            lines.push(Line::from(""));

            lines.push(Line::styled("Spent from this wallet", t.muted));
            if tx.inputs.is_empty() {
                lines.push(Line::styled("  nothing", t.muted));
            }
            for i in &tx.inputs {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<12}", pool_label(i.pool)), t.pool(i.pool)),
                    Span::styled(format!("{:>20} ZEC", format::zec_full(i.value)), t.text),
                    Span::styled("   from ", t.muted),
                    Span::styled(
                        format!(
                            "{}:{}",
                            format::truncate_middle(&i.source_txid, 18),
                            i.output_index
                        ),
                        t.info,
                    ),
                ];
                if i.pending {
                    spans.push(Span::styled("   spend pending", t.warning));
                }
                lines.push(Line::from(spans));
                push_memo(
                    &mut lines,
                    i.memo.as_deref(),
                    width,
                    t,
                    t.text.add_modifier(Modifier::ITALIC),
                );
            }
            lines.push(Line::from(""));

            let with_role = |role: OutputRole| -> Vec<&OutputRow> {
                tx.outputs.iter().filter(|o| o.role == role).collect()
            };
            let memo_style = t.text.add_modifier(Modifier::ITALIC);
            let mut first = true;
            let mut section = |lines: &mut Vec<Line<'static>>, title: &'static str| {
                if !first {
                    lines.push(Line::from(""));
                }
                first = false;
                lines.push(Line::styled(title, t.muted));
            };

            let received = with_role(OutputRole::Received);
            if !received.is_empty() {
                section(&mut lines, "Received by this wallet");
                for o in received {
                    lines.push(output_line(o, t, false));
                    let spend = match o.spend {
                        SpendStatus::Unspent => t.positive,
                        SpendStatus::Spent(_) => t.muted,
                        SpendStatus::CalculatedSpent(_)
                        | SpendStatus::TransmittedSpent(_)
                        | SpendStatus::MempoolSpent(_) => t.warning,
                    };
                    lines.push(Line::styled(
                        format!("      {}", spend_describe(&o.spend)),
                        spend,
                    ));
                    push_memo(&mut lines, o.memo.as_deref(), width, t, memo_style);
                }
            }

            let sent = with_role(OutputRole::Sent);
            if !sent.is_empty() {
                section(&mut lines, "Sent to others");
                for o in sent {
                    lines.push(output_line(o, t, false));
                    if let Some(address) = &o.address {
                        for l in format::wrap_chars(address, width.saturating_sub(6)) {
                            lines.push(Line::styled(format!("      {l}"), t.info));
                        }
                    }
                    push_memo(&mut lines, o.memo.as_deref(), width, t, memo_style);
                }
            }

            let to_self = with_role(OutputRole::ToSelf);
            if !to_self.is_empty() {
                section(&mut lines, "Sent to this wallet");
                for o in to_self {
                    lines.push(output_line(o, t, false));
                    lines.push(Line::styled(
                        format!("      {}", spend_describe(&o.spend)),
                        t.muted,
                    ));
                    push_memo(&mut lines, o.memo.as_deref(), width, t, memo_style);
                }
            }

            let change = with_role(OutputRole::Change);
            if !change.is_empty() {
                section(&mut lines, "Change");
                for o in change {
                    lines.push(output_line(o, t, true));
                    lines.push(Line::styled(
                        format!("      {}", spend_describe(&o.spend)),
                        t.muted,
                    ));
                    push_memo(
                        &mut lines,
                        o.memo.as_deref(),
                        width,
                        t,
                        t.muted.add_modifier(Modifier::ITALIC),
                    );
                }
            }
        }
        DetailMode::WalletEvents => {
            lines.extend(wallet_event_flow(tx, width, t));
            lines.push(Line::from(""));
            if tx.wallet_events.is_empty() {
                lines.push(Line::styled("no wallet events", t.muted));
            }
            for event in &tx.wallet_events {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<14}", event.kind_label()), event_style(event, t)),
                    Span::styled(format!("{:>20} ZEC", format::zec_full(event.value)), t.text),
                ];
                if !event.pools.is_empty() {
                    let names = event
                        .pools
                        .iter()
                        .map(|pool| pool_label(*pool))
                        .collect::<Vec<_>>()
                        .join(", ");
                    spans.push(Span::styled(format!("   to {names}"), t.muted));
                }
                lines.push(Line::from(spans));
                if let Some(addr) = &event.recipient {
                    for l in format::wrap_chars(addr, width.saturating_sub(6)) {
                        lines.push(Line::styled(format!("      {l}"), t.info));
                    }
                }
                for memo in &event.memos {
                    push_memo(
                        &mut lines,
                        Some(memo),
                        width,
                        t,
                        t.text.add_modifier(Modifier::ITALIC),
                    );
                }
                lines.push(Line::from(""));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.detail_scroll, 0)),
        area,
    );
}

fn signed_full(tx: &TxRow) -> String {
    format!("{}{}", tx_kind_sign(tx.kind), format::zec_full(tx.value))
}

fn box_lines(tx: &TxRow) -> Vec<String> {
    let mut lines = vec![tx_kind_label(tx.kind).to_string()];
    if let Some(fee) = tx.fee {
        lines.push(format!("fee {}", format::zec(fee)));
    }
    lines
}

fn widest(entries: impl Iterator<Item = String>) -> usize {
    entries.map(|e| e.chars().count()).max().unwrap_or(0)
}

fn output_tag(o: &OutputRow) -> &'static str {
    match o.role {
        OutputRole::Received => "received",
        OutputRole::Sent => "sent",
        OutputRole::ToSelf => "self",
        OutputRole::Change => "change",
    }
}

fn rank(o: &OutputRow) -> u8 {
    match o.role {
        OutputRole::Sent => 0,
        OutputRole::ToSelf => 1,
        OutputRole::Received => 2,
        OutputRole::Change => 3,
    }
}

fn paint(rows: Vec<Vec<Piece>>, t: &Theme, left: &[Style], right: &[Style]) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|piece| {
                        let style = match piece.part {
                            Part::Pad => Style::new(),
                            Part::Wire => t.border,
                            Part::Frame => t.accent,
                            Part::Title(0) => t.title,
                            Part::Title(_) => t.muted,
                            Part::Left(i) => left.get(i).copied().unwrap_or(t.text),
                            Part::Right(i) => right.get(i).copied().unwrap_or(t.text),
                            Part::Note => t.muted,
                        };
                        Span::styled(piece.text, style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

fn inputs_outputs_flow(tx: &TxRow, width: usize, t: &Theme) -> Vec<Line<'static>> {
    let pool_w = widest(tx.inputs.iter().map(|i| pool_label(i.pool).to_string()));
    let value_w = widest(tx.inputs.iter().map(|i| format::zec(i.value)));
    let left: Vec<String> = tx
        .inputs
        .iter()
        .map(|i| {
            format!(
                "{:<pool_w$}  {:>value_w$}",
                pool_label(i.pool),
                format::zec(i.value)
            )
        })
        .collect();
    let mut outputs: Vec<&OutputRow> = tx.outputs.iter().collect();
    outputs.sort_by_key(|o| rank(o));
    let pool_w = widest(outputs.iter().map(|o| pool_label(o.pool).to_string()));
    let value_w = widest(outputs.iter().map(|o| format::zec(o.value)));
    let right: Vec<String> = outputs
        .iter()
        .map(|o| {
            format!(
                "{:<value_w$}  {:<pool_w$}  {}",
                format::zec(o.value),
                pool_label(o.pool),
                output_tag(o)
            )
        })
        .collect();
    let left_styles: Vec<Style> = tx.inputs.iter().map(|i| t.pool(i.pool)).collect();
    let right_styles: Vec<Style> = outputs
        .iter()
        .map(|o| {
            if o.role == OutputRole::Change {
                t.muted
            } else {
                t.pool(o.pool)
            }
        })
        .collect();
    let box_lines = box_lines(tx);
    let rows = Flow {
        left: &left,
        right: &right,
        box_lines: &box_lines,
        left_empty: "no wallet inputs",
        right_empty: "no outputs",
    }
    .render_parts(width);
    paint(rows, t, &left_styles, &right_styles)
}

fn wallet_event_flow(tx: &TxRow, width: usize, t: &Theme) -> Vec<Line<'static>> {
    let (incoming, outgoing): (Vec<_>, Vec<_>) = tx
        .wallet_events
        .iter()
        .partition(|event| event.is_received());
    let kind_w = widest(incoming.iter().map(|event| event.kind_label().to_string()));
    let value_w = widest(incoming.iter().map(|event| format::zec(event.value)));
    let left: Vec<String> = incoming
        .iter()
        .map(|event| {
            format!(
                "{:<kind_w$}  {:>value_w$}",
                event.kind,
                format::zec(event.value)
            )
        })
        .collect();
    let value_w = widest(outgoing.iter().map(|event| format::zec(event.value)));
    let right: Vec<String> = outgoing
        .iter()
        .map(|event| format!("{:<value_w$}  {}", format::zec(event.value), event.kind))
        .collect();
    let left_styles: Vec<Style> = incoming.iter().map(|event| event_style(event, t)).collect();
    let right_styles: Vec<Style> = outgoing.iter().map(|event| event_style(event, t)).collect();
    let box_lines = box_lines(tx);
    let rows = Flow {
        left: &left,
        right: &right,
        box_lines: &box_lines,
        left_empty: "nothing received",
        right_empty: "nothing sent",
    }
    .render_parts(width);
    paint(rows, t, &left_styles, &right_styles)
}

fn output_line(o: &OutputRow, t: &Theme, quiet: bool) -> Line<'static> {
    let (pool, value) = if quiet {
        (t.muted, t.muted)
    } else {
        (t.pool(o.pool), t.text)
    };
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(format!("{:<12}", pool_label(o.pool)), pool),
        Span::styled(format!("{:>20} ZEC", format::zec_full(o.value)), value),
        Span::styled(format!("   index {}", o.index), t.muted),
    ];
    if let Some(scope) = o.scope.as_ref().filter(|_| !quiet) {
        spans.push(Span::styled(
            format!("   {}", scope.to_lowercase()),
            t.muted,
        ));
    }
    Line::from(spans)
}

fn push_memo(
    lines: &mut Vec<Line<'static>>,
    memo: Option<&str>,
    width: usize,
    t: &Theme,
    style: Style,
) {
    let Some(memo) = memo.filter(|m| !m.is_empty()) else {
        return;
    };
    lines.push(Line::styled("      memo:", t.muted));
    for raw in memo.lines() {
        for l in format::wrap_chars(raw, width.saturating_sub(8)) {
            lines.push(Line::styled(format!("        {l}"), style));
        }
    }
}
