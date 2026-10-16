use std::path::{Path, PathBuf};

use crate::{
    model::{Board, Priority},
    provider::{Provider, ProviderError},
    store_db::{self, StoreError},
};

pub struct LocalProvider {
    db_path: PathBuf,
    project: String,
}

impl LocalProvider {
    pub fn new(db_path: PathBuf, project: String) -> Self {
        Self { db_path, project }
    }

    pub fn from_env() -> Self {
        let project = std::env::var("FLOW_PROJECT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "default".to_string());
        Self {
            db_path: resolve_db_path(),
            project,
        }
    }
}

/// Single database file for the app. `FLOW_BOARD_PATH` is optional: when set
/// it accepts a `.db` file directly or a directory (legacy `.board` dirs keep
/// working) holding `flow.db`; when unset or empty the default
/// `~/.config/flow/flow.db` is always used. Every tenant lives in that one file.
fn resolve_db_path() -> PathBuf {
    resolve_db_path_from(
        std::env::var("FLOW_BOARD_PATH").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

fn resolve_db_path_from(board_path: Option<&str>, home: Option<&str>) -> PathBuf {
    if let Some(p) = board_path
        && !p.trim().is_empty()
    {
        return as_db_file(PathBuf::from(p));
    }
    if let Some(home) = home {
        return PathBuf::from(home).join(".config/flow/flow.db");
    }
    // Last resort when HOME is unavailable (practically never on real systems).
    std::env::temp_dir().join("flow.db")
}

fn as_db_file(p: PathBuf) -> PathBuf {
    let sqlite_ext = p.extension().is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().to_str(),
            Some("db" | "sqlite" | "sqlite3")
        )
    });
    if p.is_file() || sqlite_ext || (!p.exists() && p.extension().is_some()) {
        p
    } else {
        p.join("flow.db")
    }
}

impl Provider for LocalProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        store_db::load_board(&self.db_path, &self.project)
            .map_err(|e| map_err("load_board", &self.db_path, e))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        store_db::move_card(&self.db_path, &self.project, card_id, to_col_id)
            .map_err(|e| map_err("move_card", &self.db_path, e))
    }

    fn create_card(&mut self, to_col_id: &str, _project: &str) -> Result<String, ProviderError> {
        // Cards always belong to the active tenant; the caller-supplied
        // project label is ignored so a card can never escape its partition.
        store_db::create_card(&self.db_path, &self.project, to_col_id)
            .map_err(|e| map_err("create_card", &self.db_path, e))
    }

    fn delete_card(&mut self, card_id: &str) -> Result<(), ProviderError> {
        // A card can't be deleted while other cards still depend on it —
        // that would leave a dangling reference.
        let board = self.load_board()?;
        let dependents = board.dependents_of(card_id);
        if !dependents.is_empty() {
            let ids: Vec<&str> = dependents.iter().map(|c| c.id.as_str()).collect();
            return Err(ProviderError::Validation {
                msg: format!(
                    "cannot delete {card_id}: still depended on by {}",
                    ids.join(", ")
                ),
            });
        }

        store_db::delete_card(&self.db_path, &self.project, card_id)
            .map_err(|e| map_err("delete_card", &self.db_path, e))
    }

    fn update_card(&mut self, card_id: &str, title: &str, description: &str, priority: Priority, assignee: &str, _project: &str, depends_on: &[String]) -> Result<(), ProviderError> {
        let deps = self.validate_depends_on(card_id, depends_on)?;
        store_db::update_card(&self.db_path, &self.project, card_id, title, description, priority, assignee, &deps)
            .map_err(|e| map_err("update_card", &self.db_path, e))
    }
}

impl LocalProvider {
    /// Validates a proposed dependency list and returns the normalized
    /// (de-duplicated, trimmed, empty-entries-dropped) list to persist:
    /// - every referenced card must exist,
    /// - a card can't depend on itself,
    /// - the resulting dependency graph must stay acyclic.
    fn validate_depends_on(&mut self, card_id: &str, depends_on: &[String]) -> Result<Vec<String>, ProviderError> {
        let mut seen = std::collections::HashSet::new();
        let mut deps = Vec::new();
        for id in depends_on {
            let id = id.trim();
            if id.is_empty() || !seen.insert(id.to_string()) {
                continue;
            }
            deps.push(id.to_string());
        }

        if deps.is_empty() {
            return Ok(deps);
        }

        let board = self.load_board()?;
        for dep in &deps {
            if dep == card_id {
                return Err(ProviderError::Validation {
                    msg: "a card cannot depend on itself".to_string(),
                });
            }
            if board.find_card(dep).is_none() {
                return Err(ProviderError::NotFound { id: dep.clone() });
            }
        }

        if let Some(cycle) = board.find_dependency_cycle(card_id, &deps) {
            return Err(ProviderError::Validation {
                msg: format!("circular dependency: {}", cycle.join(" -> ")),
            });
        }

        Ok(deps)
    }
}

