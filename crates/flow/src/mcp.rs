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
                    "capabilities": {"tools": {}, "resources": {}},
                    "serverInfo": {"name":"flow","version":"0.9.0"}
                }
            }));
        }
        "notifications/initialized" => {}
        "resources/list" => {
            let project = req
                .params
                .get("project")
                .and_then(Value::as_str)
                .unwrap_or("");
            // List a single resource per project is not enumerable here; return empty.
            let _ = project;
            send(json!({"jsonrpc":"2.0","id":req.id,"result":{"resources":[]}}));
        }
        "resources/read" => {
            let uri = req.params.get("uri").and_then(Value::as_str).unwrap_or("");
            // uri format: flow://board/<project>
            let project = uri.strip_prefix("flow://board/").unwrap_or("");
            let mut p = provider_for(project);
            match p.load_board() {
                Ok(board) => {
                    send(
                        json!({"jsonrpc":"2.0","id":req.id,"result":{"contents":[{"uri":uri,"mimeType":"application/json","text": serde_json::to_string(&board).unwrap_or_default()}]}}),
                    );
                }
                Err(e) => {
                    send(
                        json!({"jsonrpc":"2.0","id":req.id,"error":{"code":-32000,"message":e.to_string()}}),
                    );
                }
            }
        }
        "tools/list" => {
            send(json!({
                "jsonrpc":"2.0",
                "id": req.id,
                "result": {
                    "tools": [
                        {"name":"list_projects","description":"List tenant project codes","inputSchema":{"type":"object","properties":{}}},
                        {"name":"load_board","description":"Load a board","inputSchema":{"type":"object","properties":{"project":{"type":"string"}},"required":["project"]}},
                        {"name":"edit_card","description":"Edit a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"priority":{"type":"string"},"assignee":{"type":"string"},"depends_on":{"type":"string"}},"required":["project","card_id"]}},
                        {"name":"create_card","description":"Create a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"column_id":{"type":"string"},"title":{"type":"string"}},"required":["project","column_id"]}},
                        {"name":"move_card","description":"Move a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"},"column_id":{"type":"string"}},"required":["project","card_id","column_id"]}},
                        {"name":"delete_card","description":"Delete a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"}},"required":["project","card_id"]}},
                        {"name":"changes_since","description":"Cards changed after timestamp (millis)","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"since":{"type":"integer"}},"required":["project"]}},
                        {"name":"search_cards","description":"Search cards by text","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"query":{"type":"string"}},"required":["project","query"]}},
                        {"name":"health","description":"Daemon health check","inputSchema":{"type":"object","properties":{}}},
                        {"name":"claim_card","description":"Acquire a lease on a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"},"agent":{"type":"string"},"lease_ms":{"type":"integer"}},"required":["project","card_id","agent"]}},
                        {"name":"release_card","description":"Release a lease on a card","inputSchema":{"type":"object","properties":{"project":{"type":"string"},"card_id":{"type":"string"},"agent":{"type":"string"}},"required":["project","card_id","agent"]}}
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
                    None,
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
        "changes_since" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let since = args.get("since").and_then(Value::as_i64).unwrap_or(0);
            let mut p = provider_for(project);
            let board = p.load_board().map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for col in &board.columns {
                for card in &col.cards {
                    if card.updated_at > since {
                        out.push(json!({"column_id": col.id, "card": card}));
                    }
                }
            }
            Ok(json!(out))
        }
        "claim_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or("");
            let agent = args.get("agent").and_then(Value::as_str).unwrap_or("");
            let lease_ms = args
                .get("lease_ms")
                .and_then(Value::as_u64)
                .unwrap_or(300000);
            let mut p = provider_for(project);
            p.claim_card(card_id, agent, lease_ms)
                .map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "release_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or("");
            let agent = args.get("agent").and_then(Value::as_str).unwrap_or("");
            let mut p = provider_for(project);
            p.release_card(card_id, agent).map_err(|e| e.to_string())?;
            Ok(json!({"ok": true}))
        }
        "health" => Ok(json!({"status":"ok"})),
        "search_cards" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let ql = query.to_lowercase();
            let mut p = provider_for(project);
            let board = p.load_board().map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for col in &board.columns {
                for card in &col.cards {
                    if card.title.to_lowercase().contains(&ql)
                        || card.description.to_lowercase().contains(&ql)
                    {
                        out.push(json!({"column_id": col.id, "card": card}));
                    }
                }
            }
            Ok(json!(out))
        }
        "edit_card" => {
            let project = args.get("project").and_then(Value::as_str).unwrap_or("");
            let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or("");
            let mut p = provider_for(project);
            let board = p.load_board().map_err(|e| e.to_string())?;
            let card = board
                .columns
                .iter()
                .flat_map(|c| c.cards.iter())
                .find(|c| c.id == card_id)
                .ok_or_else(|| format!("card not found: {card_id}"))?
                .clone();
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&card.title)
                .to_string();
            let body = args
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or(&card.description)
                .to_string();
            let priority = args
                .get("priority")
                .and_then(Value::as_str)
                .map(flow_core::Priority::from_str)
                .unwrap_or(card.priority);
            let assignee = args
                .get("assignee")
                .and_then(Value::as_str)
                .unwrap_or(&card.assignee)
                .to_string();
            let depends_on: Vec<String> = args
                .get("depends_on")
                .and_then(Value::as_str)
                .map(|s| {
                    s.split(',')
                        .map(str::trim)
                        .filter(|x| !x.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or(card.depends_on.clone());
            p.update_card(
                &card_id,
                &title,
                &body,
                priority,
                &assignee,
                project,
                &depends_on,
                Some(card.version),
            )
            .map_err(|e| e.to_string())?;
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
