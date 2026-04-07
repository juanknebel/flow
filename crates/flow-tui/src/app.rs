use flow_core::model::{Board, Card, Priority, SortOrder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    CloseOrQuit,
    FocusLeft,
    FocusRight,
    SelectUp,
    SelectDown,
    MoveLeft,
    MoveRight,
    ToggleDetail,
    Refresh,
    Delete,
    Add,
    Edit,
    ToggleSort,
    Search,
    ProjectFilter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditFocus {
    Title,
    Description,
    Priority,
    Assignee,
    Project,
}

impl EditFocus {
    pub fn next(self) -> Self {
        match self {
            EditFocus::Title => EditFocus::Project,
            EditFocus::Project => EditFocus::Priority,
            EditFocus::Priority => EditFocus::Assignee,
            EditFocus::Assignee => EditFocus::Description,
            EditFocus::Description => EditFocus::Title,
        }
    }
}

pub struct EditState {
    pub card_id: String,
    pub col_id: String,
    pub is_new: bool,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub assignee: String,
    pub project: String,
    pub cursor_pos: usize,
    pub focus: EditFocus,
}

impl EditState {
    pub fn current_text(&self) -> &str {
        match self.focus {
            EditFocus::Title => &self.title,
            EditFocus::Description => &self.description,
            EditFocus::Assignee => &self.assignee,
            EditFocus::Project => &self.project,
            EditFocus::Priority => "",
        }
    }

    pub fn current_text_mut(&mut self) -> &mut String {
        match self.focus {
            EditFocus::Title => &mut self.title,
            EditFocus::Description => &mut self.description,
            EditFocus::Assignee => &mut self.assignee,
            EditFocus::Project => &mut self.project,
            EditFocus::Priority => &mut self.title, // unused for priority, but must return something
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if matches!(self.focus, EditFocus::Priority) {
            return;
        }
        let pos = self.cursor_pos;
        let text = self.current_text_mut();
        if pos >= text.len() {
            text.push(c);
        } else {
            text.insert(pos, c);
        }
        self.cursor_pos += c.len_utf8();
    }

    pub fn delete_prev(&mut self) {
        if matches!(self.focus, EditFocus::Priority) {
            return;
        }
        if self.cursor_pos > 0 {
            let pos = self.cursor_pos;
            let text = self.current_text_mut();
            if let Some((idx, _)) = text.char_indices().filter(|(i, _)| *i < pos).last() {
                text.remove(idx);
                self.cursor_pos = idx;
            }
        }
    }

    pub fn delete_curr(&mut self) {
        if matches!(self.focus, EditFocus::Priority) {
            return;
        }
        let pos = self.cursor_pos;
        let text = self.current_text_mut();
        if pos < text.len() {
            text.remove(pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if matches!(self.focus, EditFocus::Priority) {
            self.priority = self.priority.prev();
            return;
        }
        if self.cursor_pos > 0 {
            let text = self.current_text();
            let pos = self.cursor_pos;
            if let Some((idx, _)) = text.char_indices().filter(|(i, _)| *i < pos).last() {
                self.cursor_pos = idx;
            }
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.focus == EditFocus::Priority {
            self.priority = self.priority.next();
            return;
        }
        let text = self.current_text();
        let pos = self.cursor_pos;
        if let Some((idx, _)) = text.char_indices().filter(|(i, _)| *i > pos).next() {
            self.cursor_pos = idx;
        } else if pos < text.len() {
            self.cursor_pos = text.len();
        }
    }
}

pub struct SearchState {
    pub query: String,
    pub cursor_pos: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let pos = self.cursor_pos;
        if pos >= self.query.len() {
            self.query.push(c);
        } else {
            self.query.insert(pos, c);
        }
        self.cursor_pos += c.len_utf8();
    }

    pub fn delete_prev(&mut self) {
        if self.cursor_pos > 0 {
            let pos = self.cursor_pos;
            if let Some((idx, _)) = self.query.char_indices().filter(|(i, _)| *i < pos).last() {
                self.query.remove(idx);
                self.cursor_pos = idx;
            }
        }
    }

    pub fn matches_card(card: &Card, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        let q = query.to_lowercase();
        card.title.to_lowercase().contains(&q) || card.description.to_lowercase().contains(&q)
    }
}

pub struct ProjectFilterState {
    /// All available project names (sorted). Empty string means "unassigned".
    pub projects: Vec<String>,
    /// Selection state per project (parallel to `projects`).
    pub selected: Vec<bool>,
    /// Cursor position in the filter modal.
    pub cursor: usize,
}

pub struct App {
    pub board: Board,
    pub col: usize,
    pub row: usize,
    pub detail_open: bool,
    pub confirm_delete: bool,
    pub edit_state: Option<EditState>,
    pub search_state: Option<SearchState>,
    pub project_filter_state: Option<ProjectFilterState>,
    /// Active project filter: empty = show all, otherwise only these projects.
    pub project_filter: Vec<String>,
    pub banner: Option<String>,
    pub sort_order: SortOrder,
}

impl App {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            col: 0,
            row: 0,
            detail_open: false,
            confirm_delete: false,
            edit_state: None,
            search_state: None,
            project_filter_state: None,
            project_filter: Vec::new(),
            banner: None,
            sort_order: SortOrder::default(),
        }
    }

    fn reset_cursor(&mut self) {
        (self.col, self.row) = (0, 0);
    }

    fn clamp_index(idx: usize, delta: isize, max: usize) -> usize {
        if delta < 0 {
            idx.saturating_sub((-delta) as usize)
        } else {
            (idx + delta as usize).min(max)
        }
    }

    fn col_len(&self) -> usize {
        self.board
            .columns
            .get(self.col)
            .map(|c| c.cards.len())
            .unwrap_or(0)
    }

    fn clamp_row(&mut self) {
        let len = self.col_len();
        self.row = if len == 0 { 0 } else { self.row.min(len - 1) };
    }

    fn next_non_empty_col(&self, step: isize) -> Option<usize> {
        if step == 0 {
            return None;
        }

        let mut idx = self.col as isize;
        let len = self.board.columns.len() as isize;

        loop {
            idx += step;
            if idx < 0 || idx >= len {
                return None;
            }
            let col = &self.board.columns[idx as usize];
            if !col.cards.is_empty() {
                return Some(idx as usize);
            }
        }
    }

    fn dst_col(&self, dir: isize) -> Option<usize> {
        let dst = self.col as isize + dir;
        if dst < 0 {
            return None;
        }
        let dst = dst as usize;
        (dst < self.board.columns.len()).then_some(dst)
    }

    pub fn clamp(&mut self) {
        if self.board.columns.is_empty() {
            self.reset_cursor();
            return;
        }

        self.col = self.col.min(self.board.columns.len() - 1);
        self.clamp_row();
    }

    pub fn focus(&mut self, dir: isize) {
        if self.board.columns.is_empty() {
            self.reset_cursor();
            return;
        }

        let dir = dir.signum();
        if let Some(next) = self.next_non_empty_col(dir) {
            self.col = next;
            self.clamp_row();
        }
    }

    pub fn select(&mut self, delta: isize) {
        let len = self.col_len();
        if len == 0 {
            self.row = 0;
            return;
        }

        self.row = Self::clamp_index(self.row, delta, len - 1);
    }

    pub fn apply(&mut self, a: Action) -> bool {
        match a {
            Action::Quit => return true,
            Action::CloseOrQuit => {
                if self.edit_state.is_some() {
                    self.edit_state = None;
                } else if self.project_filter_state.is_some() {
                    self.project_filter_state = None;
                } else if self.search_state.is_some() {
                    self.search_state = None;
                } else if self.confirm_delete {
                    self.confirm_delete = false;
                } else if self.detail_open {
                    self.detail_open = false;
                } else {
                    return true;
                }
            }
            Action::FocusLeft => self.focus(-1),
            Action::FocusRight => self.focus(1),
            Action::SelectUp => self.select(-1),
            Action::SelectDown => self.select(1),
            Action::ToggleDetail => self.detail_open = !self.detail_open,
            Action::Delete => {
                if !self.board.columns.is_empty() && !self.board.columns[self.col].cards.is_empty() {
                    self.confirm_delete = true;
                }
            }
            Action::ToggleSort => {
                self.sort_order = self.sort_order.toggle();
                self.board.sort_cards_with(self.sort_order);
            }
            Action::Refresh | Action::MoveLeft | Action::MoveRight | Action::Add | Action::Edit | Action::Search | Action::ProjectFilter => {}
        }
        false
    }

    pub fn focus_first_non_empty(&mut self) {
        (self.col, self.row) = (first_non_empty_column(&self.board).unwrap_or(0), 0);
    }

    pub fn search_matches(&self) -> Vec<(usize, usize)> {
        let Some(search) = &self.search_state else {
            return vec![];
        };
        if search.query.is_empty() {
            return vec![];
        }
        let mut matches = Vec::new();
        for (col_idx, col) in self.board.columns.iter().enumerate() {
            for (card_idx, card) in col.cards.iter().enumerate() {
                if SearchState::matches_card(card, &search.query) {
                    matches.push((col_idx, card_idx));
                }
            }
        }
        matches
    }

    pub fn select_next_match(&mut self) {
        let matches = self.search_matches();
        if matches.is_empty() {
            return;
        }
        let current = (self.col, self.row);
        if let Some(&(col, row)) = matches.iter().find(|&&pos| pos > current) {
            self.col = col;
            self.row = row;
        } else {
            self.col = matches[0].0;
            self.row = matches[0].1;
        }
    }

    pub fn select_prev_match(&mut self) {
        let matches = self.search_matches();
        if matches.is_empty() {
            return;
        }
        let current = (self.col, self.row);
        if let Some(&(col, row)) = matches.iter().rev().find(|&&pos| pos < current) {
            self.col = col;
            self.row = row;
        } else {
            let last = matches[matches.len() - 1];
            self.col = last.0;
            self.row = last.1;
        }
    }

    pub fn optimistic_move(&mut self, dir: isize) -> Option<(String, String)> {
        if self.board.columns.is_empty() {
            return None;
        }

        self.clamp();

        let dst = self.dst_col(dir)?;
        let src = self.col;
        if self.board.columns[src].cards.is_empty() {
            return None;
        }

        let card = self.board.columns[src].cards.remove(self.row);
        let card_id = card.id.clone();
        let to_col_id = self.board.columns[dst].id.clone();

        self.board.columns[dst].cards.push(card);

        self.col = dst;
        self.row = self.board.columns[dst].cards.len() - 1;

        Some((card_id, to_col_id))
    }
}

fn first_non_empty_column(board: &Board) -> Option<usize> {
    for (i, col) in board.columns.iter().enumerate() {
        if !col.cards.is_empty() {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_core::model::{Board, Card, Column, Priority};

    fn card(id: &str, title: &str) -> Card {
        Card {
            id: id.into(),
            title: title.into(),
            description: "d".into(),
            priority: Priority::Medium,
            assignee: String::new(),
            project: String::new(),
        }
    }

    fn board_two_cols() -> Board {
        Board {
            columns: vec![
                Column {
                    id: "a".into(),
                    title: "A".into(),
                    cards: vec![card("1", "t1"), card("2", "t2")],
                },
                Column {
                    id: "b".into(),
                    title: "B".into(),
                    cards: vec![],
                },
            ],
        }
    }

    #[test]
    fn clamp_bounds_indices() {
        let mut app = App::new(board_two_cols());
        (app.col, app.row) = (9, 9);
        app.clamp();

        assert_eq!((app.col, app.row), (1, 0));
    }

    #[test]
    fn focus_skips_empty_columns() {
        let mut app = App::new(board_two_cols());

        app.focus(-1);
        assert_eq!(app.col, 0);

        app.focus(10);
        assert_eq!(app.col, 0);

        app.board.columns[1].cards.push(card("3", "t3"));
        app.focus(1);
        assert_eq!(app.col, 1);
    }

    #[test]
    fn select_clamps_rows_and_handles_empty_column() {
        let mut app = App::new(board_two_cols());

        app.select(10);
        assert_eq!(app.row, 1);

        app.select(-10);
        assert_eq!(app.row, 0);

        (app.col, app.row) = (1, 9);
        app.select(1);
        assert_eq!(app.row, 0);
    }

    #[test]
    fn move_right_moves_card_and_updates_focus_to_new_card() {
        let mut app = App::new(board_two_cols());

        let (id, dst) = app.optimistic_move(1).unwrap();

        assert_eq!(id, "1");
        assert_eq!(dst, "b");
        assert_eq!((app.col, app.row), (1, 0));
        assert_eq!(app.board.columns[1].cards.len(), 1);
        assert_eq!(app.board.columns[1].cards[0].id, "1");
        assert_eq!(app.board.columns[0].cards.len(), 1);
    }

    #[test]
    fn move_out_of_bounds_is_none() {
        let mut app = App::new(board_two_cols());

        assert!(app.optimistic_move(-1).is_none());
        assert!(app.optimistic_move(10).is_none());
    }

    #[test]
    fn move_with_empty_board_is_none_and_does_not_panic() {
        let mut app = App::new(Board { columns: vec![] });

        assert!(app.optimistic_move(1).is_none());
        assert_eq!((app.col, app.row), (0, 0));
    }

    #[test]
    fn move_from_empty_column_is_none() {
        let mut app = App::new(board_two_cols());
        (app.col, app.row) = (1, 0);

        assert!(app.optimistic_move(-1).is_none());
    }

    #[test]
    fn focus_first_non_empty_picks_first_column_with_cards() {
        let mut app = App::new(board_two_cols());

        app.board.columns[0].cards.clear();
        app.board.columns[1].cards.push(card("2", "t2"));
        app.focus_first_non_empty();

        assert_eq!((app.col, app.row), (1, 0));
    }

    #[test]
    fn close_or_quit_closes_detail_first_then_quits() {
        let mut app = App::new(board_two_cols());

        app.detail_open = true;
        assert!(!app.apply(Action::CloseOrQuit));
        assert!(!app.detail_open);

        assert!(app.apply(Action::CloseOrQuit));
    }

    #[test]
    fn edit_focus_cycles() {
        assert_eq!(EditFocus::Title.next(), EditFocus::Project);
        assert_eq!(EditFocus::Project.next(), EditFocus::Priority);
        assert_eq!(EditFocus::Priority.next(), EditFocus::Assignee);
        assert_eq!(EditFocus::Assignee.next(), EditFocus::Description);
        assert_eq!(EditFocus::Description.next(), EditFocus::Title);
    }

    fn card_with_desc(id: &str, title: &str, desc: &str) -> Card {
        Card {
            id: id.into(),
            title: title.into(),
            description: desc.into(),
            priority: Priority::Medium,
            assignee: String::new(),
            project: String::new(),
        }
    }

    fn search_board() -> Board {
        Board {
            columns: vec![
                Column {
                    id: "todo".into(),
                    title: "Todo".into(),
                    cards: vec![
                        card_with_desc("1", "Fix login bug", "auth issue"),
                        card_with_desc("2", "Add search", "filter cards"),
                    ],
                },
                Column {
                    id: "done".into(),
                    title: "Done".into(),
                    cards: vec![
                        card_with_desc("3", "Setup CI", "pipeline bug fix"),
                    ],
                },
            ],
        }
    }

    #[test]
    fn search_matches_card_by_title() {
        let card = card_with_desc("1", "Fix login bug", "some desc");
        assert!(SearchState::matches_card(&card, "login"));
        assert!(SearchState::matches_card(&card, "LOGIN")); // case-insensitive
        assert!(!SearchState::matches_card(&card, "missing"));
    }

    #[test]
    fn search_matches_card_by_description() {
        let card = card_with_desc("1", "title", "auth issue");
        assert!(SearchState::matches_card(&card, "auth"));
        assert!(!SearchState::matches_card(&card, "missing"));
    }

    #[test]
    fn search_empty_query_matches_nothing() {
        let card = card_with_desc("1", "title", "desc");
        assert!(!SearchState::matches_card(&card, ""));
    }

    #[test]
    fn search_matches_returns_matching_positions() {
        let mut app = App::new(search_board());
        app.search_state = Some(SearchState { query: "bug".into(), cursor_pos: 3 });

        let matches = app.search_matches();
        assert_eq!(matches, vec![(0, 0), (1, 0)]); // "Fix login bug" and "pipeline bug fix"
    }

    #[test]
    fn select_next_match_wraps_around() {
        let mut app = App::new(search_board());
        app.search_state = Some(SearchState { query: "bug".into(), cursor_pos: 3 });
        // matches: (0,0) "Fix login bug", (1,0) "pipeline bug fix"

        app.select_next_match(); // from (0,0) → next match is (1,0)
        assert_eq!((app.col, app.row), (1, 0));

        app.select_next_match(); // wraps to (0,0)
        assert_eq!((app.col, app.row), (0, 0));
    }

    #[test]
    fn select_prev_match_wraps_around() {
        let mut app = App::new(search_board());
        app.search_state = Some(SearchState { query: "bug".into(), cursor_pos: 3 });

        app.select_prev_match(); // wraps to last
        assert_eq!((app.col, app.row), (1, 0));

        app.select_prev_match();
        assert_eq!((app.col, app.row), (0, 0));
    }

    #[test]
    fn close_or_quit_closes_search_before_detail() {
        let mut app = App::new(search_board());
        app.search_state = Some(SearchState::new());
        app.detail_open = true;

        assert!(!app.apply(Action::CloseOrQuit));
        assert!(app.search_state.is_none());
        assert!(app.detail_open); // detail still open

        assert!(!app.apply(Action::CloseOrQuit));
        assert!(!app.detail_open);
    }

    #[test]
    fn search_insert_and_delete() {
        let mut search = SearchState::new();
        search.insert_char('a');
        search.insert_char('b');
        assert_eq!(search.query, "ab");
        assert_eq!(search.cursor_pos, 2);

        search.delete_prev();
        assert_eq!(search.query, "a");
        assert_eq!(search.cursor_pos, 1);
    }
}
