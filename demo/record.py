# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow"]
# ///
"""Records the demo at the top of the README.

Three windows are captured and nothing else: the bar, the invoice page in
Safari, and the gate panel. Each is captured by its own window id, so the
rest of the screen never enters a frame.

Run:  uv run demo/record.py
      uv run demo/record.py WORK_FOLDER    builds the GIF again from an earlier run
"""
import hashlib
import http.server
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
BIN = Path.home() / ".local/bin"
WALDO = str(BIN / "waldo")
PORT = 8765
TASKS = [
    "Read the invoice that is open in Safari. Who is it from, and how much is due? Leave Safari in the background.",
    "Pay it.",
]
GATE_SECONDS = 7
TICK = 0.3
LIMIT = 150  # seconds

work = Path(tempfile.mkdtemp(prefix="waldo-demo-"))


class Quiet(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / "demo"), **kwargs)

    def log_message(self, *args):
        pass


def serve():
    http.server.ThreadingHTTPServer.allow_reuse_address = True
    server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Quiet)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def windows():
    out = subprocess.run([WALDO, "windows"], capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        parts = line.split("\t")
        if len(parts) >= 4:
            rows.append((int(parts[0]), parts[1], parts[2], parts[3]))
    return rows


def find(app, title_part=""):
    for wid, name, title, _ in windows():
        if name == app and title_part in title:
            return wid
    return None


def grab(wid, path):
    subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l", str(wid), "-t", "png", str(path)],
                   capture_output=True)
    return path.exists()


