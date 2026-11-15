use std::path::{Path, PathBuf};

use crate::{
    model::{Board, Priority},
    provider::{Provider, ProviderError},
    store_db::{self, StoreError},
};

pub struct LocalProvider {
    db_path: PathBuf,
    project: String,
    project_explicit: bool,
}

impl LocalProvider {
    pub fn new(db_path: PathBuf, project: String) -> Self {
        Self {
            db_path,
            project,
            project_explicit: true,
        }
    }

    pub fn from_env() -> Self {
        let (project, explicit) = resolve_project(std::env::var("FLOW_PROJECT").ok().as_deref());
        Self {
            db_path: resolve_db_path(),
            project,
            project_explicit: explicit,
        }
    }

    /// Explicit tenant (CLI `--project` flag, TUI background moves).
    /// The database path still resolves from `FLOW_BOARD_PATH` or the default.
    /// Callers must pass a non-empty code.
    pub fn with_project(project: String) -> Self {
        debug_assert!(!project.trim().is_empty(), "tenant code must not be empty");
        Self {
            db_path: resolve_db_path(),
            project: project.trim().to_string(),
            project_explicit: true,
        }
    }
}

/// Tenant from configuration: the `FLOW_PROJECT` value when set and non-empty,
/// otherwise the `default` tenant flagged as needing interactive selection.
fn resolve_project(var: Option<&str>) -> (String, bool) {
    match var.map(str::trim).filter(|s| !s.is_empty()) {
        Some(code) => (code.to_string(), true),
        None => ("default".to_string(), false),
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

/// Reserved tenant code (any case): the read-only union of all tenants.
/// It can never name a real tenant — every path resolves it to the union
/// before any tenant row is touched.
fn is_all(project: &str) -> bool {
    project.eq_ignore_ascii_case("all")
}

fn read_only() -> ProviderError {
    ProviderError::Validation {
        msg: "all-projects view is read-only".to_string(),
    }
}

impl Provider for LocalProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        if is_all(&self.project) {
            return store_db::load_board_all(&self.db_path)
                .map_err(|e| map_err("load_board", &self.db_path, e));
        }
        store_db::load_board(&self.db_path, &self.project)
            .map_err(|e| map_err("load_board", &self.db_path, e))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        let card_id = self.resolve_card_id(card_id)?;
        store_db::move_card(&self.db_path, &self.project, &card_id, to_col_id)
            .map_err(|e| map_err("move_card", &self.db_path, e))
    }

    fn list_projects(&mut self) -> Result<Vec<String>, ProviderError> {
        store_db::list_projects(&self.db_path)
            .map_err(|e| map_err("list_projects", &self.db_path, e))
    }

    fn select_project(&mut self, project: &str) -> Result<(), ProviderError> {
        let code = project.trim();
        if code.is_empty() {
            return Err(ProviderError::Validation {
                msg: "project code required".to_string(),
            });
        }
        self.project = code.to_string();
        self.project_explicit = true;
        Ok(())
    }

    fn needs_project_selection(&self) -> bool {
        !self.project_explicit
    }

    fn active_project(&self) -> Option<String> {
        Some(self.project.clone())
    }

    fn is_read_only(&self) -> bool {
        is_all(&self.project)
    }

    fn supports_project_filter(&self) -> bool {
        is_all(&self.project)
    }

    fn project_card_count(&mut self, project: &str) -> Result<usize, ProviderError> {
        store_db::project_card_count(&self.db_path, project)
            .map_err(|e| map_err("project_card_count", &self.db_path, e))
    }

    fn delete_project(&mut self, project: &str) -> Result<usize, ProviderError> {
        if is_all(project) {
            return Err(ProviderError::Validation {
                msg: "cannot delete the all-projects view".to_string(),
            });
        }
        store_db::delete_project(&self.db_path, project)
            .map_err(|e| map_err("delete_project", &self.db_path, e))
    }

    fn create_card(&mut self, to_col_id: &str, _project: &str) -> Result<String, ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        // Cards always belong to the active tenant; the caller-supplied
        // project label is ignored so a card can never escape its partition.
        store_db::create_card(&self.db_path, &self.project, to_col_id)
            .map_err(|e| map_err("create_card", &self.db_path, e))
    }

    fn delete_card(&mut self, card_id: &str) -> Result<(), ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        let card_id = self.resolve_card_id(card_id)?;
        // A card can't be deleted while other cards still depend on it —
        // that would leave a dangling reference.
        let board = store_db::load_board_all(&self.db_path)
            .map_err(|e| map_err("load_board", &self.db_path, e))?;
        let dependents = board.dependents_of(&card_id);
        if !dependents.is_empty() {
            let ids: Vec<&str> = dependents.iter().map(|c| c.id.as_str()).collect();
            return Err(ProviderError::Validation {
                msg: format!(
                    "cannot delete {card_id}: still depended on by {}",
                    ids.join(", ")
                ),
            });
        }

        store_db::delete_card(&self.db_path, &self.project, &card_id)
            .map_err(|e| map_err("delete_card", &self.db_path, e))
    }

    fn update_card(
        &mut self,
        card_id: &str,
        title: &str,
        description: &str,
        priority: Priority,
        assignee: &str,
        _project: &str,
        depends_on: &[String],
        expected_version: Option<u64>,
    ) -> Result<(), ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        // Canonicalize before validating: self- and cycle checks compare
        // against stored ids, so a lowercase-typed id must be resolved first.
        let card_id = self.resolve_card_id(card_id)?;
        let deps = self.validate_depends_on(&card_id, depends_on)?;
        store_db::update_card(
            &self.db_path,
            &self.project,
            &card_id,
            title,
            description,
            priority,
            assignee,
            &deps,
            expected_version,
        )
        .map_err(|e| map_err("update_card", &self.db_path, e))
    }

    fn claim_card(
        &mut self,
        card_id: &str,
        agent: &str,
        lease_ms: u64,
    ) -> Result<(), ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        let card_id = self.resolve_card_id(card_id)?;
        store_db::claim_card(&self.db_path, &self.project, &card_id, agent, lease_ms)
            .map_err(|e| map_err("claim_card", &self.db_path, e))
    }

    fn release_card(&mut self, card_id: &str, agent: &str) -> Result<(), ProviderError> {
        if is_all(&self.project) {
            return Err(read_only());
        }
        let card_id = self.resolve_card_id(card_id)?;
        store_db::release_card(&self.db_path, &self.project, &card_id, agent)
            .map_err(|e| map_err("release_card", &self.db_path, e))
    }
}

