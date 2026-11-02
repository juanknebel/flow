use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;

use flow_core::provider;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("FLOW_DAEMON_SOCKET")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config/flow/flow.sock")
}

fn str_param(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn bool_param(params: &Value, key: &str) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn handle_stream(stream: UnixStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response {
                id: 0,
                ok: false,
                result: None,
                error: Some(format!("bad request: {e}")),
            };
            let mut w = stream;
            let _ = writeln!(w, "{}", serde_json::to_string(&resp).unwrap());
            return;
        }
    };

    let resp = dispatch(req.id, &req.method, req.params);
    let mut w = stream;
    let _ = writeln!(w, "{}", serde_json::to_string(&resp).unwrap());
}

fn dispatch(id: u64, method: &str, params: Value) -> Response {
    let res = (|| -> Result<Value, String> {
        match method {
            "ping" => Ok(Value::String("pong".to_string())),
            "list_projects" => {
                let mut p = provider::from_env();
                let projects = p.list_projects().map_err(|e| e.to_string())?;
                Ok(serde_json::to_value(projects).unwrap())
            }
            "load_board" => {
                let project = str_param(&params, "project");
                let project = if project.is_empty() {
                    None
                } else {
                    Some(project)
                };
                let mut p = provider::from_env_with_project(project);
                let board = p.load_board().map_err(|e| e.to_string())?;
                Ok(serde_json::to_value(board).unwrap())
            }
            "create_card" => {
                let project = str_param(&params, "project");
                let column_id = str_param(&params, "column_id");
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project.clone())
                };
                let mut p = provider::from_env_with_project(project_opt);
                let id = p
                    .create_card(&column_id, &project)
                    .map_err(|e| e.to_string())?;
                Ok(Value::String(id))
            }
            "update_card" => {
                let project = str_param(&params, "project");
                let card_id = str_param(&params, "card_id");
                let title = str_param(&params, "title");
                let description = str_param(&params, "description");
                let priority = params
                    .get("priority")
                    .and_then(Value::as_str)
                    .unwrap_or("medium");
                let priority = flow_core::Priority::from_str(priority);
                let assignee = str_param(&params, "assignee");
                let depends_on: Vec<String> = params
                    .get("depends_on")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project.clone())
                };
                let mut p = provider::from_env_with_project(project_opt);
                p.update_card(
                    &card_id,
                    &title,
                    &description,
                    priority,
                    &assignee,
                    &project,
                    &depends_on,
                )
                .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "move_card" => {
                let project = str_param(&params, "project");
                let card_id = str_param(&params, "card_id");
                let column_id = str_param(&params, "column_id");
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project)
                };
                let mut p = provider::from_env_with_project(project_opt);
                p.move_card(&card_id, &column_id)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "delete_card" => {
                let project = str_param(&params, "project");
                let card_id = str_param(&params, "card_id");
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project)
                };
                let mut p = provider::from_env_with_project(project_opt);
                p.delete_card(&card_id).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "project_card_count" => {
                let project = str_param(&params, "project");
                let mut p = provider::from_env();
                let n = p.project_card_count(&project).map_err(|e| e.to_string())?;
                Ok(serde_json::json!(n))
            }
            "delete_project" => {
                let project = str_param(&params, "project");
                let confirm = bool_param(&params, "confirm");
                if !confirm {
                    return Err("confirm required".to_string());
                }
                let mut p = provider::from_env();
                let n = p.delete_project(&project).map_err(|e| e.to_string())?;
                Ok(serde_json::json!(n))
            }
            _ => Err(format!("unknown method: {method}")),
        }
    })();

    match res {
        Ok(result) => Response {
            id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(e) => Response {
            id,
            ok: false,
            result: None,
            error: Some(e),
        },
    }
}

pub fn run() {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("daemon bind failed: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("flow daemon listening on {}", path.display());
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                thread::spawn(|| handle_stream(s));
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}
