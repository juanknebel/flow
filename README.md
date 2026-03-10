# flow

A keyboard-first Kanban board in your terminal.

`flow` is both a standalone CLI/TUI tool and a reusable Rust library for building Kanban-style interfaces in your own terminal applications.

Move work between states with a single keystroke and peek at issue descriptions without opening a browser.

![Demo](./demo.gif)

## Why
Opening a browser just to move an issue is slow and breaks focus.  
`flow` keeps the common actions fast, local, and keyboard-driven.

## Features
- **Standalone CLI & TUI**: Powerful interactive board and scriptable CLI.
- **Reusable Library**: Exported components, models, and UI logic for integration into other Rust TUI apps.
- **Multiple Providers**: Local filesystem (Markdown-based) and Jira Cloud support.
- **Keyboard-first**: One-keystroke transitions (`H` / `L`), `hjkl` and arrow-key navigation.
- **Integrated Editing**: Create/edit cards directly in an integrated popup with full cursor support, word wrapping, and familiar shortcuts.
- **Clean Visuals**: Terminal-native design powered by `ratatui` with responsive layouts.

## As a Library
You can use `flow` as a crate in your own Rust projects to add Kanban boards to your terminal UIs.

Add to your `Cargo.toml`:
```toml
[dependencies]
flow = { git = "https://github.com/example/flow" }
```

Basic usage with `ratatui`:
```rust
use flow::{App, Board, ui, provider_local::LocalProvider, Provider};

// 1. Initialize a provider (Local or Jira)
let mut provider = LocalProvider::new(std::path::PathBuf::from("./my-board"));

// 2. Load the board and create the App state
let board = provider.load_board().expect("failed to load board");
let mut app = App::new(board);

// 3. Render in your terminal loop
// (app handles focus, selection, and optimistic moves)
terminal.draw(|f| {
    ui::render(f, &app);
})?;
```

## Demo / Local mode
`flow` runs in **demo mode by default**.

Demo data is loaded from files on disk and can be edited directly.
This makes the demo representative of real usage, not a hardcoded example.

Default demo board location:
```
boards/demo/
```

To use a persistent local board, point `FLOW_BOARD_PATH` at the board directory:
```bash
FLOW_BOARD_PATH=/path/to/board cargo run
```

Local boards default to:
```
~/.config/flow/boards/default
```

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
- `H` / `L` — move card left / right
- `Enter` — toggle detail view
- `r` — reload board from disk
- `Esc` — close popup / cancel / quit
- `q` — quit

### Card Actions
- `a` / `n` — create a new card
- `e` — edit selected card
- `d` — delete selected card with confirmation

### Integrated Editor
- `←` / `→` — move cursor
- `Home` / `End` — move to start/end of line
- `Backspace` / `Delete` — delete character
- `Tab` — switch between Title and Description
- `Enter` — save and close
- `Esc` — cancel changes

## CLI Usage
`flow` can also be used as a CLI tool for scripts or quick actions:

```bash
# List all columns and cards
flow list

# Show a specific card
flow show FLOW-1

# Create a new card
flow create todo "My new task" --body "Some details"

# Move a card to another column
flow move FLOW-1 in_progress

# Delete a card permanently
flow delete FLOW-1

# List column IDs
flow columns
```

Output format can be changed with `-f`: `plain`, `json`, `xml`, `csv`, `table`, `markdown`.

## Installation

### From source
```bash
git clone https://github.com/example/flow
cd flow
cargo install --path .
```

## License
MIT
