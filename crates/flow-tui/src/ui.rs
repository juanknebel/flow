use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use crossterm::event::KeyCode;

use crate::app::{Action, App, EditFocus, SearchState};
use flow_core::model::Priority;

fn priority_color(p: Priority) -> Color {
    match p {
        Priority::Bug => Color::Red,
        Priority::High => Color::Yellow,
        Priority::Medium => Color::White,
        Priority::Low => Color::DarkGray,
        Priority::Wishlist => Color::Cyan,
    }
}

pub fn help_text(app: &App) -> String {
    let filter_info = if app.project_filter.is_empty() {
        String::new()
    } else {
        format!(" [{}]", app.project_filter.join(","))
    };
    format!(
        "h/l ←/→ focus  j/k ↑/↓ select  H/L move  a/n new  e edit  d del  Enter detail  r refresh  s sort({})  / search  p project{}  Esc/q quit",
        app.sort_order.label(),
        filter_info,
    )
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
        KeyCode::Char('a') | KeyCode::Char('n') => Action::Add,
        KeyCode::Char('e') => Action::Edit,
        KeyCode::Char('s') => Action::ToggleSort,
        KeyCode::Char('/') => Action::Search,
        KeyCode::Char('p') => Action::ProjectFilter,

        _ => return None,
    })
}

pub fn render(f: &mut Frame, app: &App, render_area: Option<Rect>) {
    let chunks = if app.banner.is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(render_area.unwrap_or_else(|| f.area()))
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(render_area.unwrap_or_else(|| f.area()))
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

    if let Some(search) = &app.search_state {
        let matches = app.search_matches();
        let match_info = if search.query.is_empty() {
            String::new()
        } else {
            format!("  ({} matches)", matches.len())
        };
        let search_line = Line::from(vec![
            Span::styled("/", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(&search.query),
            Span::styled(&match_info, Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(
            Paragraph::new(search_line).block(Block::default().borders(Borders::TOP)),
            help,
        );
        f.set_cursor_position((
            help.x + 1 + search.cursor_pos as u16,
            help.y + 1,
        ));
    } else {
        f.render_widget(
            Paragraph::new(help_text(app)).block(Block::default().borders(Borders::TOP)),
            help,
        );
    }

    if app.detail_open {
        let Some(col) = app.board.columns.get(app.col) else {
            return;
        };
        let Some(card) = col.cards.get(app.row) else {
            return;
        };

        let area = centered(70, 45, render_area.unwrap_or_else(|| f.area()));
        f.render_widget(Clear, area);

        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            &card.id,
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::raw("Priority: "),
            Span::styled(card.priority.label(), Style::default().fg(priority_color(card.priority))),
        ]));
        if !card.project.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("Project: "),
                Span::styled(&card.project, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            ]));
        }
        if !card.assignee.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("Assignee: "),
                Span::styled(&card.assignee, Style::default().add_modifier(Modifier::BOLD)),
            ]));
        }
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
        let area = centered(40, 20, render_area.unwrap_or_else(|| f.area()));
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

    if let Some(edit) = &app.edit_state {
        let area = centered(70, 65, f.area());
        f.render_widget(Clear, area);

        let modal_title = if edit.is_new { "New Card" } else { "Edit Card" };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(modal_title)
            .border_style(Style::default().fg(Color::Cyan));
        let inner_area = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // 0: card id
                Constraint::Length(3),  // 1: title
                Constraint::Length(3),  // 2: project
                Constraint::Length(3),  // 3: priority
                Constraint::Length(3),  // 4: assignee
                Constraint::Min(1),    // 5: description
                Constraint::Length(1),  // 6: help
            ])
            .split(inner_area);

        let header_line = if edit.is_new {
            Line::from(Span::styled("New card", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
        } else {
            Line::from(vec![
                Span::raw("Editing "),
                Span::styled(&edit.card_id, Style::default().add_modifier(Modifier::BOLD)),
            ])
        };
        f.render_widget(
            Paragraph::new(header_line).alignment(ratatui::layout::Alignment::Center),
            chunks[0],
        );

        let title_style = if edit.focus == EditFocus::Title {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(edit.title.clone())
                .block(Block::default().title("Title").borders(Borders::ALL).border_style(title_style)),
            chunks[1],
        );

        // Project field
        let project_style = if edit.focus == EditFocus::Project {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(edit.project.clone())
                .block(Block::default().title("Project").borders(Borders::ALL).border_style(project_style)),
            chunks[2],
        );

        // Priority selector
        let prio_style = if edit.focus == EditFocus::Priority {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        let prio_spans = vec![
            Span::raw(" "),
            Span::styled(
                edit.priority.label(),
                Style::default().fg(priority_color(edit.priority)).add_modifier(Modifier::BOLD),
            ),
            if edit.focus == EditFocus::Priority {
                Span::styled("  ←/→ to change", Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ];
        f.render_widget(
            Paragraph::new(Line::from(prio_spans))
                .block(Block::default().title("Priority").borders(Borders::ALL).border_style(prio_style)),
            chunks[3],
        );

        // Assignee field
        let assignee_style = if edit.focus == EditFocus::Assignee {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(edit.assignee.clone())
                .block(Block::default().title("Assignee").borders(Borders::ALL).border_style(assignee_style)),
            chunks[4],
        );

        let desc_style = if edit.focus == EditFocus::Description {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        let inner_width = chunks[5].width.saturating_sub(2) as usize;
        let wrapped_desc = wrap_text(&edit.description, inner_width);

        f.render_widget(
            Paragraph::new(wrapped_desc.join("\n"))
                .block(Block::default().title("Description").borders(Borders::ALL).border_style(desc_style)),
            chunks[5],
        );

        f.render_widget(
            Paragraph::new("Tab: switch field  \u{2190}/\u{2192}: priority  Enter: save  Esc: cancel")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center),
            chunks[6],
        );

        // Position cursor
        match edit.focus {
            EditFocus::Title => {
                f.set_cursor_position((
                    chunks[1].x + 1 + edit.cursor_pos as u16,
                    chunks[1].y + 1,
                ));
            }
            EditFocus::Project => {
                f.set_cursor_position((
                    chunks[2].x + 1 + edit.cursor_pos as u16,
                    chunks[2].y + 1,
                ));
            }
            EditFocus::Assignee => {
                f.set_cursor_position((
                    chunks[4].x + 1 + edit.cursor_pos as u16,
                    chunks[4].y + 1,
                ));
            }
            EditFocus::Description => {
                let (x, y) = calculate_visual_cursor_pos(&edit.description, edit.cursor_pos, inner_width);
                f.set_cursor_position((
                    chunks[5].x + 1 + x as u16,
                    chunks[5].y + 1 + y as u16,
                ));
            }
            EditFocus::Priority => {
                // No text cursor for priority field
            }
        }
    }

    // Project filter modal
    if let Some(pf) = &app.project_filter_state {
        let area = centered(50, 50, f.area());
        f.render_widget(Clear, area);

        let mut items: Vec<ListItem> = Vec::new();
        for (i, proj_name) in pf.projects.iter().enumerate() {
            let check = if pf.selected[i] { "[x]" } else { "[ ]" };
            let label = if proj_name.is_empty() { "(sin proyecto)" } else { proj_name.as_str() };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{check} "), Style::default().fg(Color::Cyan)),
                Span::raw(label),
            ])));
        }

        let list = List::new(items)
            .block(
                Block::default()
                    .title("Project Filter")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = ListState::default();
        state.select(Some(pf.cursor));
        f.render_stateful_widget(list, area, &mut state);
    }
}

