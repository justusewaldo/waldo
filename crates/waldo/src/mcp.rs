//! A Model Context Protocol server over stdio: one JSON message per line.

use base64::Engine;
use waldo_core::exec::{Executor, Output};
use waldo_core::tools;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

fn content(out: Output) -> Value {
    let mut items = vec![json!({ "type": "text", "text": out.text })];
    if let Some(jpeg) = out.image {
        items.push(json!({
            "type": "image",
            "data": base64::engine::general_purpose::STANDARD.encode(jpeg),
            "mimeType": "image/jpeg",
        }));
    }
    json!({ "content": items, "isError": out.is_error })
}

/// Answers one message. Returns None for notifications, which get no reply.
pub fn handle(exec: &mut Executor, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = if VERSIONS.contains(&asked) { asked } else { VERSIONS[0] };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "waldo", "version": env!("CARGO_PKG_VERSION") },
                "instructions": tools::INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(content(exec.call(name, &args)))
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    // A message with no id is a notification. It never gets a reply.
    let id = id?;
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

pub fn serve() -> io::Result<()> {
    let mut exec = Executor::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&mut exec, &msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") },
            })),
        };
        if let Some(r) = reply {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec() -> Executor {
        std::env::set_var("WALDO_HOME", std::env::temp_dir().join("waldo-test-mcp"));
        Executor::new()
    }

    #[test]
    fn initialize_echoes_a_known_version_and_carries_the_contract() {
        let mut e = exec();
        let r = handle(&mut e, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert!(r["result"]["instructions"].as_str().unwrap().contains("Hard stops"));
        let r = handle(&mut e, &json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_reply() {
        let mut e = exec();
        assert!(handle(&mut e, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    }

    #[test]
    fn tools_list_and_unknown_methods() {
        let mut e = exec();
        let r = handle(&mut e, &json!({"jsonrpc":"2.0","id":3,"method":"tools/list"})).unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 8);
        let r = handle(&mut e, &json!({"jsonrpc":"2.0","id":4,"method":"resources/list"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn an_unknown_tool_is_a_tool_error_not_a_protocol_error() {
        let mut e = exec();
        let r = handle(&mut e, &json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope","arguments":{}}})).unwrap();
        assert_eq!(r["result"]["isError"], true);
    }
}
