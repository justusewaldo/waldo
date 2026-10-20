# waldo-core

The tools behind [Waldo](https://justusewaldo.com), hands for every AI on your Mac. Most people want the [`waldo-mcp`](https://crates.io/crates/waldo-mcp) crate, which holds the command line and the MCP server.

This crate reads the macOS accessibility tree, acts on apps in the background, checks that each act happened, and gates four kinds of act behind a human yes: money, sending as you, passwords and keys, and deleting things. `exec::Executor` is the whole public surface: give it a tool name and JSON arguments, and it returns text and sometimes a screenshot.

It builds on other systems so MCP directories can list Waldo's tools, but every tool there answers that Waldo runs on macOS only.

- Source: https://github.com/justusewaldo/waldo
- Apache-2.0. Copyright 2026 Veso AI.
