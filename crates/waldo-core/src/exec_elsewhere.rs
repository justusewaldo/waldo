//! The tools on a system other than macOS. The surface matches the macOS
//! `exec`, so the MCP server and the command line build unchanged, and
//! every tool answers that Waldo runs on macOS only.

use serde_json::Value;

/// What one tool call returns.
pub struct Output {
    pub text: String,
    /// A JPEG the model should look at. Never set here.
    pub image: Option<Vec<u8>>,
    pub is_error: bool,
}

/// Stands in for the macOS executor.
#[derive(Default)]
pub struct Executor;

impl Executor {
    pub fn new() -> Executor {
        Executor
    }

    /// Every tool, known or not, answers the same: Waldo needs a Mac.
    pub fn call(&mut self, _tool: &str, _args: &Value) -> Output {
        Output { text: crate::MACOS_ONLY.to_string(), image: None, is_error: true }
    }
}
