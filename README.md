# flow

A keyboard-first Kanban board in your terminal.

`flow` is both a standalone CLI/TUI tool and a reusable Rust library for building Kanban-style interfaces in your own terminal applications.

Move work between states with a single keystroke and peek at issue descriptions without opening a browser.

![Demo](./demo.gif)

## Why
Opening a browser just to move an issue is slow and breaks focus.
`flow` keeps the common actions fast, local, and keyboard-driven.

## Features
- **Standalone CLI & TUI**: Powerful interactive board (`flow`) and scriptable CLI (`flow-cli`).
- **Reusable Library**: Exported components, models, and UI logic for integration into other Rust TUI apps.
- **Multiple Providers**: Local SQLite (single `.db` file, multitenant by project code) and Jira Cloud support.
- **Keyboard-first**: One-keystroke transitions (`H` / `L`), `hjkl` and arrow-key navigation.
- **Card Priority**: Color-coded priority levels (Low, Medium, High, Bug, Wishlist) visible on the board and editable in the popup. Cards are automatically sorted by priority (Bug → High → Medium → Low → Wishlist), then alphabetically by title.
- **Card Assignee**: Assign cards to team members (email or user id) via CLI (`--assignee`) or the TUI edit popup.
- **Card Dependencies**: Cards can depend on other cards (`--depends-on`, or the Depends On field in the editor). Blocked cards show an indicator on the board; a card can't be deleted while others still depend on it, and cycles/self-dependencies/missing ids are rejected.
- **Search**: Fuzzy-free substring search (`/`) over title and description, with next/previous match navigation.
- **Project filter & all-projects view**: `FLOW_PROJECT=all` opens a read-only union of every tenant, filterable to a subset of projects (`p`) in the TUI.
- **Undo delete**: Recover the last deleted card (`u`) — recreated with the same content in the same column.
- **Integrated Editing**: Create/edit cards directly in an integrated popup with full cursor support, word wrapping, and familiar shortcuts.
- **Export / Import**: Back up or copy a project's cards as JSON via `flow-cli` (`export` / `import`), with dependencies remapped automatically.
- **Local daemon & MCP**: `flow daemon` coordinates concurrent agents over a Unix socket; `flow mcp` exposes tools to LLM agents. Auto-starts when needed.
- **Agent leases**: Cards can be claimed/released by agents (`claim` / `release`) with time-boxed leases; expired leases are reaped automatically.
- **Swarm dashboard**: `flow-cli dashboard` and `D` in the TUI show claimed/blocked/free cards and counts by state/column.
- **Clean Visuals**: Terminal-native design powered by `ratatui` with responsive layouts.

## Project Structure

Cargo workspace with 4 crates:

```
crates/
  flow-core/    # Core library: models, providers, formatters, SQLite storage
  flow-tui/     # TUI library: app state, rendering, key handling
  flow/         # Binary: TUI launcher
  flow-cli/     # Binary: CLI commands
```

## As a Library
You can use `flow-core` and `flow-tui` as crates in your own Rust projects to add Kanban boards to your terminal UIs.

Add to your `Cargo.toml`:
```toml
[dependencies]
flow-core = { git = "https://github.com/juanknebel/flow" }
flow-tui = { git = "https://github.com/juanknebel/flow" }
```

Basic usage with `ratatui`:
```rust
use flow_core::{Board, provider_local::LocalProvider, Provider};
use flow_tui::{App, ui};

// 1. Initialize a provider (Local or Jira)
let mut provider = LocalProvider::new(std::path::PathBuf::from("./my-board"));

// 2. Load the board and create the App state
let board = provider.load_board().expect("failed to load board");
let mut app = App::new(board);

// 3. Render in your terminal loop
terminal.draw(|f| {
    ui::render(f, &app, None);
})?;
```

## Demo / Local mode
`flow` runs in **local mode by default**, backed by SQLite.

One database file holds every tenant. Rows are partitioned by project
code: each tenant sees only its own board (columns + cards).

Default database location:
```
~/.config/flow/flow.db
```

To use another database file, point `FLOW_BOARD_PATH` at it (a directory
keeps working too: `flow.db` is created inside it):
```bash
FLOW_BOARD_PATH=/path/to/board.db flow
```

To select the tenant, set `FLOW_PROJECT`:
```bash
FLOW_PROJECT=acme flow
```

When `FLOW_PROJECT` is unset, the TUI shows a project picker listing
every tenant in the database (or accepts a new code). The CLI ignores
`FLOW_PROJECT` and requires `--project` on every invocation instead.

The code `all` (any case) is reserved: it opens a read-only union of
every tenant, filterable by project with `p` in the TUI. On a database with
no tenants yet this view has no columns (standard columns are only seeded
when a real tenant is first opened).

