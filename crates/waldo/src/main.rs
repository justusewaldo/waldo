//! `waldo`: the command line and the MCP server in one binary.
//!
//! On macOS every subcommand works. Elsewhere only `waldo mcp` runs, so MCP
//! directories that start servers on Linux can list the tools. Each tool
//! then answers that Waldo runs on macOS only.

#[cfg(target_os = "macos")]
mod mac;

use std::process::ExitCode;

const HELP: &str = "\
waldo: hands for every AI on this Mac

  waldo mcp                 Run as an MCP server on stdio. Your agent starts this.
  waldo status              Show grants, displays, the app in front, and the stop switch.
  waldo grant               Ask macOS for the grants and open the right Settings panes.
  waldo bypass [on|off]     Turn bypass permissions mode on or off across sessions.
  waldo exhaust [on|off|clear] Show or manage learned UI exhaust (rank-0 controls, field physics).
  waldo stop [reason]       Stop every session now. Nothing runs until resume.
  waldo resume              Clear the stop switch.
  waldo call TOOL [JSON]    Run one tool. Add --image PATH to save the screenshot.
  waldo policy              Print the gate policy in force, as JSON.
  waldo log [N]             Print the last N lines of the audit log. Default 20.
  waldo windows             List the windows on screen with their ids.

Stop gesture: push the pointer into the top-left corner of the main display.";

fn main() -> ExitCode {
    waldo_mcp::quiet_sigpipe();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        return waldo_mcp::mcp::main();
    }
    run(&args)
}

#[cfg(target_os = "macos")]
fn run(args: &[String]) -> ExitCode {
    mac::run(args)
}

#[cfg(not(target_os = "macos"))]
fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        println!("{HELP}\n\n{}", waldo_core::MACOS_ONLY);
        return ExitCode::SUCCESS;
    }
    eprintln!("{}", waldo_core::MACOS_ONLY);
    ExitCode::from(1)
}
