#!/usr/bin/env python3
"""Live check of the MCP server against the real desktop.

It drives Calculator in the background by pressing its buttons through the
accessibility tree. It does not move the pointer and does not type, so it
is safe to run while the Mac is in use. One step shows a gate panel for a
few seconds. Leave it alone and it times out, which counts as no.

Run:  python3 tests/live.py [--pointer] [path-to-waldo]

--pointer adds one check that moves the real pointer: it proves the gate
panel refuses a press by the agent itself.
"""
import json
import os
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--pointer"]
POINTER = "--pointer" in sys.argv
WALDO = ARGS[0] if ARGS else os.path.join(ROOT, "target", "debug", "waldo")
ASK = os.path.join(os.path.dirname(WALDO), "waldo-ask")
HOME = tempfile.mkdtemp(prefix="waldo-live-")
ENV = dict(os.environ, WALDO_HOME=HOME, WALDO_GATE_TIMEOUT="4")

results = []


def check(name, ok, detail=""):
    results.append(ok)
    print(("PASS  " if ok else "FAIL  ") + name + (("  | " + detail) if detail else ""))


class Server:
    def __init__(self):
        self.p = subprocess.Popen([WALDO, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=ENV, text=True)
        self.n = 0
        self.calls = 0

    def send(self, method, params=None, notify=False):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notify:
            self.n += 1
            msg["id"] = self.n
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()
        if notify:
            return None
        return json.loads(self.p.stdout.readline())

    def tool(self, tool_name, /, **args):
        self.calls += 1
        r = self.send("tools/call", {"name": tool_name, "arguments": args})["result"]
        text = "".join(c["text"] for c in r["content"] if c["type"] == "text")
        images = [c for c in r["content"] if c["type"] == "image"]
        return text, r["isError"], images

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=5)


def waldo(*args):
    return subprocess.run([WALDO, *args], env=ENV, capture_output=True, text=True).stdout


def display_of(text):
    """The last text line before the buttons is the result line."""
    lines = [l for l in text.splitlines() if " text " in l]
    return lines[-1].split('"')[1].replace(",", "") if lines else ""


was_running = subprocess.run(["pgrep", "-x", "Calculator"], capture_output=True).returncode == 0
s = Server()

r = s.send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "live", "version": "0"}})
check("initialize", r["result"]["serverInfo"]["name"] == "waldo" and "Hard stops" in r["result"]["instructions"])
s.send("notifications/initialized", notify=True)
tools = s.send("tools/list")["result"]["tools"]
check("tools/list", [t["name"] for t in tools] == ["see", "click", "type", "key", "scroll", "drag", "app", "confirm"])

text, err, _ = s.tool("app", action="open", name="Calculator", background=True)
check("open Calculator in the background", not err, text.strip())
front_before = waldo("status")
front_before = [l for l in front_before.splitlines() if l.startswith("In front")][0]

text, err, images = s.tool("see", app="Calculator")
check("see lists the buttons", not err and 'button "Equals"' in text and 'button "7"' in text)
check("see returns one JPEG", len(images) == 1 and images[0]["mimeType"] == "image/jpeg" and len(images[0]["data"]) > 10000)

# The clear key reads "Clear" with an entry and "All Clear" without one.
# Two presses of whatever matches "Clear" always end in a clean state.
for _ in range(2):
    s.tool("click", label="Clear", role="button", app="Calculator")

# Press by label: 17 x 23 = 391.
ok = True
for label in ["1", "7", "Multiply", "2", "3", "Equals"]:
    t, e, _ = s.tool("click", label=label, role="button", app="Calculator")
    ok = ok and not e and t.startswith("Pressed")
check("click by label presses in the background", ok)
text, err, _ = s.tool("see", app="Calculator", screenshot=False)
check("Calculator shows 391", display_of(text) == "391", display_of(text))

# Press by id from the last see: 391 + 9 = 400.
ids = {}
for line in text.splitlines():
    parts = line.split(" ", 2)
    if len(parts) == 3 and parts[0].isdigit() and parts[1] == "button":
        ids[parts[2].split('"')[1]] = int(parts[0])
ok = True
for label in ["Add", "9", "Equals"]:
    t, e, _ = s.tool("click", id=ids[label])
    ok = ok and not e
