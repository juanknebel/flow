# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`flow` is a keyboard-first terminal Kanban board built in Rust. It has two provider backends: local filesystem and Jira. The TUI uses ratatui/crossterm.

## Build and run

```bash
cargo build --workspace           # build all crates
cargo build -p flow               # build TUI binary only
cargo build -p flow-cli           # build CLI binary only
cargo test --workspace            # run all tests
cargo test -p flow-core           # run tests in a specific crate
cargo install --path crates/flow      # install TUI binary
cargo install --path crates/flow-cli  # install CLI binary
```

## Environment variables

- `FLOW_PROVIDER=local|jira` — selects backend (default: local/demo)
- `FLOW_BOARD_PATH=/path` — overrides board directory for local mode
- Jira mode requires: `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN`, `JIRA_BOARD_ID`

## Architecture

Cargo workspace (edition 2024) with 4 crates under `crates/`:

- **`flow-core`** (library) — Core logic shared by TUI and CLI:
  - `model.rs` — Data types: `Board`, `Column`, `Card`, `Priority`.
  - `provider.rs` — `Provider` trait and `from_env()` factory.
  - `provider_local.rs` — Local filesystem provider, delegates to `store_fs`.
  - `provider_jira.rs` — Jira REST API provider.
  - `store_fs.rs` — Filesystem I/O for boards and cards.
  - `format.rs` — Output formatters (plain, json, xml, csv, table, markdown).

- **`flow-tui`** (library) — TUI-specific components:
  - `app.rs` — App state machine, `Action` enum, `EditState`.
  - `ui.rs` — Ratatui rendering and key handling.

- **`flow`** (binary) — TUI launcher. Depends on `flow-core` + `flow-tui`.

- **`flow-cli`** (binary) — CLI commands. Depends on `flow-core` with `cli` feature.

### Version

Defined once in root `Cargo.toml` under `[workspace.package]`. All crates inherit via `version.workspace = true`.

## Key patterns

- **Optimistic moves**: Card moves update the UI immediately. A background thread performs the actual provider operation. On failure, the board reloads to correct state.
- **Move queue**: Rapid moves queue up (max 64). The queue drains sequentially, one background move at a time.
- **Provider is created fresh per background move** (`provider::from_env()` in `spawn_move`) since providers aren't `Send`.

## Task management

This project uses `flow` itself as its Kanban task board. The project board lives in `./.board` (gitignored).

```bash
FLOW_BOARD_PATH=./.board flow-cli list             # list all cards
FLOW_BOARD_PATH=./.board flow-cli columns          # list columns
FLOW_BOARD_PATH=./.board flow-cli show <card_id>   # show card details
FLOW_BOARD_PATH=./.board flow-cli create <col> "title" --priority high
FLOW_BOARD_PATH=./.board flow-cli move <card_id> <col>
FLOW_BOARD_PATH=./.board flow-cli edit <card_id> --title "new title"
FLOW_BOARD_PATH=./.board flow-cli delete <card_id>
```

Use `flow-cli` for board management and `flow` to launch the TUI. Use this board to track tasks, bugs, and features for the project.

## Commit style

Conventional commits with optional scope: `feat(jira):`, `fix:`, `refactor(local):`, `chore:`.

## Releasing a version

When asked to release version X.Y.Z, follow these steps in order:

1. Update `version` in root `Cargo.toml` under `[workspace.package]`
2. Commit: `chore: bump version to X.Y.Z`
3. Tag: `git tag X.Y.Z`
4. Push everything: `git push && git push origin X.Y.Z`

GitHub Actions will build binaries and create the release automatically.
