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
- **Integrated Editing**: Create/edit cards directly in an integrated popup with full cursor support, word wrapping, and familiar shortcuts.
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

`FLOW_PROJECT=demo` seeds a demo board with example cards on first use.

Valid priorities: `LOW`, `MEDIUM`, `HIGH`, `BUG`, `WISHLIST`.

## Jira mode
To load issues from Jira, set:
```bash
FLOW_PROVIDER=jira
JIRA_BASE_URL=https://your-site.atlassian.net
JIRA_EMAIL=you@example.com
JIRA_API_TOKEN=your_token
JIRA_BOARD_ID=123
```

## Keybindings

### Navigation
- `h` / `l` or `←` / `→` — focus column
- `j` / `k` or `↑` / `↓` — select card
- `P` — switch project (local)
- `H` / `L` — move card left / right
- `Enter` — toggle detail view
- `r` — reload board
- `Esc` — close popup / cancel / quit
- `q` — quit

### Card Actions
- `a` / `n` — create a new card
- `e` — edit selected card
- `d` — delete selected card with confirmation

### Integrated Editor
- `←` / `→` — move cursor (cycle priority when Priority field is focused)
- `Home` / `End` — move to start/end of line
- `Backspace` / `Delete` — delete character
- `Tab` — cycle focus: Title → Priority → Assignee → Description
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

# List column IDs
flow-cli --project acme columns
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

## License
MIT
