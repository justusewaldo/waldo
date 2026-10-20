# Waldo

**Hands for every AI. Starting with your Mac.**

![waldo. Hands for every AI. Starting with your Mac.](https://raw.githubusercontent.com/justusewaldo/waldo/main/docs/media/banner-light.png)

Waldo is the pair of hands any AI can borrow. He sees every window, presses every button and types into every field. When money, messages, passwords or deletes come up, he stops and asks you.

Waldo is an MCP server for macOS. Any agent that speaks MCP can use it: Claude Code, Codex, Gemini CLI, Cursor, and others. It reads the accessibility tree, presses controls in background apps without moving your pointer, and checks that each act really happened.

## Install

You need a Mac, Rust, and the Xcode command line tools.

```
cargo install waldo-mcp
waldo grant      # Accessibility + Screen Recording, and builds the ask panel once
```

Then hand Waldo to your agent. For Claude Code:

```
claude mcp add --scope user waldo -- waldo-mcp
```

Any other MCP agent takes a server named `waldo` with the command `waldo-mcp`:

```json
{
  "mcpServers": {
    "waldo": { "command": "waldo-mcp" }
  }
}
```

macOS gives the grants to the app that starts Waldo. For most people that is the terminal.

## The four hard stops

Money, sending as you, passwords and keys, and deleting things always stop and ask you first. The hard stops live in Waldo's code, not in a prompt. The ask panel takes no keys, and it refuses any click that did not come from your own mouse. Push the pointer into the top-left corner of the screen, or run `waldo stop`, and every tool for every agent stops until you resume.

Read the [Limits](https://github.com/justusewaldo/waldo#limits) before you turn it on.

## Links

- Website: https://justusewaldo.com
- Source, README, and changelog: https://github.com/justusewaldo/waldo
- Security: https://github.com/justusewaldo/waldo/blob/main/SECURITY.md
- MCP Registry name: `mcp-name: io.github.justusewaldo/waldo`

Apache-2.0. Copyright 2026 Veso AI.
