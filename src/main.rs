use std::{
    collections::VecDeque,
    io, panic,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
};

use clap::Parser;
use flow::{
    App, Action, Board,
    app::{EditState, EditFocus},
    cli, provider,
    model::Priority,
    ui::{render, action_from_key}
};

fn main() -> io::Result<()> {
    let args = cli::Cli::parse();

    if let Some(cmd) = args.command {
        return cli::run(cmd, args.format);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_tui(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn run_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut provider = provider::from_env();

    let board = match provider.load_board() {
        Ok(b) => b,
        Err(e) => {
            let mut app = App::new(Board { columns: vec![] });
            app.banner = Some(format!("Load failed: {e}"));
            loop {
                terminal.draw(|f| render(f, &app, None))?;
                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(k) = event::read()? {
                        if k.kind == KeyEventKind::Press
                            && matches!(k.code, crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc)
                        {
                            break;
                        }
                    }
                }
            }
            return Ok(());
        }
    };

    let mut app = App::new(board);
    app.focus_first_non_empty();
    type MoveOutcome = Result<Option<Board>, String>;
    let mut move_rx: Option<Receiver<MoveOutcome>> = None;
    let mut move_queue: VecDeque<(String, String)> = VecDeque::new();
    const MAX_QUEUE_SIZE: usize = 64;
    let mut quitting = false;

    loop {
        if let Some(rx) = move_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(Some(board))) => {
                    app.board = board;
                    app.clamp();
                    app.banner = Some(
                        "Move failed: reloaded board (optimistic state corrected)".to_string(),
                    );
                    move_queue.clear(); // Drop queued moves after a failure to avoid compounding errors.
                    move_rx = None;
                    update_quit_banner(&mut app, quitting, &move_queue, move_rx.is_some());
                }
                Ok(Ok(None)) => {
                    move_rx = None;
                    if let Some((card_id, dst)) = move_queue.pop_front() {
                        move_rx = Some(spawn_move(card_id, dst));
                        app.banner = Some(format!("Moving... ({} queued)", move_queue.len()));
                    } else {
                        app.banner = None;
                    }
                    update_quit_banner(&mut app, quitting, &move_queue, move_rx.is_some());
                }
                Ok(Err(msg)) => {
                    app.banner = Some(format!("Move failed: {msg}"));
                    move_queue.clear();
                    move_rx = None;
                    update_quit_banner(&mut app, quitting, &move_queue, move_rx.is_some());
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    app.banner = Some("Move failed: worker disconnected".to_string());
                    move_rx = None;
                    update_quit_banner(&mut app, quitting, &move_queue, move_rx.is_some());
                }
            }
        }

        if quitting && move_rx.is_none() && move_queue.is_empty() {
            return Ok(());
        }

        terminal.draw(|f| render(f, &app, None))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    if let Some(edit) = app.edit_state.as_mut() {
                        match k.code {
                            crossterm::event::KeyCode::Esc => {
                                app.edit_state = None;
                            }
                            crossterm::event::KeyCode::Tab => {
                                edit.focus = edit.focus.next();
                                if edit.focus != EditFocus::Priority {
                                    edit.cursor_pos = edit.current_text().len();
                                }
                            }
                            crossterm::event::KeyCode::Enter => {
                                let card_id = edit.card_id.clone();
                                let title = edit.title.clone();
                                let description = edit.description.clone();
                                let priority = edit.priority;
                                if let Err(e) = provider.update_card(&card_id, &title, &description, priority) {
                                    app.banner = Some(format!("Save failed: {e}"));
                                } else {
                                    match provider.load_board() {
                                        Ok(b) => {
                                            app.board = b;
                                            focus_card_by_id(&mut app, &card_id);
                                            app.banner = Some("Card saved".to_string());
                                        }
                                        Err(e) => app.banner = Some(format!("Reload failed: {e}")),
                                    }
                                }
                                app.edit_state = None;
                            }
                            crossterm::event::KeyCode::Char(c) => {
                                edit.insert_char(c);
                            }
                            crossterm::event::KeyCode::Backspace => {
                                edit.delete_prev();
                            }
                            crossterm::event::KeyCode::Delete => {
                                edit.delete_curr();
                            }
                            crossterm::event::KeyCode::Left => {
                                edit.move_cursor_left();
                            }
                            crossterm::event::KeyCode::Right => {
                                edit.move_cursor_right();
                            }
                            crossterm::event::KeyCode::Home => {
                                if edit.focus != EditFocus::Priority {
                                    edit.cursor_pos = 0;
                                }
                            }
                            crossterm::event::KeyCode::End => {
                                if edit.focus != EditFocus::Priority {
                                    edit.cursor_pos = edit.current_text().len();
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.confirm_delete {
                        match k.code {
                            crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                                if let Some(card_id) = selected_card_id(&app) {
                                    if let Err(e) = provider.delete_card(&card_id) {
                                        app.banner = Some(format!("Delete failed: {e}"));
                                    } else {
                                        match provider.load_board() {
                                            Ok(b) => {
                                                app.board = b;
                                                app.clamp();
                                                app.banner = Some(format!("Card {card_id} deleted"));
                                            }
                                            Err(e) => {
                                                app.banner = Some(format!("Reload failed: {e}"))
                                            }
                                        }
                                    }
                                }
                                app.confirm_delete = false;
                            }
                            crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') | crossterm::event::KeyCode::Esc => {
                                app.confirm_delete = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if let Some(a) = action_from_key(k.code) {
                        if quitting {
                            if matches!(a, Action::MoveLeft | Action::MoveRight) {
                                continue;
                            }
                        }

                        match a {
                            Action::Add => {
                                if quitting {
                                    continue;
                                }
                                let Some(col) = app.board.columns.get(app.col) else {
                                    app.banner = Some("Create failed: no column selected".to_string());
                                    continue;
                                };
                                match provider.create_card(&col.id) {
                                    Ok(id) => {
                                        app.edit_state = Some(EditState {
                                            card_id: id,
                                            title: "New card".to_string(),
                                            description: "".to_string(),
                                            priority: Priority::Medium,
                                            cursor_pos: 8,
                                            focus: EditFocus::Title,
                                        });
                                    }
                                    Err(e) => {
                                        app.banner = Some(format!("Create failed: {e}"));
                                    }
                                }
                            }
                            Action::Delete => {
                                if !app.board.columns.is_empty() && !app.board.columns[app.col].cards.is_empty() {
                                    app.confirm_delete = true;
                                }
                            }
                            Action::Edit => {
                                if quitting {
                                    continue;
                                }
                                let Some(col) = app.board.columns.get(app.col) else { continue; };
                                let Some(card) = col.cards.get(app.row) else {
                                    app.banner = Some("Edit failed: no card selected".to_string());
                                    continue;
                                };
                                app.edit_state = Some(EditState {
                                    card_id: card.id.clone(),
                                    title: card.title.clone(),
                                    description: card.description.clone(),
                                    priority: card.priority,
                                    cursor_pos: card.title.len(),
                                    focus: EditFocus::Title,
                                });
                            }
                            Action::MoveLeft => {
                                if move_rx.is_some() {
                                    if move_queue.len() >= MAX_QUEUE_SIZE {
                                        app.banner = Some(
                                            "Move queue full — too many pending moves".to_string(),
                                        );
                                    } else if let Some((card_id, dst)) = app.optimistic_move(-1) {
                                        move_queue.push_back((card_id, dst));
                                        app.banner = Some(format!(
                                            "Moving... ({} queued)",
                                            move_queue.len()
                                        ));
                                    }
                                } else if let Some((card_id, dst)) = app.optimistic_move(-1) {
                                    move_rx = Some(spawn_move(card_id, dst));
                                    app.banner = Some("Moving...".to_string());
                                }
                            }
                            Action::MoveRight => {
                                if move_rx.is_some() {
                                    if move_queue.len() >= MAX_QUEUE_SIZE {
                                        app.banner = Some(
                                            "Move queue full — too many pending moves".to_string(),
                                        );
                                    } else if let Some((card_id, dst)) = app.optimistic_move(1) {
                                        move_queue.push_back((card_id, dst));
                                        app.banner = Some(format!(
                                            "Moving... ({} queued)",
                                            move_queue.len()
                                        ));
                                    }
                                } else if let Some((card_id, dst)) = app.optimistic_move(1) {
                                    move_rx = Some(spawn_move(card_id, dst));
                                    app.banner = Some("Moving...".to_string());
                                }
                            }
                            Action::Refresh => {
                                if quitting {
                                    continue;
                                }
                                match provider.load_board() {
                                    Ok(b) => {
                                        app.board = b;
                                        app.focus_first_non_empty();
                                        app.banner = None;
                                    }
                                    Err(e) => app.banner = Some(format!("Refresh failed: {e}")),
                                }
                            }
                            _ => {
                                if app.apply(a) {
                                    if move_rx.is_some() || !move_queue.is_empty() {
                                        quitting = true;
                                        update_quit_banner(
                                            &mut app,
                                            quitting,
                                            &move_queue,
                                            move_rx.is_some(),
                                        );
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn selected_card_id(app: &App) -> Option<String> {
    app.board
        .columns
        .get(app.col)
        .and_then(|col| col.cards.get(app.row))
        .map(|card| card.id.clone())
}

fn focus_card_by_id(app: &mut App, card_id: &str) {
    for (col_idx, col) in app.board.columns.iter().enumerate() {
        if let Some(row_idx) = col.cards.iter().position(|c| c.id == card_id) {
            app.col = col_idx;
            app.row = row_idx;
            app.clamp();
            return;
        }
    }
    app.focus_first_non_empty();
}

fn update_quit_banner(
    app: &mut App,
    quitting: bool,
    move_queue: &VecDeque<(String, String)>,
    move_in_flight: bool,
) {
    if !quitting {
        return;
    }
    let pending = move_queue.len() + if move_in_flight { 1 } else { 0 };
    app.banner = if pending == 0 {
        None
    } else {
        Some(format!("Finishing {pending} pending moves before quit..."))
    };
}

fn spawn_move(card_id: String, dst: String) -> Receiver<Result<Option<Board>, String>> {
    let (tx, rx) = mpsc::channel::<Result<Option<Board>, String>>();
    thread::spawn(move || {
        let res = panic::catch_unwind(|| {
            let mut p = provider::from_env();
            match p.move_card(&card_id, &dst) {
                Ok(()) => {
                    let _ = tx.send(Ok(None));
                }
                Err(move_err) => match p.load_board() {
                    Ok(board) => {
                        let _ = tx.send(Ok(Some(board)));
                    }
                    Err(_) => {
                        let _ = tx.send(Err(move_err.to_string()));
                    }
                },
            }
        });
        if res.is_err() {
            let _ = tx.send(Err("worker panicked".to_string()));
        }
    });
    rx
}
