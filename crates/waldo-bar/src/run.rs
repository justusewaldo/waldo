//! One task is one headless Claude Code run. This file starts it, reads its
//! event stream, and turns each event into a line the bar can show. It knows
//! nothing about gpui.

use futures::channel::mpsc;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Added to Claude Code's own prompt for runs started from the bar.
const BAR_PROMPT: &str = "You were started from the Waldo bar on the user's Mac. \
The user sees your text in a small window that shows plain text only, so do not use Markdown. \
Keep replies to a few plain sentences. Say what you did and what you saw, not how you did it.";

#[derive(Debug, PartialEq)]
pub enum Ev {
    /// The id to pass to the next run, so it carries on the same session.
    Session(String),
    Say(String),
    Tool(String),
    ToolError(String),
    Done { ok: bool, note: String },
    /// The process ended. The text is empty on a clean exit.
    Exit(String),
}

fn cut(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

/// One short line for a tool call, such as `click "Equals" in Calculator`.
pub fn tool_line(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    let in_app = if s("app").is_empty() { String::new() } else { format!(" in {}", s("app")) };
    let target = || -> String {
        if !s("label").is_empty() {
            format!("\"{}\"", s("label"))
        } else if let Some(id) = input.get("id") {
            format!("element {id}")
        } else if let (Some(x), Some(y)) = (input.get("x"), input.get("y")) {
            format!("at {x},{y}")
        } else {
            String::new()
        }
    };
    if let Some(verb) = name.strip_prefix("mcp__waldo__") {
        let detail = match verb {
            "see" => {
                let what = if s("app").is_empty() { "the front app" } else { s("app") };
                if s("filter").is_empty() {
                    what.to_string()
                } else {
                    format!("{what}, filter \"{}\"", s("filter"))
                }
            }
            "click" | "scroll" => format!("{}{in_app}", target()),
            "type" => {
                let chars = s("text").chars().count();
                let into = target();
                let into = if into.is_empty() { String::new() } else { format!(" into {into}") };
                format!("{chars} characters{into}{in_app}")
            }
            "key" => match input.get("keys") {
                Some(Value::String(k)) => format!("{k}{in_app}"),
                Some(Value::Array(ks)) => {
                    let list: Vec<&str> = ks.iter().filter_map(Value::as_str).collect();
                    format!("{}{in_app}", list.join(", "))
                }
                _ => String::new(),
            },
            "app" => format!("{} {}", s("action"), if s("name").is_empty() { s("target") } else { s("name") }),
            "confirm" => format!("{}: {}", s("gate"), cut(s("summary"), 80)),
            _ => cut(&input.to_string(), 80),
        };
        return format!("{verb} {}", detail.trim()).trim().to_string();
    }
    match name {
        "Bash" => format!("$ {}", cut(s("command"), 100)),
        "Read" | "Edit" | "Write" => {
            let path = s("file_path");
            format!("{} {}", name.to_lowercase(), path.rsplit('/').next().unwrap_or(path))
        }
        _ => format!("{name} {}", cut(&input.to_string(), 70)).trim().to_string(),
    }
}

fn result_text(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// A refusal from the tools ends with orders for the model. The person at
/// the bar needs only the first part, and it is about them.
fn for_a_person(text: &str) -> String {
    let short = text.split(" Do not retry").next().unwrap_or(text);
    match short.strip_prefix("The user ") {
        Some(rest) => format!("You {rest}"),
        None => short.to_string(),
    }
}

/// Turns one line of `--output-format stream-json` into events.
pub fn parse_line(line: &str) -> Vec<Ev> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let mut out = Vec::new();
    let content = || v["message"]["content"].as_array().cloned().unwrap_or_default();
    match v["type"].as_str().unwrap_or("") {
        "system" if v["subtype"] == "init" => {
            if let Some(id) = v["session_id"].as_str() {
                out.push(Ev::Session(id.to_string()));
            }
        }
        "assistant" => {
            for item in content() {
                match item["type"].as_str().unwrap_or("") {
                    "text" => {
                        let t = item["text"].as_str().unwrap_or("").trim();
                        if !t.is_empty() {
                            out.push(Ev::Say(t.to_string()));
                        }
                    }
                    // ToolSearch is Claude Code loading a tool. It is not a step.
                    "tool_use" if item["name"] == "ToolSearch" => {}
                    "tool_use" => out.push(Ev::Tool(tool_line(
                        item["name"].as_str().unwrap_or("tool"),
                        &item["input"],
                    ))),
                    _ => {}
                }
            }
        }
        "user" => {
            for item in content() {
                if item["type"] == "tool_result" && item["is_error"] == true {
                    out.push(Ev::ToolError(cut(&for_a_person(&result_text(&item)), 220)));
                }
            }
        }
        "result" => {
            let ok = v["is_error"] != true && v["subtype"] == "success";
            let secs = v["duration_ms"].as_f64().unwrap_or(0.0) / 1000.0;
            let turns = v["num_turns"].as_u64().unwrap_or(0);
            let note = if ok {
                format!("Done in {secs:.0} seconds, {turns} turns.")
            } else {
                let why = v["result"].as_str().or(v["subtype"].as_str()).unwrap_or("it failed");
                format!("The run ended early: {}", cut(why, 200))
            };
            if let Some(id) = v["session_id"].as_str() {
                out.push(Ev::Session(id.to_string()));
            }
            out.push(Ev::Done { ok, note });
        }
        _ => {}
    }
    out
}

/// The `claude` command. An app started from the Dock has no shell PATH, so
/// the usual install places are tried too.
pub fn find_claude() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WALDO_BAR_CLAUDE").map(PathBuf::from) {
        return Some(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut places: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    places.push(home.join(".local/bin"));
    places.push(home.join(".claude/local"));
    places.push("/opt/homebrew/bin".into());
    places.push("/usr/local/bin".into());
    places.into_iter().map(|d| d.join("claude")).find(|p| p.is_file())
}

/// Starts a run and returns its process id. Events arrive on `tx` until an
/// `Exit`. `resume` carries on an earlier session.
pub fn start(
    task: &str,
    resume: Option<&str>,
    tx: mpsc::UnboundedSender<Ev>,
) -> Result<u32, String> {
    let claude = find_claude().ok_or("The claude command was not found. Install Claude Code first.")?;
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
    let mut cmd = Command::new(claude);
    cmd.arg("-p").arg(task);
    cmd.args(["--output-format", "stream-json", "--verbose"]);
    cmd.args(["--append-system-prompt", BAR_PROMPT]);
    if let Some(id) = resume {
        cmd.args(["--resume", id]);
    }
    if let Ok(model) = std::env::var("WALDO_BAR_MODEL") {
        cmd.args(["--model", &model]);
    }
    // Its own process group, so Stop can end the run and all it started.
    cmd.current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("Could not start claude: {e}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or("no output pipe")?;
    let stderr = child.stderr.take();

    let errors = Arc::new(Mutex::new(String::new()));
    let sink = errors.clone();
    let err_thread = thread::spawn(move || {
        if let Some(mut e) = stderr {
            let mut text = String::new();
            let _ = e.read_to_string(&mut text);
            *sink.lock().unwrap() = text;
        }
    });
    thread::spawn(move || {
        let mut finished = false;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            for ev in parse_line(&line) {
                finished |= matches!(ev, Ev::Done { .. });
                let _ = tx.unbounded_send(ev);
            }
        }
        let status = child.wait();
        let _ = err_thread.join();
        let clean = finished || status.map(|s| s.success()).unwrap_or(false);
        let text = if clean { String::new() } else { cut(&errors.lock().unwrap(), 300) };
        let text = if !clean && text.is_empty() { "The run was stopped.".to_string() } else { text };
        let _ = tx.unbounded_send(Ev::Exit(text));
    });
    Ok(pid)
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Ends a run and every process it started.
pub fn stop(pid: u32) {
    const SIGTERM: i32 = 15;
    unsafe { kill(-(pid as i32), SIGTERM) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn os_tools_read_as_plain_actions() {
        assert_eq!(tool_line("mcp__waldo__click", &json!({"label":"Equals","app":"Calculator"})), "click \"Equals\" in Calculator");
        assert_eq!(tool_line("mcp__waldo__click", &json!({"id":4})), "click element 4");
        assert_eq!(tool_line("mcp__waldo__click", &json!({"x":10,"y":20})), "click at 10,20");
        assert_eq!(tool_line("mcp__waldo__see", &json!({})), "see the front app");
        assert_eq!(tool_line("mcp__waldo__see", &json!({"app":"Mail","filter":"send"})), "see Mail, filter \"send\"");
        assert_eq!(tool_line("mcp__waldo__key", &json!({"keys":["cmd+a","delete"]})), "key cmd+a, delete");
        assert_eq!(tool_line("mcp__waldo__app", &json!({"action":"open","name":"Notes"})), "app open Notes");
    }

    #[test]
    fn typed_text_is_counted_not_shown() {
        let line = tool_line("mcp__waldo__type", &json!({"text":"hunter2","label":"Password"}));
        assert_eq!(line, "type 7 characters into \"Password\"");
        assert!(!line.contains("hunter2"));
    }

    #[test]
    fn other_tools_stay_short() {
        assert_eq!(tool_line("Bash", &json!({"command":"ls  -la\n/tmp"})), "$ ls -la /tmp");
        assert_eq!(tool_line("Read", &json!({"file_path":"/a/b/c.rs"})), "read c.rs");
        assert!(tool_line("Bash", &json!({"command":"x".repeat(500)})).chars().count() < 110);
    }

    #[test]
    fn a_stream_parses_into_the_lines_the_bar_shows() {
        let init = r#"{"type":"system","subtype":"init","session_id":"abc"}"#;
        assert_eq!(parse_line(init), vec![Ev::Session("abc".into())]);

        let turn = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":" On it. "},{"type":"tool_use","name":"mcp__waldo__see","input":{"app":"Calculator"}}]}}"#;
        assert_eq!(parse_line(turn), vec![Ev::Say("On it.".into()), Ev::Tool("see Calculator".into())]);

        let bad = r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":[{"type":"text","text":"The user said no. Nothing was done. Do not retry it and do not reach the same result another way."}]}]}}"#;
        assert_eq!(parse_line(bad), vec![Ev::ToolError("You said no. Nothing was done.".into())]);

        let plumbing = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"ToolSearch","input":{"query":"select:mcp__waldo__click"}}]}}"#;
        assert!(parse_line(plumbing).is_empty());

        let good = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#;
        assert!(parse_line(good).is_empty());

        let end = r#"{"type":"result","subtype":"success","is_error":false,"result":"5535","session_id":"abc","num_turns":12,"duration_ms":10785}"#;
        assert_eq!(parse_line(end), vec![Ev::Session("abc".into()), Ev::Done { ok: true, note: "Done in 11 seconds, 12 turns.".into() }]);

        let fail = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"abc"}"#;
        assert!(matches!(&parse_line(fail)[1], Ev::Done { ok: false, note } if note.contains("error_max_turns")));

        assert!(parse_line("not json").is_empty());
    }
}