pub fn draw_col(f: &mut Frame, app: &App, idx: usize, rect: Rect) {
    let col = &app.board.columns[idx];
    let focused = idx == app.col;

    let border = if focused { Color::Cyan } else { Color::Gray };

    let searching = app.search_state.as_ref().map_or(false, |s| !s.query.is_empty());

    let items: Vec<ListItem> = col
        .cards
        .iter()
        .map(|c| {
            let dimmed = searching && !SearchState::matches_card(c, &app.search_state.as_ref().unwrap().query);
            let prio_style = if dimmed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(priority_color(c.priority))
            };
            let title_style = if dimmed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let proj_style = if dimmed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Magenta)
            };
            let mut spans = vec![
                Span::styled(format!("[{}] ", c.priority.short_label()), prio_style),
            ];
            if !c.project.is_empty() {
                spans.push(Span::styled(format!("{} ", c.project), proj_style));
            }
            spans.push(Span::styled(c.title.clone(), title_style));
            ListItem::new(Line::from(spans))
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

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for line in text.lines().chain(if text.ends_with('\n') { Some("") } else { None }) {
        if line.is_empty() {
            lines.push("".to_string());
            continue;
        }
        let mut current_line = String::new();
        for word in line.split_inclusive(' ') {
            if current_line.len() + word.len() > width && !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
            }
            // If the word itself is too long, we must break it
            let mut remaining_word = word;
            while remaining_word.len() > width {
                let (part, rest) = remaining_word.split_at(width);
                lines.push(part.to_string());
                remaining_word = rest;
            }
            current_line.push_str(remaining_word);
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    if lines.is_empty() {
        lines.push("".to_string());
    }
    lines
}

fn calculate_visual_cursor_pos(text: &str, cursor_pos: usize, width: usize) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }

    let mut current_offset = 0;
    let mut y = 0;

    for line in text.split_inclusive('\n') {
        let line_len = line.len();

        if cursor_pos >= current_offset && cursor_pos <= current_offset + line_len {
            // Found the hard line where the cursor is.
            let mut x = 0;
            let mut line_y = y;
            let mut current_pos_in_line = 0;
            let target_pos_in_line = cursor_pos - current_offset;

            for word in line.split_inclusive(' ') {
                let word_len = word.len();

                if x + word_len > width && x > 0 {
                    line_y += 1;
                    x = 0;
                }

                if target_pos_in_line >= current_pos_in_line && target_pos_in_line <= current_pos_in_line + word_len {
                    // Cursor is in this word
                    let delta = target_pos_in_line - current_pos_in_line;

                    let mut remaining_delta = delta;
                    let mut temp_x = x;
                    let mut temp_y = line_y;

                    while remaining_delta > width - temp_x && width > 0 {
                        let can_fit = width - temp_x;
                        remaining_delta -= can_fit;
                        temp_y += 1;
                        temp_x = 0;
                    }
                    return (temp_x + remaining_delta, temp_y);
                }

                let mut remaining_word_len = word_len;
                while remaining_word_len > width - x && width > 0 {
                    let can_fit = width - x;
                    remaining_word_len -= can_fit;
                    line_y += 1;
                    x = 0;
                }
                x += remaining_word_len;
                current_pos_in_line += word_len;
            }

            return (x, line_y);
        }

        // Count how many soft lines this hard line takes
        let wrapped = wrap_text(line.trim_end_matches('\n'), width);
        y += wrapped.len().max(1);
        current_offset += line_len;
    }

    (0, y)
}
