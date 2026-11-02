use std::io::{BufRead, BufReader, Write};

use flow_core::provider;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn provider_for(project: &str) -> Box<dyn provider::Provider> {
    #[cfg(unix)]
    if flow_core::provider_daemon::DaemonProvider::is_available() {
        return Box::new(flow_core::provider_daemon::DaemonProvider::new(
            flow_core::provider_daemon::DaemonProvider::socket_path(),
            project.to_string(),
        ));
    }
    let proj = if project.is_empty() {
        None
    } else {
        Some(project.to_string())
    };
    provider::from_env_with_project(proj)
}

fn send(obj: Value) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", obj);
    let _ = out.flush();
}

fn handle(req: RpcRequest) {
    match req.method.as_str() {
        "initialize" => {
            send(json!({
                "jsonrpc":"2.0",
                "id": req.id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name":"flow","version":"0.9.0"}
                }
            }));
        }
        "notifications/initialized" => {}
        "tools/list" => {
            send(json!({
                "jsonrpc":"2.0",
                "id": req.id,
                "result": {
                    "tools": [
                        {"name":"list_projects","description":"List tenant project codes","inputSchema":{"type":"object","properties":{}}},
                        {"name":"load_board","description":"Load a board","inputSchema":{"type":"object","properties":{"project":{"type":"string"}},"required":["project"]}},
                        {"name":"create_card","description":"Create a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"column_id":{"type":"string"},"title":{"type":"string"}},"required":["project","column_id"]}},
                        {"name":"move_card","description":"Move a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"},"column_id":{"type":"string"}},"required":["project","card_id","column_id"]}},
                        {"name":"delete_card","description":"Delete a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"}},"required":["project","card_id"]}}
                    ]
                }
            }));
        }
        "tools/call" => {
            let name = req.params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or(Value::Null);
            let result = call_tool(name, &args);
            let is_error = result.is_err();
            let text = match result {
                Ok(v) => v.to_string(),
                Err(e) => e,
            };
            send(json!({
                "jsonrpc":"2.0",
                "id": req.id,
                "result": {
                    "content": [{"type":"text","text": text}],
                    "isError": is_error
                }
            }));
        }
        _ => {
            if req.id.is_some() {
                send(
                    json!({"jsonrpc":"2.0","id":req.id,"error":{"code":-32601,"message":"method not found"}}),
                );
            }
        }
    }
}

fn call_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => {
            let mut p = provider::from_env();
            let projects = p.list_projects().map_err(|e| e.to_string())?;
            Ok(json!(projects))
        }
        "load_board" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let mut p = provider_for(project);
            let board = p.load_board().map_err(|e| e.to_string())?;
            Ok(json!(board))
        }
        "create_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let column_id = args
                .get("column_id")
                .and_then(Value::as_str)
                .unwrap_or("todo");
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("New card");
            let mut p = provider_for(project);
            let id = p
                .create_card(column_id, project)
                .map_err(|e| e.to_string())?;
            if title != "New card" {
                let _ = p.update_card(
                    &id,
                    title,
                    "",
                    flow_core::Priority::Medium,
                    "",
                    project,
                    &[],
                );
            }
            Ok(json!({"card_id": id}))
        }
        "move_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or("");
            let column_id = args.get("column_id").and_then(Value::as_str).unwrap_or("");
            let mut p = provider_for(project);
            p.move_card(card_id, column_id).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "delete_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or("");
            let mut p = provider_for(project);
            p.delete_card(card_id).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

pub fn run() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                send(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}),
                );
                continue;
            }
        };
        handle(req);
    }
}
