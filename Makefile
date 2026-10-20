BIN := $(HOME)/.local/bin

.PHONY: build debug test live install register bar status demo

# Release build of the tool, the bar, and the gate panel.
build:
	cargo build --release --workspace
	swiftc -O crates/waldo-core/swift/ask.swift -o target/release/waldo-ask

debug:
	cargo build --workspace
	swiftc -O crates/waldo-core/swift/ask.swift -o target/debug/waldo-ask

# Unit tests. They touch nothing on screen.
test:
	cargo test --workspace

# Live checks on the real desktop. Calculator only, in the background.
live: debug
	python3 tests/live.py --pointer

# Puts waldo and its panel on the PATH. Sessions already running keep the
# old binary until they restart.
install: build
	install -d $(BIN)
	install -m 755 target/release/waldo target/release/waldo-mcp target/release/waldo-ask target/release/waldo-bar $(BIN)/

# Gives the tools to every Claude Code session of this user.
register:
	claude mcp add --scope user waldo -- $(BIN)/waldo mcp

# Opens the bar. Started from a terminal, it uses the terminal's grants.
bar:
	$(BIN)/waldo-bar &

status:
	$(BIN)/waldo status

# Records docs/media/real-demo.gif. See demo/README.md.
demo:
	uv run demo/record.py
