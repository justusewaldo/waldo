#!/usr/bin/env python3
"""Live check of real key presses and typed text.

Keys go to the app in front, so this takes the focus for about two seconds:
it raises Calculator, sends keys, and gives the focus back. It waits until
the keyboard and mouse have been still for three seconds before it starts.

Run:  python3 tests/foreground.py [path-to-waldo]
"""
import json
import os
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WALDO = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "debug", "waldo")
ENV = dict(os.environ, WALDO_HOME=tempfile.mkdtemp(prefix="waldo-fg-"))
results = []


def check(name, ok, detail=""):
    results.append(ok)
    print(("PASS  " if ok else "FAIL  ") + name + (("  | " + detail) if detail else ""))


def call(tool, **args):
    r = subprocess.run([WALDO, "call", tool, json.dumps(args)], env=ENV, capture_output=True, text=True)
    return r.stdout.strip(), r.returncode != 0


def status(key):
    out = subprocess.run([WALDO, "status"], env=ENV, capture_output=True, text=True).stdout
    return [l.split(":", 1)[1].strip() for l in out.splitlines() if l.startswith(key)][0]


def display():
    text, _ = call("see", app="Calculator", screenshot=False)
    lines = [l for l in text.splitlines() if " text " in l]
    return lines[-1].split('"')[1].replace(",", "") if lines else ""


# FG_IDLE sets how long the keyboard and mouse must be still. Default 3.
NEED = float(os.environ.get("FG_IDLE", "3"))
for _ in range(240):
    if float(status("Idle").split()[0]) >= NEED:
        break
    time.sleep(0.25)
else:
    print("SKIP  the Mac was in use for a full minute. Nothing was sent.")
    sys.exit(2)

before = status("In front")
bundle = before.split("(")[-1].rstrip(")")
call("app", action="open", name="Calculator", background=True)
for _ in range(2):
    call("click", label="Clear", role="button", app="Calculator")

text, err = call("app", action="activate", name="Calculator")
check("bring Calculator to the front", not err, text)
text, err = call("key", keys=["4", "2"], app="Calculator")
check("key presses reach the app in front", not err, text)
text, err = call("type", text="+8=", app="Calculator")
check("typed text reaches the app in front", not err, text)
call("app", action="activate", name=bundle)
shown = display()
check("Calculator shows 50", shown == "50", shown)
check("the focus went back", status("In front") == before, status("In front"))

print(f"\n{sum(results)} of {len(results)} passed")
sys.exit(0 if all(results) else 1)
