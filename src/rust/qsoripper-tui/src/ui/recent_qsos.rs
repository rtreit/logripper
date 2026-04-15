//! Recent QSOs history panel with embedded search / filter bar.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::app::App;

/// Render the recent QSOs panel (with optional embedded search row) into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_qsos();
    let show_search = app.search_focused || !app.search_text.is_empty();

    let border_color = if app.search_focused {
        Color::Magenta
    } else if app.qso_list_focused {
        Color::Yellow
    } else {
        Color::Cyan
    };

    let total = app.recent_qsos.len();
    let matched = filtered.len();
    let count_str = if app.search_text.is_empty() {
        format!("{total} QSOs")
    } else {
        format!("{matched}/{total}")
    };

    let title = if app.qso_list_focused {
        format!(" QSOs  {count_str}  \u{2191}\u{2193} navigate  Enter load  D delete  Esc exit ")
    } else if app.search_focused {
        format!(" QSOs  {count_str}  Esc clear  \u{2193}/Enter\u{2192}list ")
    } else if !app.search_text.is_empty() {
        format!(" QSOs  {count_str}  F4 edit filter  F3 focus ")
    } else {
        format!(" QSOs  {count_str}  F3 focus  F4 search ")
    };

    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (search_area, table_area) = if show_search && inner.height > 2 {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
        (
            split.first().copied(),
            split.get(1).copied().unwrap_or(inner),
        )
    } else {
        (None, inner)
    };

    if let Some(sa) = search_area {
        render_search_row(app, frame, sa);
    }

    render_table(app, &filtered, frame, table_area);
}

fn render_search_row(app: &App, frame: &mut Frame, area: Rect) {
    let max_text = (area.width as usize).saturating_sub(10);
    let chars: Vec<char> = app.search_text.chars().collect();
    let visible: String = if chars.len() > max_text {
        chars
            .get(chars.len() - max_text..)
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    } else {
        app.search_text.clone()
    };

    let mut spans = vec![
        Span::styled("Search: ", Style::default().fg(Color::Magenta)),
        Span::styled(
            visible,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if app.search_focused {
        spans.push(Span::styled("|", Style::default().fg(Color::Yellow)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_table(app: &App, filtered: &[&crate::app::RecentQso], frame: &mut Frame, area: Rect) {
    let header_cells = [
        "UTC",
        "Callsign",
        "Band",
        "Mode",
        "RST\u{2191}",
        "RST\u{2193}",
        "Country",
        "Grid",
    ]
    .iter()
    .map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    });

    let header = Row::new(header_cells).height(1);

    let rows = filtered.iter().enumerate().map(|(i, qso)| {
        let row_style = if i % 2 == 0 {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        Row::new(vec![
            Cell::from(qso.utc.as_str()),
            Cell::from(qso.callsign.as_str()).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(qso.band.as_str()),
            Cell::from(qso.mode.as_str()),
            Cell::from(qso.rst_sent.as_str()),
            Cell::from(qso.rst_rcvd.as_str()),
            Cell::from(qso.country.as_deref().unwrap_or("")),
            Cell::from(qso.grid.as_deref().unwrap_or("")),
        ])
        .style(row_style)
        .height(1)
    });

    let widths = [
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Fill(1),
        Constraint::Length(7),
    ];

    let highlight_style = if app.qso_list_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(highlight_style);

    let mut state = TableState::default();
    state.select(if app.qso_list_focused {
        app.qso_selected
    } else {
        None
    });

    frame.render_stateful_widget(table, area, &mut state);
}