`FLOW_PROJECT=demo` seeds a demo board with example cards on first use.

Valid priorities: `LOW`, `MEDIUM`, `HIGH`, `BUG`, `WISHLIST`.

## Jira mode
The Jira backend is **read-only** in the TUI/CLI: it supports loading the
board and moving issues through transitions, but not creating, editing, or
deleting cards, and it does not expose card dependencies (Jira has its own
issue-link model). Card priorities are reported as `MEDIUM`.

To load issues from Jira, set:
```bash
FLOW_PROVIDER=jira
JIRA_BASE_URL=https://your-site.atlassian.net
JIRA_EMAIL=you@example.com
JIRA_API_TOKEN=your_token
JIRA_BOARD_ID=123
```

## Local daemon & agent integration
- `FLOW_ACTOR=name` — audit label written to cards on create/update.
- `FLOW_DAEMON_SOCKET=/path` — Unix socket for the daemon (default `~/.config/flow/flow.sock`).
- The daemon stores its auth token in `~/.config/flow/daemon.token` (mode 600) and is auto-started by clients.
- `flow daemon` — run the coordinator manually.
- `flow mcp` — run an MCP server over stdio for LLM agents.

## Keybindings

### Navigation
- `h` / `l` or `←` / `→` — focus column
- `j` / `k` or `↑` / `↓` — select card
- `s` — toggle sort order (ascending/descending priority)
- `/` — search by title/description; `↓`/`↑` jump to next/previous match while typing, `Enter`/`Esc` to close
- `p` — filter by project (all-projects view only)
- `P` — switch project (local)
- `D` — swarm dashboard
- `d` (in project picker) — delete project (type the code to confirm when it has cards)
- `H` / `L` — move card left / right
- `Enter` — toggle detail view
- `r` — reload board
- `Esc` — close popup / cancel / quit
- `q` — quit

### Card Actions
- `a` / `n` — create a new card
- `e` — edit selected card
- `d` — delete selected card with confirmation
- `u` — undo the last delete (recreates the card with the same content)

### Integrated Editor
- `←` / `→` — move cursor (cycle priority when Priority field is focused)
- `Home` / `End` — move to start/end of line
- `Backspace` / `Delete` — delete character
- `Tab` — cycle focus: Title → Depends On → Priority → Assignee → Description
- `Enter` — save and close
- `Esc` — cancel changes

## CLI Usage
`flow-cli` is the standalone CLI tool for scripts or quick actions:

```bash
# List all columns and cards
flow-cli --project acme list

# Show a specific card
flow-cli --project acme show FLOW-1

# Create a new card
flow-cli --project acme create todo "My new task" --body "Some details" --priority high

# Edit a card's priority
flow-cli --project acme edit FLOW-1 --priority bug

# Move a card to another column
flow-cli --project acme move FLOW-1 in_progress

# Delete a card permanently
flow-cli --project acme delete FLOW-1

# Make a card depend on another (comma-separated or repeated flag)
flow-cli --project acme edit FLOW-5 --depends-on FLOW-1,FLOW-2

# Delete a whole project (requires --confirm when it has cards)
flow-cli --project oldproj delete-project --confirm oldproj
flow-cli --project acme claim FLOW-1 --agent bot --lease-ms 300000
flow-cli --project acme release FLOW-1 --agent bot
flow-cli --project acme dashboard -f json
flow daemon
flow mcp

# List column IDs
flow-cli --project acme columns

# Back up a project's cards as JSON
flow-cli --project acme export --out acme-backup.json

# Restore/copy cards from a JSON export (dependencies are remapped automatically)
flow-cli --project acme-copy import acme-backup.json
```

Output format can be changed with `-f`: `plain`, `json`, `xml`, `csv`, `table`, `markdown`.

## Installation

### From source
```bash
git clone https://github.com/juanknebel/flow
cd flow
cargo install --path crates/flow       # TUI
cargo install --path crates/flow-cli   # CLI
```

## Local daemon (optional, for agent swarms)

`flow` can run a local coordination daemon over a Unix socket:

```bash
flow daemon
```

The daemon is the single writer of the SQLite database and exposes a
JSON-RPC interface (newline-delimited JSON) on
`$FLOW_DAEMON_SOCKET` (default `~/.config/flow/flow.sock`). Methods:
`ping`, `list_projects`, `load_board`, `create_card`, `update_card`,
`move_card`, `delete_card`, `project_card_count`, `delete_project`.

This is intended for multi-agent swarms running on one machine. When the
daemon is not running, the CLI and TUI access SQLite directly.

## License
MIT
