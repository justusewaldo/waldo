//! `waldo`: the command line and the MCP server in one binary.

mod mcp;

use waldo_core::exec::Executor;
use waldo_core::gate::Policy;
use waldo_core::{apps, ax, input, screen, state};
use serde_json::{json, Value};
use std::process::ExitCode;

const HELP: &str = "\
waldo: hands for every AI on this Mac

  waldo mcp                 Run as an MCP server on stdio. Your agent starts this.
  waldo status              Show grants, displays, the app in front, and the stop switch.
  waldo grant               Ask macOS for the grants and open the right Settings panes.
  waldo stop [reason]       Stop every session now. Nothing runs until resume.
  waldo resume              Clear the stop switch.
  waldo call TOOL [JSON]    Run one tool. Add --image PATH to save the screenshot.
  waldo policy              Print the gate policy in force, as JSON.
  waldo log [N]             Print the last N lines of the audit log. Default 20.
  waldo windows             List the windows on screen with their ids.

Stop gesture: push the pointer into the top-left corner of the main display.";

fn yes_no(v: bool) -> &'static str {
    if v {
        "granted"
    } else {
        "NOT granted"
    }
}

fn status() {
    println!("Accessibility:     {}", yes_no(ax::trusted()));
    println!("Screen Recording:  {}", yes_no(screen::can_capture()));
    println!("Post events:       {}", yes_no(input::can_post()));
    for d in screen::displays() {
        println!(
            "Display {}:         {}x{} points at {},{}, {}x real pixels",
            d.index, d.bounds.w, d.bounds.h, d.bounds.x, d.bounds.y, d.native
        );
    }
    match apps::frontmost() {
        Some(a) => println!("In front:          {} ({})", a.name, a.bundle),
        None => println!("In front:          unknown"),
    }
    println!("Idle:              {:.0} seconds", input::idle_seconds());
    match Policy::load() {
        Ok(_) => println!("Gate policy:       ok"),
        Err(e) => println!("Gate policy:       BROKEN. Nothing will run. {e}"),
    }
    if state::stopped() {
        println!("Stop switch:       ON ({})", state::stop_reason());
    } else {
        println!("Stop switch:       off");
    }
    println!("State folder:      {}", state::dir().display());
}

fn grant() {
    let ax_ok = ax::request_trust();
    let screen_ok = screen::can_capture() || screen::request_capture();
    println!("Accessibility:     {}", yes_no(ax_ok));
    println!("Screen Recording:  {}", yes_no(screen_ok));
    let open = |pane: &str| {
        let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
        let _ = std::process::Command::new("/usr/bin/open").arg(url).status();
    };
    if !ax_ok {
        open("Privacy_Accessibility");
    }
    if !screen_ok {
        open("Privacy_ScreenCapture");
    }
    if ax_ok && screen_ok {
        println!("Nothing to do.");
    } else {
        println!("Turn on the app that runs waldo in the pane that opened, then run this again.");
        println!("macOS gives the grant to the app that started the process: your terminal, or the bar app.");
    }
}

fn call(args: &[String]) -> ExitCode {
    let Some(tool) = args.first() else {
        eprintln!("usage: waldo call TOOL [JSON] [--image PATH]");
        return ExitCode::from(2);
    };
    let mut json_arg: Option<&str> = None;
    let mut image_path: Option<&str> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--image" {
            image_path = args.get(i + 1).map(String::as_str);
            i += 2;
        } else {
            json_arg = Some(&args[i]);
            i += 1;
        }
    }
    let parsed: Value = match json_arg {
        None => json!({}),
        Some(text) => match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("the arguments are not JSON: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let out = Executor::new().call(tool, &parsed);
    println!("{}", out.text.trim_end());
    if let (Some(jpeg), Some(path)) = (&out.image, image_path) {
        match std::fs::write(path, jpeg) {
            Ok(()) => println!("screenshot: {path}"),
            Err(e) => eprintln!("could not write {path}: {e}"),
        }
    }
    if out.is_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn log(count: usize) {
    let text = std::fs::read_to_string(state::audit_path()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(count)..] {
        println!("{line}");
    }
}

extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
}

fn main() -> ExitCode {
    // Exit quietly when the reader of our output goes away, as when the
    // output is piped to `head`. 13 is SIGPIPE and 0 is the default action.
    unsafe { signal(13, 0) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("mcp") => match mcp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("waldo mcp: {e}");
                ExitCode::from(1)
            }
        },
        Some("status") => {
            status();
            ExitCode::SUCCESS
        }
        Some("grant") => {
            grant();
            ExitCode::SUCCESS
        }
        Some("stop") => {
            let reason = if args.len() > 1 { args[1..].join(" ") } else { "waldo stop".to_string() };
            state::stop(&reason);
            println!("Stopped. Every tool now refuses. Run `waldo resume` to clear it.");
            ExitCode::SUCCESS
        }
        Some("resume") => {
            if state::resume() {
                println!("Resumed.");
            } else {
                println!("It was not stopped.");
            }
            ExitCode::SUCCESS
        }
        Some("call") => call(&args[1..]),
        Some("policy") => match Policy::load() {
            Ok(p) => {
                println!("{}", serde_json::to_string_pretty(&p).unwrap_or_default());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        Some("windows") => {
            for w in screen::windows() {
                println!("{}\t{}\t{}\t{}x{} at {},{}", w.id, w.app, w.title, w.frame.w, w.frame.h, w.frame.x, w.frame.y);
            }
            ExitCode::SUCCESS
        }
        Some("log") => {
            log(args.get(1).and_then(|n| n.parse().ok()).unwrap_or(20));
            ExitCode::SUCCESS
        }
        _ => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
    }
}
