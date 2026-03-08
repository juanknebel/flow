use std::io;

use clap::{Parser, Subcommand};

use crate::format::{self, Format};
use crate::provider;
use crate::store_fs;

#[derive(Parser)]
#[command(
    name = "flow",
    about = "A terminal Kanban board with CLI and TUI modes.",
    long_about = "\
A terminal Kanban board with CLI and TUI modes.

Without a subcommand, launches the interactive TUI (terminal UI).
With a subcommand, performs the action and prints the result to stdout.

BOARD MODEL:
  A board has ordered columns. Each column has an id, a title, and cards.
  Each card has an id (e.g. FLOW-1), a title, and an optional description.
  Cards live in exactly one column and can be moved between them.

TYPICAL WORKFLOW:
  1. flow columns              List available column ids
  2. flow list                 See all cards grouped by column
  3. flow show <card_id>       Read a card's full description
  4. flow create <col> \"title\" Create a card in a column
  5. flow edit <card_id> ...   Update a card's title or body
  6. flow move <card_id> <col> Move a card to another column

OUTPUT:
  All subcommands write to stdout. Use -f to choose the format:
    plain (default), json, xml, csv, table, markdown.
  Errors go to stderr with a non-zero exit code.

PROVIDERS:
  The board backend is selected via environment variables:
    FLOW_PROVIDER=local   Local filesystem (default). Board path set by
                          FLOW_BOARD_PATH or defaults to ~/.config/flow/boards/default.
    FLOW_PROVIDER=jira    Jira Cloud. Requires JIRA_BASE_URL, JIRA_EMAIL,
                          JIRA_API_TOKEN, and JIRA_BOARD_ID.
  Without FLOW_PROVIDER, a built-in demo board is used.

EXAMPLES:
  flow list -f json
  flow columns -f csv
  flow show FLOW-1
  flow create todo \"Fix login bug\" --body \"Users report 500 on /login\"
  flow edit FLOW-1 --title \"Updated title\"
  flow move FLOW-1 done"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Output format: plain, json, xml, csv, table, markdown
    #[arg(long, short = 'f', default_value = "plain", global = true)]
    pub format: Format,
}

#[derive(Subcommand)]
pub enum Command {
    /// List all columns with their cards (id and title per card)
    List,

    /// Show full card details: id, title, description, and column
    Show {
        /// Card identifier (e.g. FLOW-1)
        card_id: String,
    },

    /// Move a card to a different column
    Move {
        /// Card identifier to move
        card_id: String,
        /// Target column id (use `flow columns` to list them)
        column_id: String,
    },

    /// Create a new card in the given column
    Create {
        /// Target column id
        column_id: String,
        /// Card title (defaults to "New card" if omitted)
        title: Option<String>,
        /// Card body / description
        #[arg(long)]
        body: Option<String>,
    },

    /// Update a card's title and/or body (at least one required)
    Edit {
        /// Card identifier to edit
        card_id: String,
        /// New title (keeps current if omitted)
        #[arg(long)]
        title: Option<String>,
        /// New body / description (keeps current if omitted)
        #[arg(long)]
        body: Option<String>,
    },

    /// Delete a card permanently
    Delete {
        /// Card identifier to delete
        card_id: String,
    },

    /// List column ids, titles, and card counts
    Columns,
}

pub fn run(cmd: Command, fmt: Format) -> io::Result<()> {
    let mut prov = provider::from_env();

    match cmd {
        Command::List => {
            let board = prov
                .load_board()
                .map_err(|e| io::Error::other(e.to_string()))?;
            println!("{}", format::format_board(&board, fmt));
        }
        Command::Show { card_id } => {
            let board = prov
                .load_board()
                .map_err(|e| io::Error::other(e.to_string()))?;
            let (col, card) = find_card(&board, &card_id)?;
            println!(
                "{}",
                format::format_card(card, &col.id, &col.title, fmt)
            );
        }
        Command::Move {
            card_id,
            column_id,
        } => {
            prov.move_card(&card_id, &column_id)
                .map_err(|e| io::Error::other(e.to_string()))?;
            println!(
                "{}",
                format::format_result(
                    &[
                        ("action", "move"),
                        ("card_id", &card_id),
                        ("column_id", &column_id),
                    ],
                    fmt,
                )
            );
        }
        Command::Create {
            column_id,
            title,
            body,
        } => {
            let card_id = prov
                .create_card(&column_id)
                .map_err(|e| io::Error::other(e.to_string()))?;

            if title.is_some() || body.is_some() {
                let path = prov
                    .card_path(&card_id)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                let t = title.as_deref().unwrap_or("New card");
                let b = body.as_deref().unwrap_or("");
                store_fs::write_card_content(&path, t, b)?;
            }

            println!(
                "{}",
                format::format_result(
                    &[
                        ("action", "create"),
                        ("card_id", &card_id),
                        ("column_id", &column_id),
                    ],
                    fmt,
                )
            );
        }
        Command::Edit {
            card_id,
            title,
            body,
        } => {
            if title.is_none() && body.is_none() {
                return Err(io::Error::other(
                    "edit requires at least --title or --body",
                ));
            }

            let path = prov
                .card_path(&card_id)
                .map_err(|e| io::Error::other(e.to_string()))?;

            let (cur_title, cur_body) = store_fs::read_card_content(&path)?;
            let t = title.as_deref().unwrap_or(&cur_title);
            let b = body.as_deref().unwrap_or(&cur_body);
            store_fs::write_card_content(&path, t, b)?;

            println!(
                "{}",
                format::format_result(
                    &[("action", "edit"), ("card_id", &card_id)],
                    fmt,
                )
            );
        }
        Command::Delete { card_id } => {
            prov.delete_card(&card_id)
                .map_err(|e| io::Error::other(e.to_string()))?;
            println!(
                "{}",
                format::format_result(
                    &[("action", "delete"), ("card_id", &card_id)],
                    fmt,
                )
            );
        }
        Command::Columns => {
            let board = prov
                .load_board()
                .map_err(|e| io::Error::other(e.to_string()))?;
            println!("{}", format::format_columns(&board, fmt));
        }
    }

    Ok(())
}

fn find_card<'a>(
    board: &'a crate::model::Board,
    card_id: &str,
) -> io::Result<(&'a crate::model::Column, &'a crate::model::Card)> {
    for col in &board.columns {
        if let Some(card) = col.cards.iter().find(|c| c.id == card_id) {
            return Ok((col, card));
        }
    }
    Err(io::Error::other(format!("card not found: {card_id}")))
}
