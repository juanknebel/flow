# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`flow` is a keyboard-first terminal Kanban board built in Rust. It has two provider backends: local filesystem and Jira. The TUI uses ratatui/crossterm.

## Build and run

```bash
cargo build
cargo run              # runs in demo/local mode with boards/demo/
cargo test             # run all tests
cargo test app::tests  # run tests in a specific module
```

## Environment variables

- `FLOW_PROVIDER=local|jira` — selects backend (default: local/demo)
- `FLOW_BOARD_PATH=/path` — overrides board directory for local mode
- Jira mode requires: `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN`, `JIRA_BOARD_ID`

## Architecture

Single-crate Rust project (edition 2024). All source is in `src/`.

- **`model.rs`** — Core data types: `Board`, `Column`, `Card`. No logic, just structs.
- **`provider.rs`** — `Provider` trait (load_board, move_card, create_card, card_path) and `from_env()` factory that selects the backend from env vars.
- **`provider_local.rs`** — Local filesystem provider. Delegates to `store_fs` for all I/O.
- **`provider_jira.rs`** — Jira REST API provider using `reqwest` blocking client. Handles board config, transitions, and ADF description parsing.
- **`store_fs.rs`** — Filesystem operations for the local board format (`board.txt`, `cols/<id>/order.txt`, `cols/<id>/<CARD>.md`).
- **`app.rs`** — UI state machine (`App`) with `Action` enum. Handles cursor movement, optimistic card moves, detail toggle. All navigation logic is tested here.
- **`main.rs`** — TUI event loop and rendering. Wires provider, app state, key events, and async move workers together. Card moves run on background threads with an optimistic queue.

## Key patterns

- **Optimistic moves**: Card moves update the UI immediately. A background thread performs the actual provider operation. On failure, the board reloads to correct state.
- **Move queue**: Rapid moves queue up (max 64). The queue drains sequentially, one background move at a time.
- **Provider is created fresh per background move** (`provider::from_env()` in `spawn_move`) since providers aren't `Send`.

## Task management

This project uses `flow` itself as its Kanban task board. The project board lives in `./.board` (gitignored).

```bash
FLOW_BOARD_PATH=./.board flow list             # list all cards
FLOW_BOARD_PATH=./.board flow columns          # list columns
FLOW_BOARD_PATH=./.board flow show <card_id>   # show card details
FLOW_BOARD_PATH=./.board flow create <col> "title" --priority high
FLOW_BOARD_PATH=./.board flow move <card_id> <col>
FLOW_BOARD_PATH=./.board flow edit <card_id> --title "new title"
FLOW_BOARD_PATH=./.board flow delete <card_id>
```

Use `flow` directly (not `cargo run --`) when the binary is installed. Use this board to track tasks, bugs, and features for the project.

## Commit style

Conventional commits with optional scope: `feat(jira):`, `fix:`, `refactor(local):`, `chore:`.
