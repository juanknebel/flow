use std::{fmt, io, path::PathBuf};

use crate::model::{Board, Priority};

#[derive(Debug)]
pub enum ProviderError {
    NotFound {
        id: String,
    },
    Parse {
        msg: String,
    },
    Io {
        op: String,
        path: PathBuf,
        source: io::Error,
    },
    /// A business-rule validation failed (e.g. dependency cycle, or deleting
    /// a card other cards still depend on).
    Validation {
        msg: String,
    },
    /// The SQLite backend failed (local provider only).
    Db {
        op: String,
        msg: String,
    },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::NotFound { id } => write!(f, "not found: {id}"),
            ProviderError::Parse { msg } => write!(f, "parse error: {msg}"),
            ProviderError::Io { op, path, source } => {
                write!(f, "{op} failed for {}: {source}", path.display())
            }
            ProviderError::Validation { msg } => write!(f, "validation error: {msg}"),
            ProviderError::Db { op, msg } => write!(f, "{op} failed: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProviderError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub trait Provider {
    fn load_board(&mut self) -> Result<Board, ProviderError>;
    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError>;

    fn create_card(&mut self, _to_col_id: &str, _project: &str) -> Result<String, ProviderError> {
        Err(ProviderError::Parse {
            msg: "create_card not supported by current provider".to_string(),
        })
    }

    fn delete_card(&mut self, _card_id: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Parse {
            msg: "delete_card not supported by current provider".to_string(),
        })
    }

    /// All tenant codes available for interactive selection.
    fn list_projects(&mut self) -> Result<Vec<String>, ProviderError> {
        Err(ProviderError::Parse {
            msg: "project selection not supported by current provider".to_string(),
        })
    }

    /// Switch the active tenant to an existing (or new) project code.
    fn select_project(&mut self, _project: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Parse {
            msg: "project selection not supported by current provider".to_string(),
        })
    }

    /// Whether the active tenant still needs interactive selection
    /// (i.e. it was not chosen explicitly via configuration).
    fn needs_project_selection(&self) -> bool {
        false
    }

    /// The active tenant code, if this provider has that concept.
    fn active_project(&self) -> Option<String> {
        None
    }

    fn update_card(&mut self, _card_id: &str, _title: &str, _description: &str, _priority: Priority, _assignee: &str, _project: &str, _depends_on: &[String]) -> Result<(), ProviderError> {
        Err(ProviderError::Parse {
            msg: "update_card not supported by current provider".to_string(),
        })
    }
}

pub fn from_env() -> Box<dyn Provider> {
    from_env_with_project(None)
}

/// Like [`from_env`], but pins the local backend to an explicit tenant
/// (used by the CLI flag and by TUI background moves after an interactive
/// selection). The Jira backend ignores the override.
pub fn from_env_with_project(project: Option<String>) -> Box<dyn Provider> {
    match std::env::var("FLOW_PROVIDER").ok().as_deref() {
        Some("jira") => Box::new(crate::provider_jira::JiraProvider::from_env()),
        _ => match project {
            Some(p) => Box::new(crate::provider_local::LocalProvider::with_project(p)),
            None => Box::new(crate::provider_local::LocalProvider::from_env()),
        },
    }
}
