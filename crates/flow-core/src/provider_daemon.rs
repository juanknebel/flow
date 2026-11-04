//! Optional client for the local `flow` daemon. When the daemon is running,
//! providers can route operations through it instead of touching SQLite
//! directly, which serializes writes and enables eventing for agent swarms.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Board, Priority};
use crate::provider::{Provider, ProviderError};

#[derive(Deserialize)]
struct Response {
    ok: bool,
    result: Option<Value>,
    error: Option<String>,
}

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    method: &'a str,
    params: Value,
}

pub struct DaemonProvider {
    socket: PathBuf,
    project: String,
    project_explicit: bool,
}

impl DaemonProvider {
    pub fn new(socket: PathBuf, project: String) -> Self {
        Self {
            socket,
            project,
            project_explicit: true,
        }
    }

    pub fn socket_path() -> PathBuf {
        if let Ok(p) = std::env::var("FLOW_DAEMON_SOCKET")
            && !p.trim().is_empty()
        {
            return PathBuf::from(p);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".config/flow/flow.sock")
    }

    pub fn is_available() -> bool {
        Self::ensure_daemon()
    }

    fn ensure_daemon() -> bool {
        if Self::ping_socket() {
            return true;
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("daemon")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if cmd.spawn().is_ok() {
                for _ in 0..50 {
                    std::thread::sleep(Duration::from_millis(100));
                    if Self::ping_socket() {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn ping_socket() -> bool {
        let socket = Self::socket_path();
        if !socket.exists() {
            return false;
        }
        let mut stream = match UnixStream::connect(&socket) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let req = serde_json::json!({"id":0,"method":"ping","params":{}});
        if writeln!(stream, "{req}").is_err() {
            return false;
        }
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).is_ok() && line.contains("\"pong\"")
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, ProviderError> {
        let mut stream = UnixStream::connect(&self.socket).map_err(|e| ProviderError::Io {
            op: "daemon_connect".to_string(),
            path: self.socket.clone(),
            source: e,
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| ProviderError::Io {
                op: "daemon_timeout".to_string(),
                path: self.socket.clone(),
                source: e,
            })?;
        let req = Request {
            id: 1,
            method,
            params,
        };
        let body =
            serde_json::to_string(&req).map_err(|e| ProviderError::Parse { msg: e.to_string() })?;
        writeln!(stream, "{body}").map_err(|e| ProviderError::Io {
            op: "daemon_write".to_string(),
            path: self.socket.clone(),
            source: e,
        })?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| ProviderError::Io {
            op: "daemon_read".to_string(),
            path: self.socket.clone(),
            source: e,
        })?;
        let resp: Response =
            serde_json::from_str(&line).map_err(|e| ProviderError::Parse { msg: e.to_string() })?;
        if !resp.ok {
            return Err(ProviderError::Parse {
                msg: resp.error.unwrap_or_else(|| "daemon error".to_string()),
            });
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }
}

impl Provider for DaemonProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        let params = serde_json::json!({"project": self.project});
        let v = self.call("load_board", params)?;
        serde_json::from_value(v).map_err(|e| ProviderError::Parse { msg: e.to_string() })
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        let params = serde_json::json!({
            "project": self.project,
            "card_id": card_id,
            "column_id": to_col_id,
        });
        self.call("move_card", params)?;
        Ok(())
    }

    fn create_card(&mut self, to_col_id: &str, _project: &str) -> Result<String, ProviderError> {
        let params = serde_json::json!({
            "project": self.project,
            "column_id": to_col_id,
        });
        let v = self.call("create_card", params)?;
        v.as_str()
            .map(String::from)
            .ok_or_else(|| ProviderError::Parse {
                msg: "bad create_card response".to_string(),
            })
    }

    fn delete_card(&mut self, card_id: &str) -> Result<(), ProviderError> {
        let params = serde_json::json!({
            "project": self.project,
            "card_id": card_id,
        });
        self.call("delete_card", params)?;
        Ok(())
    }

    fn list_projects(&mut self) -> Result<Vec<String>, ProviderError> {
        let v = self.call("list_projects", serde_json::json!({}))?;
        serde_json::from_value(v).map_err(|e| ProviderError::Parse { msg: e.to_string() })
    }

    fn select_project(&mut self, project: &str) -> Result<(), ProviderError> {
        self.project = project.trim().to_string();
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
        self.project.eq_ignore_ascii_case("all")
    }

    fn supports_project_filter(&self) -> bool {
        self.project.eq_ignore_ascii_case("all")
    }

    fn project_card_count(&mut self, project: &str) -> Result<usize, ProviderError> {
        let params = serde_json::json!({"project": project});
        let v = self.call("project_card_count", params)?;
        v.as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| ProviderError::Parse {
                msg: "bad count response".to_string(),
            })
    }

    fn delete_project(&mut self, project: &str) -> Result<usize, ProviderError> {
        let params = serde_json::json!({"project": project, "confirm": true});
        let v = self.call("delete_project", params)?;
        v.as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| ProviderError::Parse {
                msg: "bad delete_project response".to_string(),
            })
    }

    fn update_card(
        &mut self,
        card_id: &str,
        title: &str,
        description: &str,
        priority: Priority,
        assignee: &str,
        project: &str,
        depends_on: &[String],
    ) -> Result<(), ProviderError> {
        let params = serde_json::json!({
            "project": project,
            "card_id": card_id,
            "title": title,
            "description": description,
            "priority": priority,
            "assignee": assignee,
            "depends_on": depends_on,
        });
        self.call("update_card", params)?;
        Ok(())
    }
}