fn map_err(op: &str, db_path: &Path, err: StoreError) -> ProviderError {
    match err {
        StoreError::NotFound(id) => ProviderError::NotFound { id },
        StoreError::Db(e) => ProviderError::Db {
            op: op.to_string(),
            msg: e.to_string(),
        },
        StoreError::Io(e) => ProviderError::Io {
            op: op.to_string(),
            path: db_path.to_path_buf(),
            source: e,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn tmp_db() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-provider-test-{n}.db"))
    }

    fn cleanup(db: &Path) {
        let _ = std::fs::remove_file(db);
    }

    #[test]
    fn map_err_maps_store_variants() {
        let db = PathBuf::from("/tmp/flow-test.db");
        assert!(matches!(
            map_err("op", &db, StoreError::NotFound("X-1".into())),
            ProviderError::NotFound { .. }
        ));
        assert!(matches!(
            map_err(
                "op",
                &db,
                StoreError::Db(rusqlite::Error::InvalidQuery)
            ),
            ProviderError::Db { .. }
        ));
        assert!(matches!(
            map_err(
                "op",
                &db,
                StoreError::Io(std::io::Error::other("boom"))
            ),
            ProviderError::Io { .. }
        ));
    }

    #[test]
    fn fresh_db_has_default_columns() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());

        let board = provider.load_board().map_err(|e| e.to_string())?;
        let ids: Vec<&str> = board.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["todo", "in_progress", "in_review", "done"]);

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_card_returns_not_found() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());

        let err = provider.move_card("X-1", "todo").unwrap_err();
        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "X-1"),
            _ => panic!("expected NotFound error"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_card_to_unknown_column_returns_not_found() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider.create_card("todo", "acme").map_err(|e| e.to_string())?;

        let err = provider.move_card(&id, "nope").unwrap_err();
        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "nope"),
            _ => panic!("expected NotFound error"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn create_card_uses_tenant_prefix() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());

        let id = provider.create_card("todo", "ignored-label").map_err(|e| e.to_string())?;
        assert!(id.starts_with("ACME-"), "unexpected id: {id}");

        let board = provider.load_board().map_err(|e| e.to_string())?;
        let card = board.find_card(&id).expect("created card must load");
        assert_eq!(card.project, "acme");

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_rejects_missing_dependency() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let id = provider.create_card("todo", "p").map_err(|e| e.to_string())?;

        let err = provider
            .update_card(&id, "Title", "", Priority::Medium, "", "p", &["DOES-NOT-EXIST".to_string()])
            .unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "DOES-NOT-EXIST"),
            other => panic!("expected NotFound error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_rejects_self_dependency() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let id = provider.create_card("todo", "p").map_err(|e| e.to_string())?;

        let err = provider
            .update_card(&id, "Title", "", Priority::Medium, "", "p", &[id.clone()])
            .unwrap_err();

        assert!(matches!(err, ProviderError::Validation { .. }));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_rejects_cycle() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let a = provider.create_card("todo", "p").map_err(|e| e.to_string())?;
        let b = provider.create_card("todo", "p").map_err(|e| e.to_string())?;
        provider
            .update_card(&b, "B", "", Priority::Medium, "", "p", &[a.clone()])
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(&a, "A", "", Priority::Medium, "", "p", &[b.clone()])
            .unwrap_err();

        match err {
            ProviderError::Validation { msg } => assert!(msg.contains("circular dependency"), "{msg}"),
            other => panic!("expected Validation error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn delete_card_vetoed_by_dependent() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let a = provider.create_card("todo", "p").map_err(|e| e.to_string())?;
        let b = provider.create_card("todo", "p").map_err(|e| e.to_string())?;
        provider
            .update_card(&b, "B", "", Priority::Medium, "", "p", &[a.clone()])
            .map_err(|e| e.to_string())?;

        let err = provider.delete_card(&a).unwrap_err();
        match err {
            ProviderError::Validation { msg } => assert!(msg.contains(&b), "{msg}"),
            other => panic!("expected Validation error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn delete_card_removes_it() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let id = provider.create_card("todo", "p").map_err(|e| e.to_string())?;

        provider.delete_card(&id).map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&id).is_none());

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn tenants_are_isolated_in_one_db() -> TestResult {
        let db = tmp_db();
        let mut a = LocalProvider::new(db.clone(), "aaa".to_string());
        let mut b = LocalProvider::new(db.clone(), "bbb".to_string());

        let id = a.create_card("todo", "aaa").map_err(|e| e.to_string())?;

        let board_b = b.load_board().map_err(|e| e.to_string())?;
        assert!(board_b.find_card(&id).is_none());
        assert!(b.move_card(&id, "done").is_err());

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn resolve_db_path_prefers_board_path() {
        assert_eq!(
            resolve_db_path_from(Some("/tmp/x.db"), Some("/home/u")),
            PathBuf::from("/tmp/x.db")
        );
    }

    #[test]
    fn resolve_db_path_defaults_to_config_when_unset_or_empty() {
        let expected = PathBuf::from("/home/u/.config/flow/flow.db");
        assert_eq!(resolve_db_path_from(None, Some("/home/u")), expected);
        assert_eq!(resolve_db_path_from(Some(""), Some("/home/u")), expected);
        assert_eq!(resolve_db_path_from(Some("   "), Some("/home/u")), expected);
    }

    #[test]
    fn resolve_db_path_without_home_uses_temp() {
        assert_eq!(
            resolve_db_path_from(None, None),
            std::env::temp_dir().join("flow.db")
        );
    }

    #[test]
    fn as_db_file_rules() {
        // Explicit sqlite files are used as-is.
        assert_eq!(
            as_db_file(PathBuf::from("/tmp/board.db")),
            PathBuf::from("/tmp/board.db")
        );
        assert_eq!(
            as_db_file(PathBuf::from("/tmp/board.sqlite3")),
            PathBuf::from("/tmp/board.sqlite3")
        );
        // Anything else is a directory holding flow.db (legacy .board dirs).
        assert_eq!(
            as_db_file(PathBuf::from("/tmp/.board")),
            PathBuf::from("/tmp/.board/flow.db")
        );
        assert_eq!(
            as_db_file(std::env::temp_dir()),
            std::env::temp_dir().join("flow.db")
        );
    }
}
