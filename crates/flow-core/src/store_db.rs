//! SQLite storage backend for the local provider.
//!
//! One database file holds every tenant. Rows are partitioned by project
//! code: each tenant sees only its own board (columns + cards). Tenants are
//! created on first use with the standard columns; the `demo` tenant is
//! additionally seeded with demo cards.
//!
//! Each operation opens its own short-lived connection (the app also opens
//! providers per background move), so nothing `!Send` crosses threads.

use std::{
    io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::model::{Board, Card, Column, Priority};

/// Standard columns seeded for every new tenant: (id, title).
const DEFAULT_COLUMNS: [(&str, &str); 4] = [
    ("todo", "TO DO"),
    ("in_progress", "IN PROGRESS"),
    ("in_review", "IN REVIEW"),
    ("done", "DONE"),
];

/// Demo cards seeded for a fresh `demo` tenant:
/// (id, column, title, description).
const DEMO_CARDS: [(&str, &str, &str, &str); 6] = [
    ("FLOW-1", "todo", "Add counts to column headers", "Show the number of issues in each column."),
    ("FLOW-2", "todo", "Keyboard-first transitions", "Move selected issue left/right with one keystroke."),
    ("FLOW-3", "in_progress", "Detail pane / modal", "Enter toggles detail; it follows selection."),
    ("FLOW-4", "in_progress", "Polish focused column styling", "Subtle focus color; readable defaults."),
    ("FLOW-5", "in_review", "Demo data realism pass", "Keep demo neutral and screenshot-friendly."),
    ("FLOW-6", "done", "Initial ratatui scaffold", "Basic columns + selection + transitions."),
];

#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    Db(rusqlite::Error),
    Io(io::Error),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e)
    }
}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

fn open_db(db_path: &Path) -> Result<Connection, StoreError> {
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    // Short ops from two threads at most (UI + one background move);
    // wait on lock contention instead of failing immediately.
    // No WAL on purpose: the database must stay a single file.
    conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON;")?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS projects (
            code       TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS columns (
            project_code TEXT NOT NULL REFERENCES projects(code) ON DELETE CASCADE,
            id           TEXT NOT NULL,
            title        TEXT NOT NULL,
            position     INTEGER NOT NULL,
            PRIMARY KEY (project_code, id)
        );
        CREATE TABLE IF NOT EXISTS cards (
            project_code TEXT NOT NULL,
            id           TEXT NOT NULL,
            column_id    TEXT NOT NULL,
            title        TEXT NOT NULL DEFAULT '',
            description  TEXT NOT NULL DEFAULT '',
            priority     TEXT NOT NULL DEFAULT 'MEDIUM',
            assignee     TEXT NOT NULL DEFAULT '',
            position     INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (project_code, id),
            FOREIGN KEY (project_code, column_id) REFERENCES columns(project_code, id)
        );
        CREATE TABLE IF NOT EXISTS card_deps (
            project_code TEXT NOT NULL,
            card_id      TEXT NOT NULL,
            depends_on   TEXT NOT NULL,
            PRIMARY KEY (project_code, card_id, depends_on),
            FOREIGN KEY (project_code, card_id) REFERENCES cards(project_code, id) ON DELETE CASCADE
        );",
    )?;
    Ok(())
}

/// Creates the tenant row (plus standard columns, plus demo cards for the
/// `demo` tenant) when it does not exist yet. Idempotent.
fn ensure_tenant(conn: &Connection, project: &str) -> Result<(), StoreError> {
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO projects (code, created_at) VALUES (?1, ?2)",
        params![project, now_millis() as i64],
    )?;
    if inserted == 0 {
        return Ok(());
    }
    for (pos, (id, title)) in DEFAULT_COLUMNS.iter().enumerate() {
        conn.execute(
            "INSERT INTO columns (project_code, id, title, position) VALUES (?1, ?2, ?3, ?4)",
            params![project, id, title, pos as i64],
        )?;
    }
    if project == "demo" {
        for (pos, (id, col, title, desc)) in DEMO_CARDS.iter().enumerate() {
            conn.execute(
                "INSERT INTO cards (project_code, id, column_id, title, description, priority, assignee, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'MEDIUM', '', ?6)",
                params![project, id, col, title, desc, pos as i64],
            )?;
        }
    }
    Ok(())
}

