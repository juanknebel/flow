use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{
    model::{Board, Priority},
    provider::{Provider, ProviderError},
    store_fs,
};

pub struct LocalProvider {
    root: PathBuf,
}

impl LocalProvider {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn from_env() -> Self {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        if let Ok(p) = std::env::var("FLOW_BOARD_PATH") {
            return Self {
                root: PathBuf::from(p),
            };
        }

        if std::env::var("FLOW_PROVIDER").ok().as_deref() == Some("local") {
            if let Ok(p) = std::env::var("FLOW_LOCAL_PATH") {
                return Self {
                    root: PathBuf::from(p),
                };
            }
            if let Ok(home) = std::env::var("HOME") {
                return Self {
                    root: PathBuf::from(home).join(".config/flow/boards/default"),
                };
            }
        }

        Self {
            root: manifest_dir.join("../../boards/demo"),
        }
    }
}

impl Provider for LocalProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        store_fs::load_board(&self.root).map_err(|e| map_load_err("load_board", &self.root, e))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        store_fs::move_card(&self.root, card_id, to_col_id)
            .map_err(|e| map_move_err(card_id, &self.root, e))
    }

    fn create_card(&mut self, to_col_id: &str, project: &str) -> Result<String, ProviderError> {
        store_fs::create_card(&self.root, to_col_id, project).map_err(|err| ProviderError::Io {
            op: "create_card".to_string(),
            path: self.root.clone(),
            source: err,
        })
    }

    fn card_path(&self, card_id: &str) -> Result<PathBuf, ProviderError> {
        store_fs::card_path(&self.root, card_id).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => ProviderError::NotFound {
                id: card_id.to_string(),
            },
            _ => ProviderError::Io {
                op: "card_path".to_string(),
                path: self.root.clone(),
                source: err,
            },
        })
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

        store_fs::delete_card(&self.root, card_id).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => ProviderError::NotFound {
                id: card_id.to_string(),
            },
            _ => ProviderError::Io {
                op: "delete_card".to_string(),
                path: self.root.clone(),
                source: err,
            },
        })
    }

    fn update_card(&mut self, card_id: &str, title: &str, description: &str, priority: Priority, assignee: &str, project: &str, depends_on: &[String]) -> Result<(), ProviderError> {
        let deps = self.validate_depends_on(card_id, depends_on)?;
        let path = self.card_path(card_id)?;
        store_fs::write_card_content(&path, title, description, priority, assignee, project, &deps).map_err(|err| ProviderError::Io {
            op: "update_card".to_string(),
            path,
            source: err,
        })
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

fn map_load_err(op: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: op.to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

fn map_move_err(card_id: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::NotFound => ProviderError::NotFound {
            id: card_id.to_string(),
        },
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: "move_card".to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn tmp_root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-provider-test-{n}"))
    }

    fn write(p: &Path, s: &str) -> io::Result<()> {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(p, s)
    }

    #[test]
    fn map_load_err_returns_parse_for_invalid_data() {
        let root = PathBuf::from("/tmp/flow-test");
        let err = map_load_err(
            "load_board",
            &root,
            io::Error::new(io::ErrorKind::InvalidData, "bad"),
        );

        assert!(matches!(err, ProviderError::Parse { .. }));
    }

    #[test]
    fn move_card_returns_not_found() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;

        let mut provider = LocalProvider { root: root.clone() };
        let err = provider.move_card("X-1", "todo").unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "X-1"),
            _ => panic!("expected NotFound error"),
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_rejects_missing_dependency() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let id = provider.create_card("todo", "P").map_err(|e| e.to_string())?;

        let err = provider
            .update_card(&id, "Title", "", Priority::Medium, "", "P", &["DOES-NOT-EXIST".to_string()])
            .unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "DOES-NOT-EXIST"),
            other => panic!("expected NotFound error, got {other:?}"),
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_rejects_self_dependency() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let id = provider.create_card("todo", "P").map_err(|e| e.to_string())?;

        let err = provider
            .update_card(&id, "Title", "", Priority::Medium, "", "P", &[id.clone()])
            .unwrap_err();

        assert!(matches!(err, ProviderError::Validation { .. }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_accepts_multiple_existing_dependencies() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        // Different projects so `create_card`'s millisecond-based id
        // generator can't collide between the cards.
        let dep_a = provider.create_card("todo", "DEPA").map_err(|e| e.to_string())?;
        let dep_b = provider.create_card("todo", "DEPB").map_err(|e| e.to_string())?;
        let id = provider.create_card("todo", "MAIN").map_err(|e| e.to_string())?;

        provider
            .update_card(&id, "Title", "", Priority::Medium, "", "MAIN", &[dep_a.clone(), dep_b.clone()])
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        let card = board.find_card(&id).ok_or("card not found")?;
        assert_eq!(card.depends_on, vec![dep_a, dep_b]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_deduplicates_repeated_ids() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let dep = provider.create_card("todo", "DEP").map_err(|e| e.to_string())?;
        let id = provider.create_card("todo", "MAIN").map_err(|e| e.to_string())?;

        provider
            .update_card(&id, "Title", "", Priority::Medium, "", "MAIN", &[dep.clone(), dep.clone(), dep.clone()])
            .map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        let card = board.find_card(&id).ok_or("card not found")?;
        assert_eq!(card.depends_on, vec![dep]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_rejects_direct_cycle() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let a = provider.create_card("todo", "A").map_err(|e| e.to_string())?;
        let b = provider.create_card("todo", "B").map_err(|e| e.to_string())?;

        // A depends on B.
        provider
            .update_card(&a, "A", "", Priority::Medium, "", "A", &[b.clone()])
            .map_err(|e| e.to_string())?;

        // B depending on A would create a direct cycle A -> B -> A.
        let err = provider
            .update_card(&b, "B", "", Priority::Medium, "", "B", &[a.clone()])
            .unwrap_err();

        match err {
            ProviderError::Validation { msg } => {
                assert!(msg.contains(&a));
                assert!(msg.contains(&b));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn update_card_rejects_transitive_cycle() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let a = provider.create_card("todo", "A").map_err(|e| e.to_string())?;
        let b = provider.create_card("todo", "B").map_err(|e| e.to_string())?;
        let c = provider.create_card("todo", "C").map_err(|e| e.to_string())?;

        // A -> B -> C
        provider
            .update_card(&a, "A", "", Priority::Medium, "", "A", &[b.clone()])
            .map_err(|e| e.to_string())?;
        provider
            .update_card(&b, "B", "", Priority::Medium, "", "B", &[c.clone()])
            .map_err(|e| e.to_string())?;

        // C depending on A would create a transitive cycle A -> B -> C -> A.
        let err = provider
            .update_card(&c, "C", "", Priority::Medium, "", "C", &[a.clone()])
            .unwrap_err();

        assert!(matches!(err, ProviderError::Validation { .. }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn delete_card_rejects_when_other_card_depends_on_it() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let dep_id = provider.create_card("todo", "DEP").map_err(|e| e.to_string())?;
        let id = provider.create_card("todo", "MAIN").map_err(|e| e.to_string())?;
        provider
            .update_card(&id, "Title", "", Priority::Medium, "", "MAIN", &[dep_id.clone()])
            .map_err(|e| e.to_string())?;

        let err = provider.delete_card(&dep_id).unwrap_err();
        assert!(matches!(err, ProviderError::Validation { .. }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn delete_card_rejects_when_multiple_cards_depend_on_it() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let dep_id = provider.create_card("todo", "DEP").map_err(|e| e.to_string())?;
        let id1 = provider.create_card("todo", "ONE").map_err(|e| e.to_string())?;
        let id2 = provider.create_card("todo", "TWO").map_err(|e| e.to_string())?;
        provider
            .update_card(&id1, "One", "", Priority::Medium, "", "ONE", &[dep_id.clone()])
            .map_err(|e| e.to_string())?;
        provider
            .update_card(&id2, "Two", "", Priority::Medium, "", "TWO", &[dep_id.clone()])
            .map_err(|e| e.to_string())?;

        let err = provider.delete_card(&dep_id).unwrap_err();
        match err {
            ProviderError::Validation { msg } => {
                assert!(msg.contains(&id1));
                assert!(msg.contains(&id2));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn delete_card_succeeds_when_no_dependents() -> TestResult {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n")?;
        let mut provider = LocalProvider { root: root.clone() };
        let id = provider.create_card("todo", "P").map_err(|e| e.to_string())?;

        provider.delete_card(&id).map_err(|e| e.to_string())?;

        let board = provider.load_board().map_err(|e| e.to_string())?;
        assert!(board.find_card(&id).is_none());

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
