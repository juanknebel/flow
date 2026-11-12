use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

static DB_LOCK: Mutex<()> = Mutex::new(());
static SUBSCRIBERS: Mutex<Vec<UnixStream>> = Mutex::new(Vec::new());

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

    if req.method == "subscribe" {
        let reader_stream = stream.try_clone().unwrap();
        {
            let mut subs = SUBSCRIBERS.lock().unwrap();
            subs.push(stream.try_clone().unwrap());
        }
        let mut w = stream;
        let _ = writeln!(
            w,
            "{}",
            serde_json::to_string(&Response {
                id: req.id,
                ok: true,
                result: Some(serde_json::json!({"subscribed": true})),
                error: None,
            })
            .unwrap()
        );
        // Keep connection open until client disconnects.
        let mut reader = BufReader::new(reader_stream);
        let mut l = String::new();
        while reader.read_line(&mut l).unwrap_or(0) > 0 {
            l.clear();
        }
        return;
    }

    let resp = dispatch(req.id, &req.method, req.params);
    let mut w = stream;
    let _ = writeln!(w, "{}", serde_json::to_string(&resp).unwrap());
}

fn broadcast_event(method: &str, params: &Value) {
    let event = serde_json::json!({"event": method, "params": params});
    let line = format!("{}\n", event);
    let mut subs = SUBSCRIBERS.lock().unwrap();
    subs.retain(|s| {
        let mut w = s;
        w.write_all(line.as_bytes()).is_ok() && w.flush().is_ok()
    });
}

fn dispatch(id: u64, method: &str, params: Value) -> Response {
    let _guard = DB_LOCK.lock().unwrap();
    let res = (|| -> Result<Value, String> {
        match method {
            "ping" => Ok(Value::String("pong".to_string())),
            "health" => Ok(serde_json::json!({"status":"ok","socket": socket_path()})),
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
                let expected = params.get("expected_version").and_then(Value::as_u64);
                p.update_card(
                    &card_id,
                    &title,
                    &description,
                    priority,
                    &assignee,
                    &project,
                    &depends_on,
                    expected,
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
            "claim_card" => {
                let project = str_param(&params, "project");
                let card_id = str_param(&params, "card_id");
                let agent = str_param(&params, "agent");
                let lease_ms = params
                    .get("lease_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(300000);
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project)
                };
                let mut p = provider::from_env_with_project(project_opt);
                p.claim_card(&card_id, &agent, lease_ms)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "release_card" => {
                let project = str_param(&params, "project");
                let card_id = str_param(&params, "card_id");
                let agent = str_param(&params, "agent");
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project)
                };
                let mut p = provider::from_env_with_project(project_opt);
                p.release_card(&card_id, &agent)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
            "changes_since" => {
                let project = str_param(&params, "project");
                let since = params.get("since").and_then(Value::as_i64).unwrap_or(0);
                let project_opt = if project.is_empty() {
                    None
                } else {
                    Some(project.clone())
                };
                let mut p = provider::from_env_with_project(project_opt);
                let board = p.load_board().map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for col in &board.columns {
                    for card in &col.cards {
                        if card.updated_at > since {
                            out.push(serde_json::json!({
                                "column_id": col.id,
                                "card": card,
                            }));
                        }
                    }
                }
                Ok(Value::Array(out))
            }
            _ => Err(format!("unknown method: {method}")),
        }
    })();

    let is_mutation = matches!(
        method,
        "create_card"
            | "update_card"
            | "move_card"
            | "delete_card"
            | "claim_card"
            | "release_card"
            | "delete_project"
    );
    if is_mutation && res.is_ok() {
        broadcast_event(method, &params);
    }
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
