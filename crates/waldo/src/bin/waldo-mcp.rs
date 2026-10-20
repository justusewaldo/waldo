//! `waldo-mcp`: the MCP server alone. It is `waldo mcp` under the name MCP
//! registries and clients use for this package, so it needs no arguments.

fn main() -> std::process::ExitCode {
    waldo_mcp::quiet_sigpipe();
    waldo_mcp::mcp::main()
}