def main():
    server = serve()
    front = subprocess.run([WALDO, "status"], capture_output=True, text=True).stdout
    print([l for l in front.splitlines() if l.startswith("In front")][0])

    subprocess.run(["/usr/bin/open", "-g", "-a", "Safari", f"http://127.0.0.1:{PORT}/invoice.html"])
    for _ in range(40):
        time.sleep(0.5)
        page = find("Safari", "Invoice")
        if page:
            break
    else:
        sys.exit("Safari did not open the invoice")
    # A fixed, small window keeps the page readable in the recording.
    subprocess.run(["/usr/bin/osascript", "-e",
                    'tell application "System Events" to tell process "Safari" to set size of window 1 to {560, 560}'],
                   capture_output=True)
    time.sleep(1.0)
    # Safari builds the page tree on first request. Ask once now, so the
    # recording does not spend a turn on it.
    subprocess.run([WALDO, "call", "see", '{"app":"Safari","screenshot":false}'], capture_output=True)

    env = dict(os.environ, WALDO_BAR_QUIET="1", WALDO_BAR_AUTOSEND="||".join(TASKS),
               WALDO_BAR_MODEL=os.environ.get("WALDO_BAR_MODEL", "sonnet"), WALDO_GATE_TIMEOUT=str(GATE_SECONDS))
    bar_proc = subprocess.Popen([str(BIN / "waldo-bar")], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(40):
        time.sleep(0.25)
        bar = find("waldo-bar", "Waldo")
        if bar:
            break
    else:
        sys.exit("the bar did not open")

    ticks = []
    start = time.time()
    seen_panel = False
    quiet_since = None
    last_sig = None
    n = 0
    while time.time() - start < LIMIT:
        t0 = time.time()
        panel = find("waldo-ask")
        seen_panel = seen_panel or bool(panel)
        shot = {"bar": work / f"bar-{n:04d}.png", "page": work / f"page-{n:04d}.png"}
        grab(bar, shot["bar"])
        grab(page, shot["page"])
        if panel:
            shot["panel"] = work / f"panel-{n:04d}.png"
            if not grab(panel, shot["panel"]):
                del shot["panel"]
        sig = hashlib.md5(b"".join(p.read_bytes() for p in shot.values() if p.exists())).hexdigest()
        ticks.append((shot, sig))
        n += 1
        # Stop once both tasks have run and nothing has changed for a while.
        if sig != last_sig:
            quiet_since = time.time()
            last_sig = sig
        if seen_panel and not panel and time.time() - quiet_since > 9:
            break
        time.sleep(max(0.0, TICK - (time.time() - t0)))

    bar_proc.terminate()
    subprocess.run([WALDO, "call", "app", '{"action":"quit","name":"Safari"}'], capture_output=True)
    server.shutdown()
    print(f"{len(ticks)} ticks in {time.time() - start:.0f} s, panel seen: {seen_panel}")
    compose(ticks)


def place(canvas, path, size, at):
    image = Image.open(path).convert("RGBA").resize(size, Image.LANCZOS)
    canvas.paste(image, at, image)


CAPTIONS = [
    ("It reads an invoice in a window behind your work.", "Your pointer does not move. Your typing is not interrupted."),
    ("Then it is told to pay. Money is a hard stop.", "The panel takes no keys. Only your own click can allow it."),
    ("No click, no payment.", "A click posted by software is refused. So is the agent's own press."),
]


def compose(ticks):
    small = ImageFont.truetype("/System/Library/Fonts/SFNS.ttf", 13)
    big = ImageFont.truetype("/System/Library/Fonts/SFNS.ttf", 21)
    mid = ImageFont.truetype("/System/Library/Fonts/SFNS.ttf", 15)
    pad, gap, label_h = 22, 18, 26
    bar0 = Image.open(ticks[0][0]["bar"])
    page0 = Image.open(ticks[0][0]["page"])
    bar_w, bar_h = bar0.width // 2, bar0.height // 2
    page_w, page_h = page0.width // 2, page0.height // 2
    width = pad + bar_w + gap + page_w + pad
    height = pad + label_h + max(bar_h, page_h) + pad
    top = pad + label_h

    # The caption follows the panel: before it, while it shows, and after.
    phase, seen = [], False
    for shot, _ in ticks:
        if "panel" in shot:
            seen = True
            phase.append(1)
        else:
            phase.append(2 if seen else 0)

    # Collapse runs of identical frames, and cap how long any one holds.
    runs = []
    for (shot, sig), ph in zip(ticks, phase):
        if runs and runs[-1][1] == (sig, ph):
            runs[-1][2] += 1
        else:
            runs.append([shot, (sig, ph), 1])
    frames = work / "frames"
    frames.mkdir()
    lines = []
    for i, (shot, (_, ph), count) in enumerate(runs):
        canvas = Image.new("RGB", (width, height), (13, 13, 16))
        draw = ImageDraw.Draw(canvas)
        draw.text((pad, pad), "The bar. You type a task. Claude Code does it.", font=small, fill=(150, 150, 158))
        draw.text((pad + bar_w + gap, pad), "Safari, behind your other windows", font=small, fill=(150, 150, 158))
        place(canvas, shot["bar"], (bar_w, bar_h), (pad, top))
        place(canvas, shot["page"], (page_w, page_h), (pad + bar_w + gap, top))
        head, sub = CAPTIONS[ph]
        cy = top + bar_h + 26
        draw.text((pad, cy), head, font=big, fill=(255, 159, 10) if ph == 1 else (235, 235, 240))
        draw.text((pad, cy + 34), sub, font=mid, fill=(150, 150, 158))
        if "panel" in shot and shot["panel"].exists():
            raw = Image.open(shot["panel"])
            size = (int(raw.width * 0.44), int(raw.height * 0.44))
            x = pad + bar_w + gap + (page_w - size[0]) // 2
            y = top + page_h - size[1] - 12
            draw.rounded_rectangle((x - 3, y - 3, x + size[0] + 2, y + size[1] + 2), radius=14, fill=(255, 159, 10))
            place(canvas, shot["panel"], size, (x, y))
        path = frames / f"f{i:04d}.png"
        canvas.save(path)
        hold = min(count * TICK, 1.5)
        if ph == 1:
            hold = min(count * TICK, 2.4)
        if i == len(runs) - 1:
            hold = 5.0
        lines.append(f"file '{path}'\nduration {hold:.2f}")
    lines.append(f"file '{frames / f'f{len(runs) - 1:04d}.png'}'")
    listing = work / "frames.txt"
    listing.write_text("\n".join(lines) + "\n")

    out = ROOT / "docs" / "media" / "real-demo.gif"
    out.parent.mkdir(parents=True, exist_ok=True)
    palette = work / "palette.png"
    base = ["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing)]
    subprocess.run(base + ["-vf", "palettegen=max_colors=160:stats_mode=diff", str(palette)], check=True)
    subprocess.run(base + ["-i", str(palette), "-lavfi", "fps=10 [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5",
                           str(out)], check=True)
    # One still of the hard stop, for places that do not play a GIF.
    stop = next((i for i, r in enumerate(runs) if r[1][1] == 1), 0)
    shutil.copy(frames / f"f{stop:04d}.png", out.parent / "real-hard-stop.png")
    seconds = sum(float(l.split("duration ")[1]) for l in lines if "duration" in l)
    print(f"{len(runs)} frames, {seconds:.0f} s, {out.stat().st_size / 1e6:.1f} MB, {width}x{height}: {out}")
    print("work folder:", work)


def recompose(folder):
    """Builds the GIF again from the captures of an earlier run."""
    ticks = []
    for bar in sorted(folder.glob("bar-*.png")):
        n = bar.stem.split("-")[1]
        shot = {"bar": bar, "page": folder / f"page-{n}.png"}
        if (folder / f"panel-{n}.png").exists():
            shot["panel"] = folder / f"panel-{n}.png"
        sig = hashlib.md5(b"".join(p.read_bytes() for p in shot.values() if p.exists())).hexdigest()
        ticks.append((shot, sig))
    compose(ticks)


if __name__ == "__main__":
    if len(sys.argv) > 1:
        recompose(Path(sys.argv[1]))
    else:
        main()
