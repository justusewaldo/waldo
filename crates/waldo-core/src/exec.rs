//! The tools. One `Executor` lives for one session and holds what the model
//! last saw, so `click 12` means the twelfth line of the last `see`.

use crate::apps::{self, AppInfo, Seen};
use crate::ax::{self, Element, Node, WalkOpts};
use crate::gate::{self, Act, Answer, Gate, Labeled, Policy, Ticket};
use crate::geom::{Rect, View};
use crate::input::{self, Button, Chord};
use crate::screen;
use crate::verify::{self, Outcome, Way};
use crate::state;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct Output {
    pub text: String,
    /// A JPEG the model should look at.
    pub image: Option<Vec<u8>>,
    pub is_error: bool,
}

fn done(text: impl Into<String>) -> Result<Output, String> {
    Ok(Output { text: text.into(), image: None, is_error: false })
}

const STOPPED: &str = "Stopped by the user. Nothing was done. Do not retry and do not look for \
another way. Tell the user you have stopped and wait. They resume with `waldo resume`.";

const DENIED: &str = "Nothing was done. Do not retry it and do not reach the same result another \
way. Tell the user what you wanted to do and why.";

const BROWSERS: &[&str] = &[
    "com.apple.safari", "com.google.chrome", "com.brave.browser", "com.microsoft.edgemac",
    "company.thebrowser.browser", "org.mozilla.firefox", "com.vivaldi.vivaldi",
    "com.operasoftware.opera", "org.chromium.chromium", "app.zen-browser.zen",
    "com.kagi.kagimacos", "com.duckduckgo.macos.browser", "company.thebrowser.dia",
];

fn is_browser(bundle: &str) -> bool {
    let b = bundle.to_lowercase();
    BROWSERS.iter().any(|x| b.starts_with(x))
}

enum Target {
    Node { el: Element, short: String, label: String, actionable: bool, pid: i32 },
    Point { x: f64, y: f64 },
}

pub struct Executor {
    policy: Result<Policy, String>,
    table: Vec<Node>,
    view: View,
    woken: HashSet<i32>,
    /// One-shot approvals from `confirm`, good for the next gated action.
    passes: HashMap<Gate, Instant>,
    /// Timed approvals for one app or place.
    windows: HashMap<(Gate, String), Instant>,
    gate_timeout: Duration,
    max_edge: f64,
    hide_text: bool,
}

fn s<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

fn b(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn n(args: &Value, key: &str) -> Option<f64> {
    let v = args.get(key)?;
    v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok()))
}

/// Web pages squash what is scrolled away into a sliver at the edge.
fn in_view(f: &Rect) -> bool {
    f.w >= 3.0 && f.h >= 3.0
}

