use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use crossterm::event::KeyCode;

use crate::app::{Action, App};

pub fn help_text() -> &'static str {
    "h/l or ←/→ focus  j/k or ↑/↓ select  H/L move  n new  e edit  d delete  Enter detail  r refresh  Esc close/quit  q quit"
}

pub fn action_from_key(code: KeyCode) -> Option<Action> {
    Some(match code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::CloseOrQuit,

        KeyCode::Char('h') | KeyCode::Left => Action::FocusLeft,
        KeyCode::Char('l') | KeyCode::Right => Action::FocusRight,

        KeyCode::Char('j') | KeyCode::Down => Action::SelectDown,
        KeyCode::Char('k') | KeyCode::Up => Action::SelectUp,

        KeyCode::Char('H') => Action::MoveLeft,
        KeyCode::Char('L') => Action::MoveRight,

        KeyCode::Enter => Action::ToggleDetail,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('d') => Action::Delete,

        _ => return None,
    })
}

pub fn render(f: &mut Frame, app: &App) {
    let chunks = if app.banner.is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(f.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(f.area())
    };

    let (banner_area, main, help) = if app.banner.is_some() {
        (Some(chunks[0]), chunks[1], chunks[2])
    } else {
        (None, chunks[0], chunks[1])
    };

    if let (Some(a), Some(text)) = (banner_area, app.banner.as_deref()) {
        f.render_widget(
            Paragraph::new(Span::styled(text, Style::default().fg(Color::Yellow))),
            a,
        );
    }

    if app.board.columns.is_empty() {
        f.render_widget(
            Paragraph::new("No columns found. Check board.txt.")
                .block(Block::default().borders(Borders::ALL)),
            main,
        );
    } else {
        let rects = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, app.board.columns.len() as u32);
                app.board.columns.len()
            ])
            .split(main);

        for (i, r) in rects.iter().enumerate() {
            draw_col(f, app, i, *r);
        }
    }

    f.render_widget(
        Paragraph::new(help_text()).block(Block::default().borders(Borders::TOP)),
        help,
    );

    if app.detail_open {
        let Some(col) = app.board.columns.get(app.col) else {
            return;
        };
        let Some(card) = col.cards.get(app.row) else {
            return;
        };

        let area = centered(70, 45, f.area());
        f.render_widget(Clear, area);

        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            &card.id,
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(card.title.clone()));
        lines.push(Line::from(""));

        if card.description.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "No description",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for l in card.description.lines() {
                lines.push(Line::from(l.to_string()));
            }
        }

        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .title("Detail")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
    }

    if app.confirm_delete {
        let area = centered(40, 20, f.area());
        f.render_widget(Clear, area);

        let card_id = selected_card_id(app).unwrap_or_else(|| "Unknown".to_string());
        let text = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("Delete card "),
                Span::styled(&card_id, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("?"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("es / "),
                Span::styled("n", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw("o"),
            ]),
        ];

        f.render_widget(
            Paragraph::new(text)
                .alignment(ratatui::layout::Alignment::Center)
                .block(
                    Block::default()
                        .title("Confirm Delete")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Red)),
                ),
            area,
        );
    }
}

pub fn draw_col(f: &mut Frame, app: &App, idx: usize, rect: Rect) {
    let col = &app.board.columns[idx];
    let focused = idx == app.col;

    let border = if focused { Color::Cyan } else { Color::Gray };

    let items: Vec<ListItem> = col
        .cards
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::styled(&c.id, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::raw(c.title.clone()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!("{} ({})", col.title, col.cards.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    if focused && !col.cards.is_empty() {
        state.select(Some(app.row.min(col.cards.len() - 1)));
    }

    f.render_stateful_widget(list, rect, &mut state);
}

pub fn centered(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
}

fn selected_card_id(app: &App) -> Option<String> {
    app.board
        .columns
        .get(app.col)
        .and_then(|col| col.cards.get(app.row))
        .map(|card| card.id.clone())
}