impl LocalProvider {
    /// Resolves a user-typed card id to the canonical stored id within the
    /// active tenant (matched case-insensitively). Users may type `acme-1`;
    /// operations run against `ACME-1`. Card operations are tenant-scoped,
    /// so unlike dependency validation this never looks at other tenants.
    /// Missing cards fail with `NotFound` carrying the typed id.
    fn resolve_card_id(&self, card_id: &str) -> Result<String, ProviderError> {
        let board = store_db::load_board(&self.db_path, &self.project)
            .map_err(|e| map_err("load_board", &self.db_path, e))?;
        board
            .find_card_case_insensitive(card_id)
            .map(|c| c.id.clone())
            .ok_or_else(|| ProviderError::NotFound {
                id: card_id.to_string(),
            })
    }

    /// Validates a proposed dependency list and returns the normalized list
    /// to persist: trimmed, empty-entries-dropped, resolved to the canonical
    /// stored id of each referenced card, and de-duplicated. Users may type
    /// ids in any case (`acme-1`); what gets stored is the canonical id
    /// (`ACME-1`). Rules:
    /// - every referenced card must exist (matched case-insensitively),
    /// - a card can't depend on itself,
    /// - the resulting dependency graph must stay acyclic.
    fn validate_depends_on(
        &mut self,
        card_id: &str,
        depends_on: &[String],
    ) -> Result<Vec<String>, ProviderError> {
        let mut trimmed = Vec::new();
        for id in depends_on {
            let id = id.trim();
            if !id.is_empty() {
                trimmed.push(id);
            }
        }

        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let board = store_db::load_board_all(&self.db_path)
            .map_err(|e| map_err("load_board", &self.db_path, e))?;
        let mut seen = std::collections::HashSet::new();
        let mut deps = Vec::new();
        for dep in trimmed {
            let canonical = board
                .find_card_case_insensitive(dep)
                .map(|c| c.id.clone())
                .ok_or_else(|| ProviderError::NotFound {
                    id: dep.to_string(),
                })?;
            if canonical.to_lowercase() == card_id.to_lowercase() {
                return Err(ProviderError::Validation {
                    msg: "a card cannot depend on itself".to_string(),
                });
            }
            if seen.insert(canonical.clone()) {
                deps.push(canonical);
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
        StoreError::Conflict(msg) => ProviderError::Validation { msg },
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
    fn cross_project_dependency_is_allowed() -> TestResult {
        let db = tmp_db();
        let mut p1 = LocalProvider::new(db.clone(), "aaa".to_string());
        let a = p1.create_card("todo", "aaa").map_err(|e| e.to_string())?;
        let mut p2 = LocalProvider::new(db.clone(), "bbb".to_string());
        let b = p2.create_card("todo", "bbb").map_err(|e| e.to_string())?;
        // b depends on a (cross-project).
        p2.update_card(
            &b,
            "B",
            "",
            Priority::Medium,
            "",
            "bbb",
            std::slice::from_ref(&a),
            None,
        )
        .map_err(|e| e.to_string())?;
        let board = p2.load_board().map_err(|e| e.to_string())?;
        assert_eq!(board.find_card(&b).unwrap().depends_on, vec![a.clone()]);
        // deleting a is vetoed because b depends on it (cross-project).
        let mut p1b = LocalProvider::new(db.clone(), "aaa".to_string());
        let err = p1b.delete_card(&a).unwrap_err();
        assert!(matches!(err, ProviderError::Validation { .. }));
        cleanup(&db);
        Ok(())
    }

    #[test]
    fn map_err_maps_store_variants() {
        let db = PathBuf::from("/tmp/flow-test.db");
        assert!(matches!(
            map_err("op", &db, StoreError::NotFound("X-1".into())),
            ProviderError::NotFound { .. }
        ));
        assert!(matches!(
            map_err("op", &db, StoreError::Db(rusqlite::Error::InvalidQuery)),
            ProviderError::Db { .. }
        ));
        assert!(matches!(
            map_err("op", &db, StoreError::Io(std::io::Error::other("boom"))),
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
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

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

        let id = provider
            .create_card("todo", "ignored-label")
            .map_err(|e| e.to_string())?;
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
        let id = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &id,
                "Title",
                "",
                Priority::Medium,
                "",
                "p",
                &["DOES-NOT-EXIST".to_string()],
                None,
            )
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
        let id = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &id,
                "Title",
                "",
                Priority::Medium,
                "",
                "p",
                std::slice::from_ref(&id),
                None,
            )
            .unwrap_err();

        assert!(matches!(err, ProviderError::Validation { .. }));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_rejects_cycle() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let a = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;
        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "p",
                std::slice::from_ref(&a),
                None,
            )
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &a,
                "A",
                "",
                Priority::Medium,
                "",
                "p",
                std::slice::from_ref(&b),
                None,
            )
            .unwrap_err();

        match err {
            ProviderError::Validation { msg } => {
                assert!(msg.contains("circular dependency"), "{msg}")
            }
            other => panic!("expected Validation error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_accepts_mixed_case_dep_list_and_stores_canonical() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let a = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let c = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        assert_eq!(
            (a.as_str(), b.as_str(), c.as_str()),
            ("ACME-1", "ACME-2", "ACME-3")
        );

        provider
            .update_card(
                &c,
                "C",
                "",
                Priority::Medium,
                "",
                "acme",
                &["acme-1".to_string(), "Acme-2".to_string()],
                None,
            )
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert_eq!(
            board.find_card(&c).unwrap().depends_on,
            vec!["ACME-1".to_string(), "ACME-2".to_string()]
        );

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_dedupes_dependencies_across_cases() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let _a = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "acme",
                &[
                    "acme-1".to_string(),
                    "ACME-1".to_string(),
                    " ACME-1 ".to_string(),
                    String::new(),
                ],
                None,
            )
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert_eq!(
            board.find_card(&b).unwrap().depends_on,
            vec!["ACME-1".to_string()]
        );

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_rejects_self_dependency_in_any_case() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &id,
                "Title",
                "",
                Priority::Medium,
                "",
                "acme",
                &[id.to_lowercase()],
                None,
            )
            .unwrap_err();

        assert!(matches!(err, ProviderError::Validation { .. }));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_detects_cycle_through_lowercase_dependency() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let a = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "acme",
                std::slice::from_ref(&a),
                None,
            )
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &a,
                "A",
                "",
                Priority::Medium,
                "",
                "acme",
                &[b.to_lowercase()],
                None,
            )
            .unwrap_err();

        match err {
            ProviderError::Validation { msg } => {
                assert!(msg.contains("circular dependency"), "{msg}")
            }
            other => panic!("expected Validation error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_missing_dependency_reports_typed_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                &id,
                "Title",
                "",
                Priority::Medium,
                "",
                "acme",
                &["nope-1".to_string()],
                None,
            )
            .unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "nope-1"),
            other => panic!("expected NotFound error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_card_resolves_lowercase_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        assert_eq!(id, "ACME-1");

        provider
            .move_card("acme-1", "done")
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.columns[0].cards.is_empty());
        assert_eq!(board.columns[3].cards[0].id, "ACME-1");

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn move_card_missing_id_reports_typed_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());

        let err = provider.move_card("nope-1", "done").unwrap_err();
        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "nope-1"),
            other => panic!("expected NotFound error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_resolves_lowercase_card_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

        provider
            .update_card(
                "acme-1",
                "Retitled",
                "",
                Priority::Medium,
                "",
                "acme",
                &[],
                None,
            )
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        let card = board.find_card(&id).expect("card still present");
        assert_eq!(card.title, "Retitled");
        assert_eq!(card.id, "ACME-1");

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn update_card_detects_cycle_through_lowercase_card_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let a = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "acme",
                std::slice::from_ref(&a),
                None,
            )
            .map_err(|e| e.to_string())?;

        let err = provider
            .update_card(
                "acme-1",
                "A",
                "",
                Priority::Medium,
                "",
                "acme",
                std::slice::from_ref(&b),
                None,
            )
            .unwrap_err();

        match err {
            ProviderError::Validation { msg } => {
                assert!(msg.contains("circular dependency"), "{msg}")
            }
            other => panic!("expected Validation error, got {other:?}"),
        }

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn delete_card_resolves_lowercase_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let a = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;
        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "acme",
                std::slice::from_ref(&a),
                None,
            )
            .map_err(|e| e.to_string())?;

        provider.delete_card("acme-2").map_err(|e| e.to_string())?;
        provider.delete_card("acme-1").map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&a).is_none());
        assert!(board.find_card(&b).is_none());

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn claim_and_release_resolve_lowercase_id() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        let id = provider
            .create_card("todo", "acme")
            .map_err(|e| e.to_string())?;

        provider
            .claim_card("acme-1", "bot", 300000)
            .map_err(|e| e.to_string())?;
        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert_eq!(board.find_card(&id).unwrap().claimed_by, "bot");

        provider
            .release_card("acme-1", "bot")
            .map_err(|e| e.to_string())?;
        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&id).unwrap().claimed_by.is_empty());

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn delete_card_vetoed_by_dependent() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "p".to_string());
        let a = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;
        let b = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;
        provider
            .update_card(
                &b,
                "B",
                "",
                Priority::Medium,
                "",
                "p",
                std::slice::from_ref(&a),
                None,
            )
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
        let id = provider
            .create_card("todo", "p")
            .map_err(|e| e.to_string())?;

        provider.delete_card(&id).map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&id).is_none());

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn resolve_project_flags_implicit_default() {
        assert_eq!(resolve_project(None), ("default".to_string(), false));
        assert_eq!(resolve_project(Some("")), ("default".to_string(), false));
        assert_eq!(resolve_project(Some("  ")), ("default".to_string(), false));
        assert_eq!(resolve_project(Some("acme")), ("acme".to_string(), true));
        assert_eq!(
            resolve_project(Some("  acme  ")),
            ("acme".to_string(), true)
        );
    }

    #[test]
    fn select_project_switches_tenant() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "acme".to_string());
        assert!(!provider.needs_project_selection());

        provider
            .select_project("  beta ")
            .map_err(|e| e.to_string())?;
        assert_eq!(provider.project, "beta");
        assert!(!provider.needs_project_selection());

        let err = provider.select_project("   ").unwrap_err();
        assert!(matches!(err, ProviderError::Validation { .. }));

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn with_project_pins_explicit_tenant() {
        let provider = LocalProvider::with_project("  acme ".to_string());
        assert_eq!(provider.project, "acme");
        assert_eq!(provider.active_project(), Some("acme".to_string()));
        assert!(!provider.needs_project_selection());
    }

    #[test]
    fn list_projects_lists_tenants() -> TestResult {
        let db = tmp_db();
        let mut provider = LocalProvider::new(db.clone(), "zeta".to_string());
        assert!(
            provider
                .list_projects()
                .map_err(|e| e.to_string())?
                .is_empty()
        );

        provider.load_board().map_err(|e| e.to_string())?;
        provider.select_project("acme").map_err(|e| e.to_string())?;
        provider.load_board().map_err(|e| e.to_string())?;

        assert_eq!(
            provider.list_projects().map_err(|e| e.to_string())?,
            vec!["acme".to_string(), "zeta".to_string()]
        );

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn all_view_is_read_only_union() -> TestResult {
        let db = tmp_db();
        let mut a = LocalProvider::new(db.clone(), "aaa".to_string());
        let id_a = a.create_card("todo", "aaa").map_err(|e| e.to_string())?;
        let mut z = LocalProvider::new(db.clone(), "zzz".to_string());
        let id_z = z.create_card("done", "zzz").map_err(|e| e.to_string())?;

        let mut all = LocalProvider::new(db.clone(), "all".to_string());
        assert!(all.is_read_only());
        let board = all.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&id_a).is_some());
        assert!(board.find_card(&id_z).is_some());

        for res in [
            all.move_card(&id_a, "done").map(|_| ()),
            all.create_card("todo", "all").map(|_| ()),
            all.update_card(&id_a, "t", "", Priority::Low, "", "all", &[], None),
            all.delete_card(&id_a),
        ] {
            assert!(matches!(res.unwrap_err(), ProviderError::Validation { .. }));
        }
        // Selecting "all" (any case) enters the union, never a real tenant.
        a.select_project("ALL").map_err(|e| e.to_string())?;
        assert!(a.is_read_only());
        assert!(
            !a.list_projects()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|c| c.eq_ignore_ascii_case("all"))
        );

        cleanup(&db);
        Ok(())
    }

    #[test]
    fn project_filter_offered_only_in_all_view() {
        let tenant = LocalProvider::new(PathBuf::from("/tmp/x.db"), "acme".to_string());
        assert!(!tenant.supports_project_filter());
        let all = LocalProvider::new(PathBuf::from("/tmp/x.db"), "all".to_string());
        assert!(all.supports_project_filter());
    }

    #[test]
    fn delete_project_removes_tenant() -> TestResult {
        let db = tmp_db();
        let mut a = LocalProvider::new(db.clone(), "aaa".to_string());
        a.create_card("todo", "aaa").map_err(|e| e.to_string())?;
        let mut z = LocalProvider::new(db.clone(), "zzz".to_string());
        z.create_card("todo", "zzz").map_err(|e| e.to_string())?;

        assert_eq!(a.project_card_count("aaa").map_err(|e| e.to_string())?, 1);
        assert_eq!(a.delete_project("aaa").map_err(|e| e.to_string())?, 1);
        assert_eq!(
            a.list_projects().map_err(|e| e.to_string())?,
            vec!["zzz".to_string()]
        );
        assert!(matches!(
            a.delete_project("aaa").unwrap_err(),
            ProviderError::NotFound { .. }
        ));
        assert!(matches!(
            a.delete_project("all").unwrap_err(),
            ProviderError::Validation { .. }
        ));

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