check("click by id", ok)
text, _, _ = s.tool("see", app="Calculator", screenshot=False)
check("Calculator shows 400", display_of(text) == "400", display_of(text))

front_after = [l for l in waldo("status").splitlines() if l.startswith("In front")][0]
check("the app in front did not change", front_before == front_after, front_after)

# Filter.
text, _, _ = s.tool("see", app="Calculator", screenshot=False, filter="equals")
check("filter narrows the list", 'button "Equals"' in text and 'button "7"' not in text)

# Guards: nothing is sent when the wrong app is in front.
text, err, _ = s.tool("key", keys="1", app="Calculator")
check("key refuses when another app is in front", err and "No key was pressed" in text, text.strip())
text, err, _ = s.tool("type", text="1", app="Calculator")
check("type refuses when another app is in front", err and "Nothing was typed" in text, text.strip())

# Bad input.
text, err, _ = s.tool("click", id=9999)
check("a stale id is an error", err and "Call see again" in text)
text, err, _ = s.tool("key", keys="hyper+k")
check("an unknown key is an error", err and "unknown modifier" in text)
text, err, _ = s.tool("click", label="No Such Button", app="Calculator")
check("a missing label is an error", err and "nothing in Calculator is labelled" in text, text.strip())

# Hard stop: Calculator's backspace is labelled Delete, which the
# destructive gate catches. The dialog times out, which counts as no.
print("      a gate panel now shows for 4 seconds. Leave it alone.")
t0 = time.time()
text, err, _ = s.tool("click", label="Delete", role="button", app="Calculator")
waited = time.time() - t0
if err:
    check("a gated click waits for the user and is refused on no answer", "did not approve" in text and waited >= 3, f"{waited:.1f}s")
    after, _, _ = s.tool("see", app="Calculator", screenshot=False)
    check("the refused click changed nothing", display_of(after) == "400", display_of(after))
else:
    check("a gated click waited for the user, who clicked Allow", waited >= 0.3, f"{waited:.1f}s")

# The panel must refuse the agent's own press, by either route.
if POINTER and os.path.exists(ASK):
    out = os.path.join(HOME, "probe.txt")
    body = "Self-test. It stays open for 8 seconds whatever is clicked."
    probe = subprocess.Popen([ASK, "--probe", "--timeout", "8", "--title", "Probe", "--body", body],
                             stdout=open(out, "w"), stderr=subprocess.DEVNULL)
    time.sleep(1.5)
    text, _, _ = s.tool("see", app="waldo-ask", screenshot=False, filter="Probe")
    line = [l for l in text.splitlines() if 'button "Probe"' in l]
    if line:
        x, y = line[0].split("@")[1].split(" ")[0].split(",")
        s.tool("click", label="Probe", role="button", app="waldo-ask")
        time.sleep(0.4)
        s.tool("click", x=int(x), y=int(y))
        time.sleep(0.4)
    probe.wait()
    seen = open(out).read().split()
    check("the panel refuses a press through accessibility", "none" in seen, " ".join(seen))
    check("the panel refuses a posted click", "posted" in seen, " ".join(seen))
    check("the panel took no agent press for a real one", seen.count("real") == 0, " ".join(seen))

# Stop switch.
waldo("stop", "live test")
text, err, _ = s.tool("see", app="Calculator", screenshot=False)
check("stop refuses every tool", err and "Stopped by the user" in text)
text, err, _ = s.tool("click", label="1", app="Calculator")
check("stop refuses clicks", err and "Stopped by the user" in text)
waldo("resume")
text, err, _ = s.tool("see", app="Calculator", screenshot=False)
check("resume clears it", not err)

# Audit log.
log = open(os.path.join(HOME, "audit.jsonl")).read().splitlines()
events = [json.loads(l) for l in log]
logged = sum(1 for e in events if e["event"] == "tool")
check("every call is in the audit log", logged == s.calls, f"{logged} of {s.calls}")
check("the gate answer is in the audit log", any(e["event"] == "gate" for e in events))

if not was_running:
    s.tool("click", label="Clear", role="button", app="Calculator")
    text, err, _ = s.tool("app", action="quit", name="Calculator")
    check("quit Calculator", not err, text.strip())
s.close()

print(f"\n{sum(results)} of {len(results)} passed")
sys.exit(0 if all(results) else 1)