fn quote(label: &str) -> String {
    format!("\"{}\"", label.replace('"', "'"))
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Executor {
        let displays = screen::displays();
        let max_edge = std::env::var("WALDO_MAX_EDGE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v.clamp(640.0, 4096.0))
            .unwrap_or(1568.0);
        let gate_secs = std::env::var("WALDO_GATE_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| v.clamp(3, 600))
            .unwrap_or(120);
        let d = &displays[0];
        Executor {
            policy: Policy::load(),
            table: Vec::new(),
            view: View::for_display(d.index, d.bounds, d.native, max_edge),
            woken: HashSet::new(),
            passes: HashMap::new(),
            windows: HashMap::new(),
            gate_timeout: Duration::from_secs(gate_secs),
            max_edge,
            hide_text: false,
        }
    }

    /// Runs one tool and writes one line to the audit log.
    pub fn call(&mut self, tool: &str, args: &Value) -> Output {
        let started = Instant::now();
        self.hide_text = false;
        let result = self.dispatch(tool, args);
        let out = match result {
            Ok(o) => o,
            Err(e) => Output { text: e, image: None, is_error: true },
        };
        let mut logged = args.clone();
        if let Some(text) = logged.get("text").and_then(Value::as_str).map(str::to_string) {
            let count = text.chars().count();
            let shown: String = if self.hide_text { String::new() } else { text.chars().take(80).collect() };
            logged["text"] = json!({ "chars": count, "start": shown });
        }
        state::audit(json!({
            "event": "tool",
            "tool": tool,
            "args": logged,
            "ok": !out.is_error,
            "result": out.text.chars().take(240).collect::<String>(),
            "ms": started.elapsed().as_millis() as u64,
        }));
        out
    }

    fn dispatch(&mut self, tool: &str, args: &Value) -> Result<Output, String> {
        if state::stopped() {
            return Err(STOPPED.into());
        }
        match tool {
            "see" => self.see(args),
            "click" => self.click(args),
            "type" => self.type_text(args),
            "key" => self.key(args),
            "scroll" => self.scroll(args),
            "drag" => self.drag(args),
            "app" => self.app(args),
            "confirm" => self.confirm(args),
            // Not in the tool list. For `waldo call inspect`, when a control
            // does not show up the way you expect.
            "inspect" => self.inspect(args),
            other => Err(format!("there is no tool named \"{other}\"")),
        }
    }

    /// Checks that hold before any click, key press, or drag.
    fn ready(&mut self) -> Result<(), String> {
        if let Err(e) = &self.policy {
            return Err(format!("The gate policy is broken, so nothing will run. {e}"));
        }
        if gate::pending() {
            return Err("A question is on screen for the user. Wait for their answer.".into());
        }
        let (cx, cy) = input::cursor();
        if (0.0..=3.0).contains(&cx) && (0.0..=3.0).contains(&cy) {
            state::stop("the pointer is in the top-left corner");
            return Err(STOPPED.into());
        }
        if !ax::trusted() {
            return Err(ax::error_text(crate::ffi::kAXErrorAPIDisabled));
        }
        Ok(())
    }

    fn app_arg(&self, args: &Value) -> Result<Option<AppInfo>, String> {
        match s(args, "app") {
            None => Ok(None),
            Some(q) => apps::find(q)?.into_iter().next().map(Some).ok_or_else(|| {
                format!("no running app matches \"{q}\". Open it with the app tool first.")
            }),
        }
    }

    fn target_app(&self, args: &Value) -> Result<AppInfo, String> {
        match self.app_arg(args)? {
            Some(a) => Ok(a),
            None => apps::frontmost().ok_or_else(|| "could not tell which app is in front".into()),
        }
    }

    fn app_element(&mut self, app: &AppInfo) -> Element {
        let el = Element::app(app.pid);
        el.set_timeout(1.5);
        if self.woken.insert(app.pid) {
            // Electron honours the first flag. Chromium browsers need the second.
            ax::wake_with(&el, ax::WakeFlag::Manual);
            if is_browser(&app.bundle) {
                ax::wake_with(&el, ax::WakeFlag::Enhanced);
                sleep(Duration::from_millis(250));
            }
        }
        el
    }

    /// The window to read, plus any menu that is open right now.
    fn roots(&mut self, app: &AppInfo, with_menubar: bool) -> (Option<Element>, Vec<Element>) {
        let app_el = self.app_element(app);
        let win = ax::focus_window(&app_el);
        let mut roots: Vec<Element> = win.iter().cloned().collect();
        // A context menu hangs off the app, not off a window.
        for child in app_el.elements("AXChildren") {
            if child.text("AXRole").as_deref() == Some("AXMenu") {
                roots.push(child);
            }
        }
        if let Some(bar) = app_el.element("AXMenuBar") {
            for item in bar.elements("AXChildren") {
                if item.flag("AXSelected") == Some(true) {
                    roots.extend(item.elements("AXChildren"));
                }
            }
            if with_menubar {
                roots.push(bar);
            }
        }
        (win, roots)
    }

    fn walk_app(&mut self, app: &AppInfo, with_menubar: bool) -> (Option<Element>, ax::Walk) {
        let mut found = self.walk_once(app, with_menubar);
        // A browser builds the tree of a page the first time it is asked.
        // The first answer can hold the toolbar and nothing of the page.
        if is_browser(&app.bundle) {
            for _ in 0..3 {
                if found.1.url.is_some() || found.0.is_none() {
                    break;
                }
                sleep(Duration::from_millis(350));
                found = self.walk_once(app, with_menubar);
            }
        }
        found
    }

    fn walk_once(&mut self, app: &AppInfo, with_menubar: bool) -> (Option<Element>, ax::Walk) {
        let (win, roots) = self.roots(app, with_menubar);
        let mut all = ax::Walk {
            nodes: Vec::new(),
            visited: 0,
            truncated: false,
            url: None,
            elapsed: Duration::ZERO,
        };
        let started = Instant::now();
        for (i, root) in roots.iter().enumerate() {
            // Only the window is clipped. Menus may hang outside it.
            let clip = if i == 0 { win.as_ref().and_then(|w| w.frame()) } else { None };
            let left = Duration::from_millis(2500).saturating_sub(started.elapsed());
            let w = ax::walk(root, &WalkOpts { clip, budget: left, ..Default::default() });
            all.visited += w.visited;
            all.truncated |= w.truncated;
            all.url = all.url.or(w.url);
            all.nodes.extend(w.nodes);
        }
        all.elapsed = started.elapsed();
        (win, all)
    }

    /// Window title and web address of an app, for the gate rules.
    fn context(&mut self, app: &AppInfo) -> (String, String) {
        let app_el = self.app_element(app);
        let Some(win) = ax::focus_window(&app_el) else { return (String::new(), String::new()) };
        let title = win.text("AXTitle").unwrap_or_default();
        let mut url = win.text("AXDocument").filter(|d| d.contains("://")).unwrap_or_default();
        if (url.is_empty() || url.starts_with("file:")) && is_browser(&app.bundle) {
            if let Some(u) = ax::find_url(&win, 400) {
                url = u;
            }
        }
        (title, url)
    }

    fn act(&mut self, verb: &str, app: &AppInfo) -> Act {
        let (window, url) = self.context(app);
        Act {
            verb: verb.into(),
            app_name: app.name.clone(),
            app_bundle: app.bundle.clone(),
            window,
            url,
            ..Default::default()
        }
    }

    /// Stops for the person when a hard stop applies. `what` is one plain
    /// sentence that says what is about to happen.
    fn pass_gate(&mut self, act: &Act, what: &str) -> Result<Ticket, String> {
        let policy = self.policy.as_ref().map_err(|e| e.clone())?;
        let passes = &mut self.passes;
        let windows = &mut self.windows;
        let timeout = self.gate_timeout;
        gate::admit(policy, act, |hit| {
            let now = Instant::now();
            if let Some(until) = passes.remove(&hit.gate) {
                if now < until {
                    state::audit(json!({ "event": "gate", "gate": hit.gate.id(), "answer": "confirmed earlier", "summary": what }));
                    return Ok(());
                }
            }
            if let Some(place) = &hit.place {
                if windows.get(&(hit.gate, place.clone())).is_some_and(|until| now < *until) {
                    return Ok(());
                }
            }
            let mut summary = format!("{what}\n\nApp: {}", act.app_name);
            if !act.window.is_empty() {
                summary.push_str(&format!("\nWindow: {}", ax::clean(&act.window, 80)));
            }
            if !act.url.is_empty() {
                summary.push_str(&format!("\nPage: {}", ax::clean(&act.url, 100)));
            }
            summary.push_str(&format!("\nWhy this stops: {}.", hit.why));
            match gate::ask(hit.gate, &summary, hit.place.as_deref(), timeout) {
                Answer::Once => Ok(()),
                Answer::Minutes(m) => {
                    if let Some(place) = hit.place {
                        windows.insert((hit.gate, place), now + Duration::from_secs(m * 60));
                    }
                    Ok(())
                }
                Answer::Deny => Err(format!(
                    "The user did not approve this. It is a hard stop: {}. {DENIED}",
                    hit.gate.title()
                )),
                Answer::Stop => Err(STOPPED.into()),
            }
        })
    }

    /// Brings an app to the front and checks that it got there. Tries the
    /// accessibility API, then AppKit, then LaunchServices.
    fn bring_front(&mut self, t: &Ticket, app: &AppInfo) -> Result<&'static str, String> {
        let ways: Vec<Way> = vec![
            ("accessibility", Duration::from_millis(250), Box::new(|| apps::raise_ax(t, app))),
            ("AppKit", Duration::from_millis(600), Box::new(|| apps::raise_appkit(t, app))),
            ("LaunchServices", Duration::from_millis(1200), Box::new(|| apps::raise_launch_services(t, app))),
        ];
        match verify::ladder(ways, || apps::front_is(app.pid)) {
            Outcome::Done { by, .. } => {
                sleep(Duration::from_millis(60));
                Ok(by)
            }
            Outcome::NoEffect { tried } => Err(format!(
                "could not bring {} to the front. Tried {}; another app kept the focus.",
                app.name,
                tried.join(", ")
            )),
            Outcome::Unobservable { why, .. } => {
                Err(format!("asked {} to come to the front, but could not confirm it: {why}.", app.name))
            }
        }
    }

    /// A fingerprint of what a person would see in the app's front window.
    /// Two equal fingerprints mean nothing visible changed.
    fn digest(&mut self, app: &AppInfo, target: Option<&Element>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let app_el = Element::app(app.pid);
        app_el.elements("AXWindows").len().hash(&mut h);
        if let Some(w) = ax::focus_window(&app_el) {
            w.text("AXTitle").hash(&mut h);
            let opts = WalkOpts { max_visited: 400, max_depth: 25, budget: Duration::from_millis(150), clip: None };
            for n in ax::walk(&w, &opts).nodes {
                (&n.role, &n.label, &n.value, n.enabled, n.selected, n.focused).hash(&mut h);
            }
        }
        if let Some(t) = target {
            t.text("AXValue").hash(&mut h);
        }
        ax::focused_element().map(|f| f.hash()).hash(&mut h);
        h.finish()
    }

    /// Waits up to half a second for the fingerprint to move.
    fn change_note(&mut self, app: &AppInfo, target: Option<&Element>, before: u64) -> &'static str {
        for wait in [90u64, 160, 250] {
            sleep(Duration::from_millis(wait));
            if self.digest(app, target) != before {
                return "Verified: the window changed.";
            }
        }
        "No change was seen within half a second. Call see to check."
    }

    // ---- see ----------------------------------------------------------

    fn see(&mut self, args: &Value) -> Result<Output, String> {
        let want_shot = b(args, "screenshot", true);
        let want_tree = b(args, "elements", true);
        let filter = s(args, "filter").map(str::to_lowercase);
        let max = n(args, "max").map(|v| v as usize).unwrap_or(120).clamp(1, 600);
        let front = apps::frontmost();
        let app = match self.app_arg(args)? {
            Some(a) => a,
            None => front.clone().ok_or("could not tell which app is in front")?,
        };
        let trusted = ax::trusted();
        let (win, walk) = if trusted && want_tree {
            self.walk_app(&app, b(args, "menubar", false))
        } else if trusted {
            let app_el = self.app_element(&app);
            (ax::focus_window(&app_el), ax::Walk { nodes: vec![], visited: 0, truncated: false, url: None, elapsed: Duration::ZERO })
        } else {
            (None, ax::Walk { nodes: vec![], visited: 0, truncated: false, url: None, elapsed: Duration::ZERO })
        };
        let win_frame = win.as_ref().and_then(|w| w.frame());
        let win_title = win.as_ref().and_then(|w| w.text("AXTitle")).unwrap_or_default();

        let displays = screen::displays();
        let disp = match n(args, "display") {
            Some(v) => displays
                .get((v as usize).saturating_sub(1))
                .cloned()
                .ok_or(format!("there is no display {v}. There are {}.", displays.len()))?,
            None => match win_frame {
                Some(f) => screen::display_at(&displays, f.center().0, f.center().1),
                None => displays[0].clone(),
            },
        };
        self.view = View::for_display(disp.index, disp.bounds, disp.native, self.max_edge);
        let view = self.view;

        if let Some(z) = args.get("zoom").and_then(Value::as_array) {
            let v: Vec<f64> = z.iter().filter_map(Value::as_f64).collect();
            if v.len() != 4 || v[2] < 4.0 || v[3] < 4.0 {
                return Err("zoom takes [x, y, width, height] in image pixels".into());
            }
            let (gx, gy) = view.to_screen(v[0], v[1]);
            let rect = Rect { x: gx, y: gy, w: v[2] / view.scale, h: v[3] / view.scale };
            let (jpeg, w, h) =
                screen::capture_region(rect, disp.native, self.max_edge, &state::tmp_dir())?;
            return Ok(Output {
                text: format!(
                    "Close-up of {},{} {}x{}, shown at {w}x{h}. It is for reading only. \
                     Positions for click still come from the full view.",
                    v[0] as i64, v[1] as i64, v[2] as i64, v[3] as i64
                ),
                image: Some(jpeg),
                is_error: false,
            });
        }

        let mut out = String::new();
        if let Some(f) = &front {
            out.push_str(&format!("front app: {} ({})\n", f.name, f.bundle));
            if f.pid != app.pid {
                out.push_str(&format!("reading: {} ({}), which is not in front\n", app.name, app.bundle));
            }
        }
        if !win_title.is_empty() {
            out.push_str(&format!("window: {}\n", quote(&ax::clean(&win_title, 100))));
        }
        if let Some(u) = &walk.url {
            out.push_str(&format!("page: {}\n", ax::clean(u, 160)));
        }
        out.push_str(&format!(
            "display {} of {}: image {}x{}. Every x,y below is a pixel in this image.\n",
            disp.index,
            displays.len(),
            view.img_w,
            view.img_h
        ));

        let at = |f: &Rect| -> String {
            let (cx, cy) = f.center();
            let (ix, iy) = view.to_image(cx, cy);
            format!(
                "@{},{} {}x{}",
                ix.round() as i64,
                iy.round() as i64,
                view.len_to_image(f.w).round() as i64,
                view.len_to_image(f.h).round() as i64
            )
        };

        let wins: Vec<screen::WindowInfo> = screen::windows()
            .into_iter()
            .filter(|w| {
                let big = w.frame.w >= 60.0 && w.frame.h >= 40.0;
                big && (0..25).contains(&w.layer) && w.frame.intersects(&disp.bounds)
            })
            .collect();
        if !wins.is_empty() {
            out.push_str("windows on this display, front to back:\n");
            for w in wins.iter().take(10) {
                let title = if w.title.is_empty() { String::new() } else { format!(" {}", quote(&ax::clean(&w.title, 60))) };
                out.push_str(&format!("  {}{} {}\n", w.app, title, at(&w.frame)));
            }
            if wins.len() > 10 {
                out.push_str(&format!("  and {} more\n", wins.len() - 10));
            }
        }

        if !trusted {
            out.push_str("elements: none. ");
            out.push_str(&ax::error_text(crate::ffi::kAXErrorAPIDisabled));
            out.push('\n');
        } else if want_tree {
            let matches = |node: &Node| match &filter {
                None => true,
                Some(f) => {
                    node.label.to_lowercase().contains(f)
                        || node.short.contains(f.as_str())
                        || node.value.as_deref().is_some_and(|v| v.to_lowercase().contains(f))
                }
            };
            let pool: Vec<usize> = (0..walk.nodes.len()).filter(|&i| matches(&walk.nodes[i])).collect();
            let total = pool.len();
            // When the list is too long, keep what is in view and can be
            // acted on first, then text in view, then what is scrolled away.
            let rank = |i: usize| -> u8 {
                let node = &walk.nodes[i];
                match (in_view(&node.frame), node.actionable) {
                    (true, true) => 0,
                    (true, false) => 1,
                    (false, true) => 2,
                    (false, false) => 3,
                }
            };
            let mut picked: Vec<usize> = Vec::new();
            for r in 0..4u8 {
                let room = max.saturating_sub(picked.len());
                picked.extend(pool.iter().copied().filter(|&i| rank(i) == r).take(room));
            }
            picked.sort_unstable();
            self.table = picked.iter().map(|&i| walk.nodes[i].clone()).collect();

            let mut head = format!("elements in {}: {} shown", app.name, self.table.len());
            if total > self.table.len() {
                head.push_str(&format!(" of {total}. Pass filter or a larger max to see the rest"));
            }
            if walk.truncated {
                head.push_str(". The tree was too large to read in full");
            }
            out.push_str(&head);
            out.push_str(".\n");
            for (i, node) in self.table.iter().enumerate() {
                let mut line = format!("{} {} {}", i + 1, node.short, quote(&node.label));
                if let Some(v) = &node.value {
                    line.push_str(&format!(" ={}", quote(v)));
                }
                line.push(' ');
                if in_view(&node.frame) {
                    line.push_str(&at(&node.frame));
                } else {
                    line.push_str("scrolled out of view");
                }
                if !node.enabled {
                    line.push_str(" disabled");
                }
                if node.focused {
                    line.push_str(" focused");
                }
                if node.selected {
                    line.push_str(" selected");
                }
                out.push_str(&line);
                out.push('\n');
            }
        }

        let mut image = None;
        if want_shot {
            match screen::capture_display(&disp, view.img_w, view.img_h, &state::tmp_dir()) {
                Ok(jpeg) => image = Some(jpeg),
                Err(e) => out.push_str(&format!("screenshot: none. {e}\n")),
            }
        }
        Ok(Output { text: out, image, is_error: false })
    }

    // ---- targets ------------------------------------------------------

    fn has_target(args: &Value) -> bool {
        args.get("id").is_some()
            || s(args, "label").is_some()
            || s(args, "role").is_some()
            || (n(args, "x").is_some() && n(args, "y").is_some())
    }

    fn resolve(&mut self, args: &Value) -> Result<Target, String> {
        if let Some(id) = n(args, "id") {
            let i = id as usize;
            let node = i
                .checked_sub(1)
                .and_then(|i| self.table.get(i))
                .ok_or(format!("there is no element {i} in the last see. Call see again."))?;
            let pid = node.el.pid().ok_or("that element no longer exists. Call see again.")?;
            return Ok(Target::Node {
                el: node.el.clone(),
                short: node.short.to_string(),
                label: node.label.clone(),
                actionable: node.actionable,
                pid,
            });
        }
        if let Some(label) = s(args, "label") {
            return self.find_label(args, label);
        }
        // A role alone is enough when the window has one such thing, such
        // as the one text area of a note that has no label.
        if s(args, "role").is_some() && n(args, "x").is_none() {
            return self.find_label(args, "");
        }
        match (n(args, "x"), n(args, "y")) {
            (Some(ix), Some(iy)) => {
                let (x, y) = self.view.to_screen(ix, iy);
                Ok(Target::Point { x, y })
            }
            _ => Err("say what to act on: id, label, or x and y".into()),
        }
    }

    fn find_label(&mut self, args: &Value, label: &str) -> Result<Target, String> {
        let app = self.target_app(args)?;
        let role = s(args, "role").map(str::to_lowercase);
        let (_, walk) = self.walk_app(&app, true);
        let want = label.to_lowercase();
        let role_ok = |node: &Node| role.as_deref().map(|r| node.short == r).unwrap_or(true);
        let exact: Vec<&Node> = walk
            .nodes
            .iter()
            .filter(|x| role_ok(x) && (want.is_empty() || x.label.to_lowercase() == want))
            .collect();
        let mut found = if exact.is_empty() {
            walk.nodes.iter().filter(|x| role_ok(x) && x.label.to_lowercase().contains(&want)).collect()
        } else {
            exact
        };
        if found.len() > 1 {
            let usable: Vec<&Node> = found.iter().copied().filter(|x| x.actionable && x.enabled).collect();
            if !usable.is_empty() {
                found = usable;
            }
        }
        match found.len() {
            0 if label.is_empty() => Err(format!(
                "{} has no {} in its window. Call see to read what is there.",
                app.name,
                role.as_deref().unwrap_or("such element")
            )),
            0 => Err(format!("nothing in {} is labelled {}. Call see to read what is there.", app.name, quote(label))),
            1 => {
                let node = found[0];
                Ok(Target::Node {
                    el: node.el.clone(),
                    short: node.short.to_string(),
                    label: node.label.clone(),
                    actionable: node.actionable,
                    pid: app.pid,
                })
            }
            many => {
                let view = self.view;
                let mut msg = if label.is_empty() {
                    format!("{} has {many} things with that role. Call see and use an id. They are:", app.name)
                } else {
                    format!("{many} things in {} match {}. Call see and use an id. They are:", app.name, quote(label))
                };
                for node in found.iter().take(8) {
                    let (cx, cy) = node.frame.center();
                    let (ix, iy) = view.to_image(cx, cy);
                    msg.push_str(&format!("\n  {} {} @{},{}", node.short, quote(&node.label), ix.round() as i64, iy.round() as i64));
                }
                Err(msg)
            }
        }
    }

    fn describe_target(short: &str, label: &str) -> String {
        if label.is_empty() {
            format!("an unlabelled {short}")
        } else {
            format!("{short} {}", quote(label))
        }
    }

    // ---- click --------------------------------------------------------

    fn click(&mut self, args: &Value) -> Result<Output, String> {
        self.ready()?;
        let button = match s(args, "button").unwrap_or("left") {
            "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            other => return Err(format!("unknown button \"{other}\"")),
        };
        let count = n(args, "count").map(|v| v as u32).unwrap_or(1).clamp(1, 3);
        let mods: Vec<String> = args
            .get("modifiers")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let flags = input::modifier_flags(&mods)?;
        let force_mouse = b(args, "mouse", false);

        match self.resolve(args)? {
            Target::Node { el, short, label, actionable, pid } => {
                let app = apps::resolve(pid).ok_or("the app of that element is gone. Call see again.")?;
                let mut act = self.act("click", &app);
                let (chain, in_control) = ax::label_chain(&el);
                act.labels = chain.into_iter().map(|l| Labeled { label: l, control: in_control || actionable }).collect();
                if !label.is_empty() {
                    act.labels.push(Labeled { label: label.clone(), control: actionable || in_control });
                }
                let what = Self::describe_target(&short, &label);
                let t = self.pass_gate(&act, &format!("Click {what}."))?;

                let plain = button == Button::Left && count == 1 && flags == 0 && !force_mouse;
                if plain && el.actions().iter().any(|a| a == "AXPress") {
                    let before = self.digest(&app, Some(&el));
                    if el.perform(&t, "AXPress").is_ok() {
                        let note = self.change_note(&app, Some(&el), before);
                        return done(format!("Pressed {what} in {}. {note}", app.name));
                    }
                }
                self.bring_front(&t, &app)?;
                let mut frame = el.frame().ok_or("that element no longer exists. Call see again.")?;
                if !in_view(&frame) {
                    let _ = el.perform(&t, "AXScrollToVisible");
                    sleep(Duration::from_millis(250));
                    frame = el.frame().unwrap_or(frame);
                    if !in_view(&frame) {
                        return Err(format!("{what} is scrolled out of view. Scroll to it, call see, and try again."));
                    }
                }
                let (x, y) = frame.center();
                if let Some(owner) = Element::at_point(x, y).and_then(|h| h.pid()) {
                    if owner != pid {
                        let name = apps::resolve(owner).map(|a| a.name).unwrap_or_else(|| "another app".into());
                        return Err(format!("{what} is covered by a window of {name}. Nothing was clicked."));
                    }
                }
                let before = self.digest(&app, Some(&el));
                input::click(&t, x, y, button, count, flags);
                let note = self.change_note(&app, Some(&el), before);
                done(format!("Clicked {what} in {} with the mouse. {note}", app.name))
            }
            Target::Point { x, y } => {
                if (0.0..=3.0).contains(&x) && (0.0..=3.0).contains(&y) {
                    return Err("that point is the stop corner. Pick another point.".into());
                }
                let hit = Element::at_point(x, y);
                let owner = hit.as_ref().and_then(|h| h.pid()).and_then(apps::resolve);
                let app = match owner {
                    Some(a) => a,
                    None => apps::frontmost().ok_or("could not tell which app is in front")?,
                };
                if let Some(q) = s(args, "app") {
                    if !apps::matches(&app, q) {
                        return Err(format!("that point is over {}, not {q}. Nothing was clicked.", app.name));
                    }
                }
                let mut act = self.act("click", &app);
                let mut what = "a point".to_string();
                if let Some(h) = &hit {
                    let (chain, in_control) = ax::label_chain(h);
                    let d = h.describe();
                    what = Self::describe_target(&d.short, chain.first().map(String::as_str).unwrap_or(""));
                    act.labels = chain.into_iter().map(|l| Labeled { label: l, control: in_control }).collect();
                }
                let t = self.pass_gate(&act, &format!("Click {what}."))?;
                let before = self.digest(&app, hit.as_ref());
                input::click(&t, x, y, button, count, flags);
                let note = self.change_note(&app, hit.as_ref(), before);
                done(format!("Clicked {what} in {} with the mouse. {note}", app.name))
            }
        }
    }

    // ---- type ---------------------------------------------------------

    fn default_button(&mut self, app: &AppInfo) -> Vec<Labeled> {
        let app_el = self.app_element(app);
        ax::focus_window(&app_el)
            .and_then(|w| w.element("AXDefaultButton"))
            .map(|btn| btn.describe().label)
            .filter(|l| !l.is_empty())
            .map(|label| vec![Labeled { label, control: true }])
            .unwrap_or_default()
    }

    fn type_text(&mut self, args: &Value) -> Result<Output, String> {
        self.ready()?;
        let raw = args.get("text").and_then(Value::as_str).ok_or("text is required")?;
        let text = raw.replace("\r\n", "\n");
        let enter = b(args, "enter", false);
        let chars = text.chars().count();

        let target = if Self::has_target(args) { Some(self.resolve(args)?) } else { None };
        if let Some(Target::Point { .. }) = target {
            return Err("type takes id or label. Click the point first, then type.".into());
        }

        if b(args, "set", false) {
            let Some(Target::Node { el, short, label, pid, .. }) = target else {
                return Err("set needs an id or a label".into());
            };
            let app = apps::resolve(pid).ok_or("the app of that element is gone. Call see again.")?;
            let mut act = self.act("type", &app);
            act.focus_role = short.clone();
            self.hide_text = short == "securefield";
            let what = Self::describe_target(&short, &label);
            let t = self.pass_gate(&act, &format!("Set the text of {what} ({chars} characters)."))?;
            el.set_text(&t, "AXValue", &text)?;
            let note = match el.text("AXValue") {
                Some(v) if v == text => "Verified: the field holds the new text.",
                Some(_) => "The field shows different text than was set. Call see to check.",
                None => "Could not read the field back to check.",
            };
            return done(format!("Set {what} in {} to {chars} characters. {note}", app.name));
        }

        // With a target, put the focus on it and check it got there before
        // typing a single key.
        let mut focus_note = String::new();
        if let Some(Target::Node { el, pid, short, label, .. }) = &target {
            let app = apps::resolve(*pid).ok_or("the app of that element is gone. Call see again.")?;
            let what = Self::describe_target(short, label);
            let act = self.act("focus", &app);
            let ft = self.pass_gate(&act, &format!("Put the focus on {what}."))?;
            self.bring_front(&ft, &app)?;
            let ways: Vec<Way> = vec![
                ("accessibility focus", Duration::from_millis(250), Box::new(|| el.set_flag(&ft, "AXFocused", true))),
                (
                    "a mouse click",
                    Duration::from_millis(400),
                    Box::new(|| {
                        let (x, y) = el.frame().ok_or("the element is gone")?.center();
                        input::click(&ft, x, y, Button::Left, 1, 0);
                        Ok(())
                    }),
                ),
            ];
            match verify::ladder(ways, || focus_on(el)) {
                Outcome::Done { by, .. } => focus_note = format!("Focus checked ({by}). "),
                Outcome::NoEffect { tried } => {
                    return Err(format!(
                        "could not put the focus on {what}. Tried {}. Nothing was typed.",
                        tried.join(", ")
                    ))
                }
                Outcome::Unobservable { why, .. } => {
                    return Err(format!("could not confirm the focus is on {what}: {why}. Nothing was typed."))
                }
            }
        }

        // Keys go to whatever app is in front. Check it is the one meant.
        let front = match apps::front() {
            Seen::Yes(a) => a,
            Seen::No => return Err("no app is in front. Nothing was typed.".into()),
            Seen::Unknown(why) => return Err(format!("could not tell which app is in front: {why}. Nothing was typed.")),
        };
        if let Some(Target::Node { pid, .. }) = &target {
            if front.pid != *pid {
                return Err(format!("{} took the focus before typing. Nothing was typed.", front.name));
            }
        } else if let Some(q) = s(args, "app") {
            if !apps::matches(&front, q) {
                return Err(format!("{} is in front, not {q}. Nothing was typed.", front.name));
            }
        }

        let focused = ax::focused_element();
        let focus = focused.as_ref().map(|f| f.describe()).unwrap_or_default();
        self.hide_text = focus.short == "securefield";
        let field = Self::describe_target(if focus.short.is_empty() { "field" } else { &focus.short }, &focus.label);
        let had = focused.as_ref().and_then(|f| f.text("AXValue")).map(|v| v.chars().count());
        let mut act = self.act("type", &front);
        act.focus_role = focus.short.clone();
        let t = self.pass_gate(&act, &format!("Type {chars} characters into {field}."))?;

        let rt = if enter || text.contains('\n') {
            let mut ret = act.clone();
            ret.verb = "key".into();
            ret.chord = Some("return".into());
            ret.labels = self.default_button(&front);
            Some(self.pass_gate(&ret, &format!("Press Return in {field}."))?)
        } else {
            None
        };

        let key = |name: &str| input::parse_chord(name).expect("built-in chord");
        let clear = b(args, "clear", false);
        if clear {
            input::press(&t, &key("cmd+a"));
            sleep(Duration::from_millis(50));
        }

        let mut typed = 0usize;
        let mut buf = String::new();
        let still_front = |pid: i32| apps::front_is(pid) == Seen::Yes(());
        let flush = |buf: &mut String, typed: &mut usize| -> Result<(), String> {
            if buf.is_empty() {
                return Ok(());
            }
            if !still_front(front.pid) {
                return Err(format!("The focus left {} after {} characters. Typing stopped there.", front.name, *typed));
            }
            input::type_text(&t, buf);
            *typed += buf.chars().count();
            buf.clear();
            Ok(())
        };
        for ch in text.chars() {
            match ch {
                '\n' | '\t' => {
                    flush(&mut buf, &mut typed)?;
                    if !still_front(front.pid) {
                        return Err(format!("The focus left {} after {typed} characters. Typing stopped there.", front.name));
                    }
                    let k = key(if ch == '\n' { "return" } else { "tab" });
                    match (&rt, ch) {
                        (Some(r), '\n') => input::press(r, &k),
                        _ => input::press(&t, &k),
                    }
                    typed += 1;
                    sleep(Duration::from_millis(30));
                }
                _ => {
                    buf.push(ch);
                    if buf.chars().count() >= 48 {
                        flush(&mut buf, &mut typed)?;
                    }
                }
            }
        }
        flush(&mut buf, &mut typed)?;
        if let (true, Some(r)) = (enter, &rt) {
            sleep(Duration::from_millis(40));
            if !still_front(front.pid) {
                return Err(format!("The focus left {} before Return. The text was typed. Return was not pressed.", front.name));
            }
            input::press(r, &key("return"));
        }

        // Say what the field held before, and read it back when the text
        // stayed in one field.
        let mut notes = focus_note;
        if let (Some(n), false) = (had, clear) {
            if n > 0 {
                notes.push_str(&format!("The field already held {n} characters; the new text went in with them. "));
            }
        }
        let one_field = !text.contains('\n') && !text.contains('\t');
        if one_field {
            sleep(Duration::from_millis(60));
            let field_el = match &target {
                Some(Target::Node { el, .. }) => Some(el.clone()),
                _ => focused.clone(),
            };
            let shown = field_el.as_ref().and_then(|f| f.text("AXValue"));
            notes.push_str(match shown {
                Some(v) if v.contains(text.trim_end()) => "Verified: the field shows the text.",
                Some(_) if enter => "The field changed after Return; call see to check.",
                Some(_) => "The field does not show the text. Call see to check.",
                None => "Could not read the field back to check.",
            });
        }
        done(format!(
            "Typed {chars} characters into {field} in {}{}. {}",
            front.name,
            if enter { ", then pressed Return" } else { "" },
            notes.trim_end()
        ))
    }

    // ---- key ----------------------------------------------------------

    fn key(&mut self, args: &Value) -> Result<Output, String> {
        self.ready()?;
        let names: Vec<String> = match args.get("keys") {
            Some(Value::String(one)) => vec![one.clone()],
            Some(Value::Array(many)) => many.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => return Err("keys is required: one chord such as \"cmd+c\", or a list".into()),
        };
        if names.is_empty() {
            return Err("keys is empty".into());
        }
        let chords: Vec<Chord> = names.iter().map(|k| input::parse_chord(k)).collect::<Result<_, _>>()?;
        let repeat = n(args, "repeat").map(|v| v as usize).unwrap_or(1).clamp(1, 50);

        let front = apps::frontmost().ok_or("could not tell which app is in front")?;
        if let Some(q) = s(args, "app") {
            if !apps::matches(&front, q) {
                return Err(format!("{} is in front, not {q}. No key was pressed.", front.name));
            }
        }
        let base = self.act("key", &front);
        let mut pressed = 0usize;
        for _ in 0..repeat {
            for chord in &chords {
                let mut act = base.clone();
                act.chord = Some(chord.name.clone());
                act.focus_role = ax::focused_element().map(|f| f.describe().short).unwrap_or_default();
                if chord.name == "return" {
                    act.labels = self.default_button(&front);
                }
                let t = self.pass_gate(&act, &format!("Press {}.", chord.name))?;
                if apps::front_is(front.pid) != Seen::Yes(()) {
                    return Err(format!("The focus left {} after {pressed} key presses. The rest were not sent.", front.name));
                }
                input::press(&t, chord);
                pressed += 1;
                sleep(Duration::from_millis(45));
            }
        }
        let list: Vec<&str> = chords.iter().map(|c| c.name.as_str()).collect();
        done(format!(
            "Pressed {} in {}{}.",
            list.join(", "),
            front.name,
            if repeat > 1 { format!(", {repeat} times") } else { String::new() }
        ))
    }

    // ---- scroll and drag ---------------------------------------------

    fn point_of(&mut self, target: &Target) -> Result<(f64, f64), String> {
        match target {
            Target::Point { x, y } => Ok((*x, *y)),
            Target::Node { el, .. } => el
                .frame()
                .map(|f| f.center())
                .ok_or_else(|| "that element no longer exists. Call see again.".to_string()),
        }
    }

    fn scroll(&mut self, args: &Value) -> Result<Output, String> {
        self.ready()?;
        let dy = n(args, "dy").unwrap_or(0.0) as i32;
        let dx = n(args, "dx").unwrap_or(0.0) as i32;
        if dx == 0 && dy == 0 {
            return Err("give dy or dx in lines. Positive dy shows what is further down.".into());
        }
        let target = if Self::has_target(args) { Some(self.resolve(args)?) } else { None };
        let app = match &target {
            Some(Target::Node { pid, .. }) => apps::resolve(*pid).ok_or("the app of that element is gone. Call see again.")?,
            _ => self.target_app(args)?,
        };
        let (x, y) = match &target {
            Some(t) => self.point_of(t)?,
            None => {
                let app_el = self.app_element(&app);
                ax::focus_window(&app_el)
                    .and_then(|w| w.frame())
                    .map(|f| f.center())
                    .ok_or(format!("{} has no window to scroll", app.name))?
            }
        };
        let act = self.act("scroll", &app);
        let t = self.pass_gate(&act, "Scroll.")?;
        input::move_to(&t, x, y);
        sleep(Duration::from_millis(50));
        input::scroll(&t, dx.clamp(-200, 200), dy.clamp(-200, 200));
        let (ix, iy) = self.view.to_image(x, y);
        done(format!("Scrolled dy {dy}, dx {dx} at {},{}.", ix.round() as i64, iy.round() as i64))
    }

    fn drag(&mut self, args: &Value) -> Result<Output, String> {
        self.ready()?;
        let from_args = args.get("from").ok_or("from is required: an object with id, label, or x and y")?.clone();
        let to_args = args.get("to").ok_or("to is required: an object with id, label, or x and y")?.clone();
        let from = self.resolve(&from_args)?;
        let to = self.resolve(&to_args)?;
        let a = self.point_of(&from)?;
        let z = self.point_of(&to)?;
        let app = match &from {
            Target::Node { pid, .. } => apps::resolve(*pid).ok_or("the app of that element is gone. Call see again.")?,
            Target::Point { x, y } => Element::at_point(*x, *y)
                .and_then(|h| h.pid())
                .and_then(apps::resolve)
                .or_else(apps::frontmost)
                .ok_or("could not tell which app is in front")?,
        };
        let mut act = self.act("drag", &app);
        if let Some(h) = Element::at_point(z.0, z.1) {
            act.drop_labels = ax::label_chain(&h).0;
        }
        let t = self.pass_gate(&act, "Drag an item and drop it.")?;
        if let Target::Node { .. } = from {
            self.bring_front(&t, &app)?;
        }
        input::drag(&t, a, z);
        let (ax1, ay1) = self.view.to_image(a.0, a.1);
        let (ax2, ay2) = self.view.to_image(z.0, z.1);
        done(format!(
            "Dragged from {},{} to {},{} in {}.",
            ax1.round() as i64,
            ay1.round() as i64,
            ax2.round() as i64,
            ay2.round() as i64,
            app.name
        ))
    }

    // ---- app ----------------------------------------------------------

    fn one_app(&self, args: &Value) -> Result<AppInfo, String> {
        let q = s(args, "name").or_else(|| s(args, "bundle")).ok_or("name or bundle is required")?;
        let found = apps::find(q)?;
        match found.len() {
            0 => Err(format!("no running app matches \"{q}\"")),
            1 => Ok(found[0].clone()),
            _ => Err(format!(
                "\"{q}\" matches more than one app: {}. Use the bundle id.",
                found.iter().map(|a| format!("{} ({})", a.name, a.bundle)).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    fn app(&mut self, args: &Value) -> Result<Output, String> {
        let action = s(args, "action").ok_or("action is required: list, open, activate, hide, or quit")?;
        match action {
            "list" => {
                let front = apps::frontmost();
                let mut out = String::from("running apps:\n");
                for a in apps::running() {
                    let mark = if front.as_ref().is_some_and(|f| f.pid == a.pid) { "  in front" } else { "" };
                    out.push_str(&format!("  {} ({}){mark}\n", a.name, a.bundle));
                }
                done(out)
            }
            "open" => {
                let name = s(args, "name").or_else(|| s(args, "bundle"));
                let target = s(args, "target");
                let background = b(args, "background", false);
                apps::open(name, target, background)?;
                if let Some(q) = name {
                    for _ in 0..40 {
                        if let Some(a) = apps::find(q).ok().and_then(|v| v.into_iter().next()) {
                            sleep(Duration::from_millis(300));
                            return done(format!("{} is open{}.", a.name, if background { " in the background" } else { "" }));
                        }
                        sleep(Duration::from_millis(200));
                    }
                    return done(format!("Asked macOS to open {q}. It is not running yet. Call see to check."));
                }
                done(format!("Opened {}.", target.unwrap_or("it")))
            }
            "activate" => {
                let a = self.one_app(args)?;
                let act = self.act("app", &a);
                let t = self.pass_gate(&act, &format!("Bring {} to the front.", a.name))?;
                let by = self.bring_front(&t, &a)?;
                done(format!("{} is in front. Verified ({by}).", a.name))
            }
            "hide" => {
                let a = self.one_app(args)?;
                let act = self.act("app", &a);
                let t = self.pass_gate(&act, &format!("Hide {}.", a.name))?;
                let mut keys = act.clone();
                keys.verb = "key".into();
                keys.chord = Some("cmd+h".into());
                let kt = self.pass_gate(&keys, &format!("Press cmd+h in {}.", a.name))?;
                let cmd_h = input::parse_chord("cmd+h")?;
                let ways: Vec<Way> = vec![
                    ("accessibility", Duration::from_millis(400), Box::new(|| apps::hide_ax(&t, &a))),
                    ("AppKit", Duration::from_millis(500), Box::new(|| apps::hide_appkit(&t, &a))),
                    (
                        "cmd+h",
                        Duration::from_millis(600),
                        Box::new(|| {
                            apps::raise_launch_services(&t, &a)?;
                            for _ in 0..20 {
                                if apps::front_is(a.pid) == Seen::Yes(()) {
                                    input::press(&kt, &cmd_h);
                                    return Ok(());
                                }
                                sleep(Duration::from_millis(50));
                            }
                            Err("it would not come to the front, so cmd+h was not pressed".into())
                        }),
                    ),
                ];
                match verify::ladder(ways, || apps::hidden(a.pid)) {
                    Outcome::Done { by, .. } => done(format!("{} is hidden. Verified ({by}).", a.name)),
                    Outcome::NoEffect { tried } => {
                        Err(format!("could not hide {}. Tried {}; it stayed on screen.", a.name, tried.join(", ")))
                    }
                    Outcome::Unobservable { why, .. } => {
                        Err(format!("asked {} to hide, but could not confirm it: {why}.", a.name))
                    }
                }
            }
            "quit" => {
                let a = self.one_app(args)?;
                if own_ancestors().contains(&a.pid) {
                    return Err(format!("{} runs this session. Quitting it would end the session. Nothing was done.", a.name));
                }
                let act = self.act("app", &a);
                let t = self.pass_gate(&act, &format!("Quit {}.", a.name))?;
                if apps::quit(&t, a.pid) {
                    done(format!("Asked {} to quit. It may ask to save first.", a.name))
                } else {
                    Err(format!("could not quit {}", a.name))
                }
            }
            other => Err(format!("unknown action \"{other}\". Use list, open, activate, hide, or quit.")),
        }
    }

    // ---- inspect ------------------------------------------------------

    /// Dumps everything the accessibility API says about one element: its
    /// attributes, its actions, what encloses it, and what it holds.
    fn inspect(&mut self, args: &Value) -> Result<Output, String> {
        let el = match self.resolve(args)? {
            Target::Node { el, .. } => el,
            Target::Point { x, y } => {
                Element::at_point(x, y).ok_or("there is no element at that point")?
            }
        };
        let show = |v: &crate::cf::Cf| -> String {
            if let Some(t) = v.to_text() {
                return quote(&ax::clean(&t, 120));
            }
            if v.is_array() {
                let items: Vec<String> = (0..v.array_len().min(8))
                    .map(|i| unsafe { crate::cf::string_from(v.array_get(i)) }.unwrap_or_else(|| "…".into()))
                    .collect();
                return format!("[{} items] {}", v.array_len(), items.join(", "));
            }
            "(a value)".into()
        };
        let mut out = String::new();
        let d = el.describe();
        out.push_str(&format!("{} {} (role {}, subrole {})\n", d.short, quote(&d.label), d.role, d.subrole));
        out.push_str(&format!("actions: {}\n", el.actions().join(", ")));
        out.push_str("attributes:\n");
        for name in el.attr_names() {
            match el.copy_attr(&name) {
                Ok(v) => out.push_str(&format!("  {name} = {}\n", show(&v))),
                Err(_) => out.push_str(&format!("  {name}\n")),
            }
        }
        out.push_str("enclosed by:\n");
        let mut cur = el.element("AXParent");
        for _ in 0..6 {
            let Some(p) = cur else { break };
            let d = p.describe();
            let classes = p.strings("AXDOMClassList").join(" ");
            out.push_str(&format!("  {} {} [{}] {}\n", d.role, quote(&d.label), classes, p.actions().join(",")));
            cur = p.element("AXParent");
        }
        let kids = el.elements("AXChildren");
        out.push_str(&format!("holds {} children:\n", kids.len()));
        for k in kids.iter().take(25) {
            let d = k.describe();
            let value = k.text("AXValue").map(|v| ax::clean(&v, 60)).unwrap_or_default();
            let classes = k.strings("AXDOMClassList").join(" ");
            out.push_str(&format!("  {} {} ={} [{}]\n", d.role, quote(&d.label), quote(&value), classes));
        }
        done(out)
    }

    // ---- confirm ------------------------------------------------------

    fn confirm(&mut self, args: &Value) -> Result<Output, String> {
        if let Err(e) = &self.policy {
            return Err(format!("The gate policy is broken, so nothing will run. {e}"));
        }
        let gate = s(args, "gate")
            .and_then(Gate::parse)
            .ok_or("gate is required: money, sending, passwords, or destructive")?;
        let summary = s(args, "summary").ok_or("summary is required: one plain sentence on what you will do")?;
        if gate::pending() {
            return Err("A question is already on screen for the user. Wait for their answer.".into());
        }
        match gate::ask(gate, &ax::clean(summary, 500), None, self.gate_timeout) {
            Answer::Deny => Err(format!("The user said no. {DENIED}")),
            Answer::Stop => Err(STOPPED.into()),
            _ => {
                self.passes.insert(gate, Instant::now() + Duration::from_secs(60));
                done("The user said yes. This covers the next one action of that kind, within 60 seconds.")
            }
        }
    }
}

/// Pids of this process and the chain of processes that started it.
/// Whether the keyboard focus is on `target`, inside it, or on the field
/// it wraps. Unknown when nothing reports the focus.
fn focus_on(target: &Element) -> Seen<()> {
    let Some(f) = ax::focused_element() else {
        return Seen::Unknown("no element reports the keyboard focus".into());
    };
    if f.same(target) {
        return Seen::Yes(());
    }
    let mut up = f.element("AXParent");
    for _ in 0..4 {
        match up {
            Some(e) if e.same(target) => return Seen::Yes(()),
            Some(e) => up = e.element("AXParent"),
            None => break,
        }
    }
    let mut up = target.element("AXParent");
    for _ in 0..2 {
        match up {
            Some(e) if e.same(&f) => return Seen::Yes(()),
            Some(e) => up = e.element("AXParent"),
            None => break,
        }
    }
    Seen::No
}

#[allow(clippy::disallowed_methods)] // reads one parent pid; to replace with proc_pidinfo
fn own_ancestors() -> Vec<i32> {
    let mut out = vec![std::process::id() as i32];
    let mut pid = std::process::id() as i32;
    for _ in 0..12 {
        let ppid = std::process::Command::new("/bin/ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<i32>().ok());
        match ppid {
            Some(p) if p > 1 => {
                out.push(p);
                pid = p;
            }
            _ => break,
        }
    }
    out
}
