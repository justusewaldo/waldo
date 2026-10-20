# Changelog

Notable changes to Waldo, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [SemVer](https://semver.org/).

## [Unreleased]

Vision first, with the accessibility tree working underneath. The model can act on what it sees in the screenshot, and Waldo still presses through accessibility, so the pointer stays put.

### Changed

- **Point, then press.** A click at x,y presses the control under that point through accessibility when it can: buttons, links, menus, checkboxes, and tabs. The pointer stays put and the app can stay in the background. Text fields, steppers, rows, and cells still take a real click.
- **One call, not two.** When a press changes nothing, `click` clicks with the real pointer in the same call. It never does this after a hard stop, so an approved act is never done twice. When a field does not take a direct `set`, `type` types the text instead.
- **A lean `see`.** It lists up to 40 controls and text in view. `max` lists more, including what is scrolled away, and `filter` finds one thing.

### Measured

The same task in Safari, through Claude Code on a claude.ai subscription, one run each: read a receipt that exists only as pixels, pick the vendor from a menu, type the total, and press an icon-only button.

| | Before | After |
|---|---|---|
| With the element list | 12 calls, 126 s, 4,736 output tokens | 9 calls, 59 s, 1,902 output tokens |
| By pixels only | 0 of 4 clicks without the pointer | 3 of 4 clicks without the pointer |

## [0.4.0] - 2026-10-20: install from crates.io

### Added

- **`cargo install waldo-mcp`.** Waldo is on crates.io as `waldo-mcp` (the command line and the MCP server) and `waldo-core` (the tools). The Makefile install works as before, and it is still the one that builds the bar.
- **`waldo-mcp`, a second binary.** It starts the MCP server with no arguments, the same as `waldo mcp`. MCP registries and clients start servers by the package name, so `{"command": "waldo-mcp"}` is enough.
- **`waldo grant` builds the ask panel.** The panel's Swift source is now inside Waldo. When no panel sits next to the `waldo` binary, `waldo grant` builds one with swiftc into the Waldo folder (`~/Library/Application Support/Waldo/waldo-ask`), and the gates use it. Until a panel exists, a hard stop shows a plain system alert instead. `waldo status` says which panel is in use.
- **A Linux build for MCP directories.** Directories such as Glama start a server on Linux to list its tools. Waldo now builds there and lists the same tools, and every tool answers that Waldo runs on macOS only. `docker/Dockerfile` is the image, and CI checks it on every push. Nothing changes on a Mac.

### Changed

- **The gate panel's source moved** from `helpers/ask.swift` to `crates/waldo-core/swift/ask.swift`, so it ships with the crate.
- **`make install`** also puts `waldo-mcp` in `~/.local/bin`.

## [0.3.0] - 2026-10-20: OS Control is now Waldo

OS Control has a new name and a new home: Waldo, at [justusewaldo/waldo](https://github.com/justusewaldo/waldo). The tools, the gates, the stop switch, and the checks work as before. These are the changes you will notice.

### Changed

- **The command is `waldo`.** It was `osctl`. Every subcommand stays: `waldo mcp`, `waldo status`, `waldo grant`, `waldo stop`, `waldo resume`, `waldo call`, `waldo policy`, `waldo log`, and `waldo windows`.
- **The gate panel is `waldo-ask`.** It was `osctl-ask`. Its title says Waldo, and it reads "Waldo wants to do this on your Mac:".
- **The bar is `waldo-bar`.** It was `os-bar`.
- **The MCP server is `waldo`.** It was `os`. The tools appear as `mcp__waldo__*`. To allow them all, put `"mcp__waldo"` in `permissions.allow` in place of `"mcp__os"`.
- **Data lives in `~/Library/Application Support/Waldo`.** It was `~/Library/Application Support/OS Control`. That folder holds `policy.json`, the stop switch, and `audit.jsonl`. `WALDO_HOME` moves it. It was `OSCTL_HOME`.
- **The crates are `waldo`, `waldo-core`, and `waldo-bar`.** They were `osctl`, `os-core`, and `os-bar`.
- **Any MCP agent.** The install steps and the docs cover Codex, Gemini CLI, Cursor, and any other agent that speaks MCP, as well as Claude Code.

### Upgrade

Install Waldo from the new home, then point your agent at it:

```
git clone https://github.com/justusewaldo/waldo
cd waldo
make install
make register
```

Then clean up the old names:

1. Remove the old `os` server from your agent. In Claude Code: `claude mcp remove os --scope user`.
2. Delete `osctl`, `osctl-ask`, and `os-bar` from `~/.local/bin`.
3. To keep your edited policy and your log, move `~/Library/Application Support/OS Control` to `~/Library/Application Support/Waldo`.
4. Restart your agent sessions. A running session keeps the old binary until it restarts.

The versions below shipped as OS Control, from [Veso-AI-Open-Source/os-control](https://github.com/Veso-AI-Open-Source/os-control). Their entries keep the names of the time.

## [0.2.0] - 2026-10-20: the verified core

The app layer used to trust two kinds of unverified signal: the text output of `lsappinfo`, and success codes from calls that macOS can accept and then ignore. 0.2.0 moves that trust into types, so the compiler enforces it. It also makes each act that macOS can ignore check what it changed. The full story, with numbers, is in [WORKLOG.md](https://github.com/Veso-AI-Open-Source/os-control/blob/main/WORKLOG.md) (2026-10-16) and [#1](https://github.com/Veso-AI-Open-Source/os-control/pull/1).

### Added

- **Acts check their result.** A ladder tries each way to cause an effect and checks after each one. A way that returns OK but changes nothing does not count.
  - `app` activate: accessibility, then AppKit, then LaunchServices.
  - `app` hide: accessibility, then AppKit, then cmd+h.
  - `type` with a target: the accessibility focus, then a real click. No key is typed until the focus is confirmed.
- **`click` says whether anything changed:** "Verified: the window changed." or "No change was seen within half a second."
- **`type` reports text that was already in the field**, and reads the field back.
- **A gate `Ticket`.** Every mouse and keyboard event, and every press or edit through the accessibility API, needs one, and only the gate can make one. Code that skips the gate does not compile.
- **`Seen<T>`** (yes, no, unknown), so "could not tell" is never read as "no".
- **`clippy.toml`** bans `Command::new` outside six marked sites, each with its reason, so OS state cannot be read by parsing program output.
- **CI** on every pull request and every push to `main`: build every crate, test target and example; run the tests; run the lint; build the gate panel; and, on pull requests, check that no home paths or personal email addresses were added.
- **`CONTRIBUTING.md`** and a pull request template: `ui/`, `core/`, `docs/` and `ci/` branches, a draft pull request early, CI, then a rebase merge.

### Changed

- App identity comes from the kernel's process list (`proc_listallpids`) resolved through `NSRunningApplication`, with on-screen window owners as a second witness.
- The front app comes from the accessibility API's focused app, then the app NSWorkspace marks active.
- `scroll` passes the gate like every other act.
- When the app list cannot be read, the error says so ("could not list the running apps"). It no longer says that no app matches.

### Fixed

- An app opened through the Gatekeeper "Open" prompt could not be found, because `lsappinfo` printed its pid only inside `token=[…]`.
- `app activate` timed out when macOS ignored the accessibility "frontmost" flag. Before: it worked 8 times in 12, the report was true 4 times in 12, and the median was 1.6 s. Now it works 12 times in 12, every report is true, and the median is 0.15 s.
- `app hide` reported "could not hide" after hiding the app. Now 9 of 9 reports are truthful, up from 0.
- The front app could not be told on macOS 27. Now 12 of 12, up from 0.
- `type` with a target typed into the wrong field when an app ignored the accessibility focus, as LibreOffice's Name Box does. Now 3 of 3, up from 0.
- `type` into a field that already held text: 3 of 3 now pass, up from 0.

### Removed

- The `lsappinfo` text parser.

### For contributors

- `AppInfo` can only be made inside `apps.rs`. Use `apps::resolve`, `apps::find` or `apps::front`.
- Input functions, `Element::{perform, set_flag, set_text}`, and the raise, hide and quit functions in `apps` take a `&Ticket`. Only `gate::admit` makes one. Inside the executor, `pass_gate` calls it for every act.
- To make an act check its result, give `verify::ladder` the ways to cause the effect and a check that returns `Seen<()>`.
- `apps::find` returns `Result<Vec<AppInfo>, String>`.

### Known issues

- [#4](https://github.com/Veso-AI-Open-Source/os-control/issues/4) `click` with an id ignores the `app` argument.
- [#5](https://github.com/Veso-AI-Open-Source/os-control/issues/5) The gate fires on body text that contains a gated word.
- [#6](https://github.com/Veso-AI-Open-Source/os-control/issues/6) The destructive rule `format` catches app menus.
- [#7](https://github.com/Veso-AI-Open-Source/os-control/issues/7) System permission prompts, such as Gatekeeper's Open and a privacy Allow, are not gated yet.
- [#8](https://github.com/Veso-AI-Open-Source/os-control/issues/8) `see` returns a black screenshot without saying that Screen Recording is missing.
- [#9](https://github.com/Veso-AI-Open-Source/os-control/issues/9) The app list needs a soak test in a long-lived server.

### Upgrade

```
git pull && make install
```

Then restart your Claude Code sessions. A running session keeps the old binary until it restarts.

## [0.1.0] - 2026-10-12: first public release

- `osctl`: an MCP server and command line with 8 tools (`see`, `click`, `type`, `key`, `scroll`, `drag`, `app`, `confirm`) that read the accessibility tree and act on controls in the background.
- `osctl-ask`: the gate panel. It never takes the keyboard, and it accepts only a real click.
- `os-bar`: a small native window that runs Claude Code tasks.
- Four hard stops: money, sending as you, passwords and keys, and destructive changes. A stop switch, and a local audit log.

[Unreleased]: https://github.com/justusewaldo/waldo/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/justusewaldo/waldo/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/justusewaldo/waldo/releases/tag/v0.3.0
[0.2.0]: https://github.com/Veso-AI-Open-Source/os-control/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Veso-AI-Open-Source/os-control/tree/v0.1.0