/// All tenant codes in the database, sorted. Creates the schema on a fresh
/// database but no tenants.
pub fn list_projects(db_path: &Path) -> Result<Vec<String>, StoreError> {
    let conn = open_db(db_path)?;
    let mut stmt = conn.prepare("SELECT code FROM projects ORDER BY code")?;
    let codes: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(codes)
}

pub fn load_board(db_path: &Path, project: &str) -> Result<Board, StoreError> {
    let conn = open_db(db_path)?;
    ensure_tenant(&conn, project)?;

    let mut cols = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, title FROM columns WHERE project_code = ?1 ORDER BY position",
    )?;
    let col_rows: Vec<(String, String)> = stmt
        .query_map(params![project], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    for (id, title) in col_rows {
        let cards = load_cards(&conn, project, &id)?;
        cols.push(Column { id, title, cards });
    }

    let mut board = Board { columns: cols };
    board.sort_cards();
    Ok(board)
}

fn load_cards(conn: &Connection, project: &str, col_id: &str) -> Result<Vec<Card>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, description, priority, assignee
         FROM cards WHERE project_code = ?1 AND column_id = ?2 ORDER BY position",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map(params![project, col_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    let mut cards = Vec::with_capacity(rows.len());
    for (id, title, description, priority, assignee) in rows {
        let mut dep_stmt = conn.prepare(
            "SELECT depends_on FROM card_deps WHERE project_code = ?1 AND card_id = ?2 ORDER BY depends_on",
        )?;
        let depends_on: Vec<String> = dep_stmt
            .query_map(params![project, id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        cards.push(Card {
            id,
            title,
            description,
            priority: Priority::from_str(&priority),
            assignee,
            project: project.to_string(),
            depends_on,
        });
    }
    Ok(cards)
}

pub fn move_card(db_path: &Path, project: &str, card_id: &str, to_col_id: &str) -> Result<(), StoreError> {
    let conn = open_db(db_path)?;
    ensure_tenant(&conn, project)?;

    let src: Option<String> = conn
        .query_row(
            "SELECT column_id FROM cards WHERE project_code = ?1 AND id = ?2",
            params![project, card_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(src) = src else {
        return Err(StoreError::NotFound(card_id.to_string()));
    };
    if src == to_col_id {
        return Ok(());
    }
    let col_exists: Option<String> = conn
        .query_row(
            "SELECT id FROM columns WHERE project_code = ?1 AND id = ?2",
            params![project, to_col_id],
            |row| row.get(0),
        )
        .optional()?;
    if col_exists.is_none() {
        return Err(StoreError::NotFound(to_col_id.to_string()));
    }

    conn.execute(
        "UPDATE cards SET column_id = ?1,
            position = (SELECT COALESCE(MAX(position), -1) + 1 FROM cards
                        WHERE project_code = ?2 AND column_id = ?1)
         WHERE project_code = ?2 AND id = ?3",
        params![to_col_id, project, card_id],
    )?;
    Ok(())
}

pub fn create_card(db_path: &Path, project: &str, to_col_id: &str) -> Result<String, StoreError> {
    let conn = open_db(db_path)?;
    ensure_tenant(&conn, project)?;

    let col_exists: Option<String> = conn
        .query_row(
            "SELECT id FROM columns WHERE project_code = ?1 AND id = ?2",
            params![project, to_col_id],
            |row| row.get(0),
        )
        .optional()?;
    if col_exists.is_none() {
        return Err(StoreError::NotFound(to_col_id.to_string()));
    }

    let prefix = project.to_uppercase();
    // Millis-based ids can collide under rapid creates; retry with a fresh
    // timestamp instead of surfacing a primary-key violation.
    for _ in 0..100 {
        let id = format!("{}-{}", prefix, now_millis());
        let res = conn.execute(
            "INSERT INTO cards (project_code, id, column_id, title, description, priority, assignee, position)
             VALUES (?1, ?2, ?3, 'New card', '', 'MEDIUM', '',
                (SELECT COALESCE(MAX(position), -1) + 1 FROM cards
                 WHERE project_code = ?1 AND column_id = ?3))",
            params![project, id, to_col_id],
        );
        match res {
            Ok(_) => return Ok(id),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            Err(e) => return Err(StoreError::Db(e)),
        }
    }
    Err(StoreError::Db(rusqlite::Error::ToSqlConversionFailure(
        "could not mint a unique card id".into(),
    )))
}

pub fn delete_card(db_path: &Path, project: &str, card_id: &str) -> Result<(), StoreError> {
    let conn = open_db(db_path)?;
    ensure_tenant(&conn, project)?;

    // The provider vetoes deletion while dependents exist, but clear every
    // reference anyway so no dangling edge survives.
    conn.execute(
        "DELETE FROM card_deps WHERE project_code = ?1 AND (card_id = ?2 OR depends_on = ?2)",
        params![project, card_id],
    )?;
    let removed = conn.execute(
        "DELETE FROM cards WHERE project_code = ?1 AND id = ?2",
        params![project, card_id],
    )?;
    if removed == 0 {
        return Err(StoreError::NotFound(card_id.to_string()));
    }
    Ok(())
}

pub fn update_card(
    db_path: &Path,
    project: &str,
    card_id: &str,
    title: &str,
    description: &str,
    priority: Priority,
    assignee: &str,
    depends_on: &[String],
) -> Result<(), StoreError> {
    let conn = open_db(db_path)?;
    ensure_tenant(&conn, project)?;

    let updated = conn.execute(
        "UPDATE cards SET title = ?1, description = ?2, priority = ?3, assignee = ?4
         WHERE project_code = ?5 AND id = ?6",
        params![title, description, priority.label(), assignee, project, card_id],
    )?;
    if updated == 0 {
        return Err(StoreError::NotFound(card_id.to_string()));
    }
    conn.execute(
        "DELETE FROM card_deps WHERE project_code = ?1 AND card_id = ?2",
        params![project, card_id],
    )?;
    for dep in depends_on {
        conn.execute(
            "INSERT OR IGNORE INTO card_deps (project_code, card_id, depends_on) VALUES (?1, ?2, ?3)",
            params![project, card_id, dep],
        )?;
    }
    Ok(())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn tmp_db() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-store-test-{n}.db"))
    }

    fn cleanup(db: &Path) {
        let _ = std::fs::remove_file(db);
    }

    #[test]
    fn fresh_db_seeds_default_columns() -> TestResult {
        let db = tmp_db();
        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;

        let ids: Vec<&str> = board.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["todo", "in_progress", "in_review", "done"]);
        assert!(board.columns.iter().all(|c| c.cards.is_empty()));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn schema_init_is_idempotent() -> TestResult {
        let db = tmp_db();
        load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        // Second open must not duplicate columns or fail.
        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert_eq!(board.columns.len(), 4);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn create_and_load_roundtrip() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        assert!(id.starts_with("ACME-"), "unexpected id: {id}");

        update_card(
            &db,
            "acme",
            &id,
            "My Title",
            "Body text",
            Priority::High,
            "user@test.com",
            &["DEP-1".to_string(), "DEP-2".to_string()],
        )
        .map_err(|e| format!("{e:?}"))?;

        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        let card = board.find_card(&id).expect("card must load");
        assert_eq!(card.title, "My Title");
        assert_eq!(card.description, "Body text");
        assert_eq!(card.priority, Priority::High);
        assert_eq!(card.assignee, "user@test.com");
        assert_eq!(card.project, "acme");
        assert_eq!(card.depends_on, vec!["DEP-1".to_string(), "DEP-2".to_string()]);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn load_and_move_persists() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;

        move_card(&db, "acme", &id, "done").map_err(|e| format!("{e:?}"))?;

        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert!(board.columns[0].cards.is_empty());
        assert_eq!(board.columns[3].cards.len(), 1);
        assert_eq!(board.columns[3].cards[0].id, id);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_to_same_column_is_noop() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        move_card(&db, "acme", &id, "todo").map_err(|e| format!("{e:?}"))?;

        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert_eq!(board.columns[0].cards.len(), 1);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_missing_card_or_column_is_not_found() {
        let db = tmp_db();
        assert!(matches!(
            move_card(&db, "acme", "X-1", "todo"),
            Err(StoreError::NotFound(_))
        ));
        let id = create_card(&db, "acme", "todo").expect("create");
        assert!(matches!(
            move_card(&db, "acme", &id, "nope"),
            Err(StoreError::NotFound(_))
        ));
        cleanup(&db);
    }

    #[test]
    fn update_missing_card_is_not_found() {
        let db = tmp_db();
        assert!(matches!(
            update_card(&db, "acme", "X-1", "t", "", Priority::Low, "", &[]),
            Err(StoreError::NotFound(_))
        ));
        cleanup(&db);
    }

    #[test]
    fn update_replaces_dependencies() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        update_card(&db, "acme", &id, "t", "", Priority::Low, "", &["A".to_string()])
            .map_err(|e| format!("{e:?}"))?;
        update_card(&db, "acme", &id, "t", "", Priority::Low, "", &["B".to_string()])
            .map_err(|e| format!("{e:?}"))?;

        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert_eq!(board.find_card(&id).unwrap().depends_on, vec!["B".to_string()]);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn delete_removes_card_and_dep_edges() -> TestResult {
        let db = tmp_db();
        let a = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        let b = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        update_card(&db, "acme", &b, "t", "", Priority::Low, "", &[a.clone()])
            .map_err(|e| format!("{e:?}"))?;

        delete_card(&db, "acme", &b).map_err(|e| format!("{e:?}"))?;
        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert!(board.find_card(&b).is_none());

        // Deleting the dependency target clears the edge on the other side too.
        let c = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        update_card(&db, "acme", &c, "t", "", Priority::Low, "", &[a.clone()])
            .map_err(|e| format!("{e:?}"))?;
        delete_card(&db, "acme", &a).map_err(|e| format!("{e:?}"))?;
        let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert!(board.find_card(&c).unwrap().depends_on.is_empty());

        assert!(matches!(
            delete_card(&db, "acme", "X-1"),
            Err(StoreError::NotFound(_))
        ));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn priority_roundtrips() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
        for p in [Priority::Low, Priority::Medium, Priority::High, Priority::Bug, Priority::Wishlist] {
            update_card(&db, "acme", &id, "t", "", p, "", &[]).map_err(|e| format!("{e:?}"))?;
            let board = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
            assert_eq!(board.find_card(&id).unwrap().priority, p, "roundtrip failed for {p:?}");
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn tenants_are_isolated() -> TestResult {
        let db = tmp_db();
        let id = create_card(&db, "aaa", "todo").map_err(|e| format!("{e:?}"))?;

        let board = load_board(&db, "bbb").map_err(|e| format!("{e:?}"))?;
        assert!(board.find_card(&id).is_none());
        assert_eq!(board.columns.len(), 4);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn list_projects_reports_tenants_sorted() -> TestResult {
        let db = tmp_db();
        assert!(list_projects(&db).map_err(|e| format!("{e:?}"))?.is_empty());

        load_board(&db, "zeta").map_err(|e| format!("{e:?}"))?;
        load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;

        assert_eq!(
            list_projects(&db).map_err(|e| format!("{e:?}"))?,
            vec!["acme".to_string(), "zeta".to_string()]
        );

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn demo_tenant_seeds_demo_cards() -> TestResult {
        let db = tmp_db();
        let board = load_board(&db, "demo").map_err(|e| format!("{e:?}"))?;

        let total: usize = board.columns.iter().map(|c| c.cards.len()).sum();
        assert_eq!(total, 6);
        let card = board.find_card("FLOW-1").expect("demo card FLOW-1");
        assert_eq!(card.title, "Add counts to column headers");
        assert_eq!(card.project, "demo");

        // Other tenants stay empty.
        let other = load_board(&db, "acme").map_err(|e| format!("{e:?}"))?;
        assert_eq!(other.columns.iter().map(|c| c.cards.len()).sum::<usize>(), 0);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn rapid_creates_get_unique_ids() -> TestResult {
        let db = tmp_db();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..20 {
            let id = create_card(&db, "acme", "todo").map_err(|e| format!("{e:?}"))?;
            assert!(ids.insert(id.clone()), "duplicate id: {id}");
        }

        cleanup(&db);
        Ok(())
    }
}
