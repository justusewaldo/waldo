//! Waldo: hands for every AI on your Mac. This package holds the command
//! line and the MCP server.
//!
//! - `waldo`: the command line. `waldo mcp` runs the MCP server.
//! - `waldo-mcp`: the MCP server alone, under the name MCP registries and
//!   clients use for this package.
//!
//! The tools themselves live in the `waldo-core` crate.

pub mod mcp;

/// Exit quietly when the reader of our output goes away, as when the output
/// is piped to `head`. 13 is SIGPIPE and 0 is the default action.
pub fn quiet_sigpipe() {
    extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    unsafe { signal(13, 0) };
}
