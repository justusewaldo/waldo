//! Exhaust: what Waldo learns from his own tool calls as you use him.
//!
//! Zero LLM calls and zero fuzzy guessing. Every verified `click` and `type`
//! leaves deterministic exhaust in `~/Library/Application Support/Waldo/exhaust.json`:
//!
//! 1. **Verified controls** per `(app, domain)`: pinned at rank 0 in `see`, so
//!    controls you actually use never get pushed past the 40-element cut by
//!    sidebar or feed links.
//! 2. **Field physics** per `(app, domain, role, label)`: whether direct
//!    `AXValue` (`set`) works in 28ms, and whether the field holds default or
//!    signature text that needs clearing before typing.
//! 3. **Last regular app**: fallback when `see` is called with no `app` right
//!    after a helper window had focus.

use crate::state;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CONTROLS_PER_SCOPE: usize = 64;

const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "textfield",
    "textarea",
    "combobox",
    "popup",
    "checkbox",
    "radio",
    "tab",
    "menuitem",
    "searchfield",
    "slider",
];

fn is_false(b: &bool) -> bool {
    !*b
}

/// What Waldo has learned about a specific text field.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FieldMem {
    /// `Some(true)` when `AXValue` direct set stuck; `Some(false)` when the
    /// field ignored it and needed keystrokes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_works: Option<bool>,
    /// True when the field holds pre-filled or signature text that should be
    /// cleared before typing new text.
    #[serde(default, skip_serializing_if = "is_false")]
    pub needs_clear: bool,
}

/// Persistent exhaust stored in `exhaust.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Exhaust {
    /// `"<app>|<domain>"` -> `"<role>:<label>"` -> verified hit count.
    #[serde(default)]
    pub controls: BTreeMap<String, BTreeMap<String, u32>>,
    /// `"<app>|<domain>|<role>|<label>"` -> learned field behavior.
    #[serde(default)]
    pub fields: BTreeMap<String, FieldMem>,
    /// Last regular app seen in front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_app: Option<String>,
}

pub fn path() -> PathBuf {
    state::dir().join("exhaust.json")
}

fn off_file() -> PathBuf {
    state::dir().join("NO_EXHAUST")
}

/// True when exhaust learning and ranking are enabled (on by default).
/// Can be overridden with `WALDO_EXHAUST=off` or `waldo exhaust off`.
pub fn enabled() -> bool {
    enabled_with(|k| std::env::var(k).ok(), off_file().exists())
}

pub fn enabled_with(env: impl Fn(&str) -> Option<String>, off_exists: bool) -> bool {
    if let Some(v) = env("WALDO_EXHAUST") {
        let v = v.trim().to_lowercase();
        if matches!(v.as_str(), "1" | "on" | "true" | "yes") {
            return true;
        }
        if matches!(v.as_str(), "0" | "off" | "false" | "no") {
            return false;
        }
    }
    !off_exists
}

/// Turns exhaust mode on or off across sessions. Returns true if changed.
pub fn set_enabled(on: bool, reason: &str) -> bool {
    let was_off = off_file().exists();
    if on {
        let _ = fs::remove_file(off_file());
        if was_off {
            state::audit(json!({ "event": "exhaust", "state": "on", "reason": reason }));
        }
        was_off
    } else {
        let tmp = off_file().with_extension("writing");
        if fs::write(&tmp, format!("{} {}\n", state::now(), reason)).is_ok() {
            let _ = fs::rename(&tmp, off_file());
        }
        if !was_off {
            state::audit(json!({ "event": "exhaust", "state": "off", "reason": reason }));
        }
        !was_off
    }
}

/// Deletes `exhaust.json` so Waldo starts fresh. Returns true if a file was removed.
pub fn clear() -> bool {
    let p = path();
    let existed = p.exists();
    let _ = fs::remove_file(p);
    if existed {
        state::audit(json!({ "event": "exhaust", "state": "cleared" }));
    }
    existed
}

/// Extracts the domain host from a URL, or `"*"` when there is none.
pub fn extract_domain(url: Option<&str>) -> String {
    let Some(u) = url.map(str::trim).filter(|s| !s.is_empty()) else {
        return "*".into();
    };
    let rest = match u.find("://") {
        Some(idx) => &u[idx + 3..],
        None => return "*".into(),
    };
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("").trim().to_lowercase();
    if host.is_empty() {
        "*".into()
    } else {
        host
    }
}

/// True when `(number)` appears in a label, such as `(4) LinkedIn` or `Inbox (503)`.
fn has_badge_count(label: &str) -> bool {
    let bytes = label.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b')' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// True when `(role, label)` is a reusable UI control worth remembering,
/// rather than a password field, a dynamic link, or a notification badge.
pub fn is_stable_control(role: &str, label: &str) -> bool {
    if !INTERACTIVE_ROLES.contains(&role) {
        return false;
    }
    let clean = label.trim();
    let count = clean.chars().count();
    if count == 0 || count > 48 || clean.contains('\n') {
        return false;
    }
    if has_badge_count(clean) {
        return false;
    }
    if clean.starts_with("Invite ") || clean.contains("Starts on ") {
        return false;
    }
    true
}

fn scope_key(app: &str, domain: &str) -> String {
    let dom = if domain.trim().is_empty() { "*" } else { domain.trim() };
    format!("{}|{}", app.trim(), dom)
}

fn control_key(role: &str, label: &str) -> String {
    format!("{}:{}", role.trim(), label.trim())
}

fn field_key(app: &str, domain: &str, role: &str, label: &str) -> String {
    let dom = if domain.trim().is_empty() { "*" } else { domain.trim() };
    format!("{}|{}|{}|{}", app.trim(), dom, role.trim(), label.trim())
}

impl Exhaust {
    /// Loads `exhaust.json` from `state::dir()`, or returns an empty `Exhaust`.
    pub fn load() -> Self {
        Self::load_from(&path())
    }

    pub fn load_from(src: &Path) -> Self {
        fs::read_to_string(src)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes `exhaust.json` atomically via a `.writing` rename.
    pub fn save(&self) {
        self.save_to(&path());
    }

    pub fn save_to(&self, dest: &Path) {
        let tmp = dest.with_extension("writing");
        if let Ok(body) = serde_json::to_string_pretty(self) {
            if fs::write(&tmp, body).is_ok() {
                let _ = fs::rename(&tmp, dest);
            }
        }
    }

    /// Total number of verified controls stored across all apps and domains.
    pub fn total_controls(&self) -> usize {
        self.controls.values().map(BTreeMap::len).sum()
    }

    /// Total number of learned field rules stored across all apps and domains.
    pub fn total_fields(&self) -> usize {
        self.fields.len()
    }

    /// How many times `(role, label)` has succeeded in `(app, domain)`.
    /// Returns 0 when exhaust mode is off.
    pub fn control_hits(&self, app: &str, domain: &str, role: &str, label: &str) -> u32 {
        if !enabled() {
            return 0;
        }
        let ck = control_key(role, label);
        let sk = scope_key(app, domain);
        if let Some(hits) = self.controls.get(&sk).and_then(|m| m.get(&ck)).copied() {
            if hits > 0 {
                return hits;
            }
        }
        if domain != "*" {
            let fallback = scope_key(app, "*");
            if let Some(hits) = self.controls.get(&fallback).and_then(|m| m.get(&ck)).copied() {
                return hits;
            }
        }
        0
    }

    /// Records a verified `click` or `type` on `(role, label)` in `(app, domain)`.
    pub fn record_control(&mut self, app: &str, domain: &str, role: &str, label: &str) {
        if !enabled() || !is_stable_control(role, label) {
            return;
        }
        let sk = scope_key(app, domain);
        let ck = control_key(role, label);
        let map = self.controls.entry(sk).or_default();
        if !map.contains_key(&ck) && map.len() >= MAX_CONTROLS_PER_SCOPE {
            if let Some(min_k) = map.iter().min_by_key(|(_, &v)| v).map(|(k, _)| k.clone()) {
                map.remove(&min_k);
            }
        }
        let entry = map.entry(ck).or_insert(0);
        *entry = entry.saturating_add(1);
        self.save();
    }

    /// Looks up learned field behavior for `(app, domain, role, label)`.
    pub fn field(&self, app: &str, domain: &str, role: &str, label: &str) -> Option<FieldMem> {
        if !enabled() {
            return None;
        }
        let fk = field_key(app, domain, role, label);
        if let Some(f) = self.fields.get(&fk) {
            return Some(f.clone());
        }
        if domain != "*" {
            let fallback = field_key(app, "*", role, label);
            if let Some(f) = self.fields.get(&fallback) {
                return Some(f.clone());
            }
        }
        None
    }

    /// Updates learned field behavior for `(app, domain, role, label)`.
    pub fn record_field(
        &mut self,
        app: &str,
        domain: &str,
        role: &str,
        label: &str,
        set_works: Option<bool>,
        needs_clear: Option<bool>,
    ) {
        if !enabled() || !is_stable_control(role, label) {
            return;
        }
        if set_works.is_none() && needs_clear.is_none() {
            return;
        }
        let fk = field_key(app, domain, role, label);
        let mem = self.fields.entry(fk).or_default();
        if let Some(sw) = set_works {
            mem.set_works = Some(sw);
        }
        if let Some(nc) = needs_clear {
            mem.needs_clear = nc;
        }
        self.save();
    }

    /// Remembers the last regular app so `see {}` can fall back to it when
    /// `waldo-overlay` is momentarily focused.
    pub fn record_app(&mut self, app_name: &str) {
        if !enabled() {
            return;
        }
        let name = app_name.trim();
        if name.is_empty() || name.starts_with("waldo-") {
            return;
        }
        if self.last_app.as_deref() != Some(name) {
            self.last_app = Some(name.to_string());
            self.save();
        }
    }

    pub fn last_app(&self) -> Option<&str> {
        if !enabled() {
            return None;
        }
        self.last_app.as_deref()
    }

    /// Backfills `exhaust.json` from historical tool events in `audit.jsonl`.
    /// Returns the number of verified actions imported.
    pub fn backfill_from_audit(&mut self) -> usize {
        let Ok(text) = fs::read_to_string(state::audit_path()) else {
            return 0;
        };
        let mut imported = 0usize;
        let mut cur_app = String::new();
        let mut cur_dom = String::from("*");

        for line in text.lines() {
            let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(tool) = ev.get("tool").and_then(|v| v.as_str()) else {
                continue;
            };
            let ok = ev.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if !ok {
                continue;
            }
            let args = ev.get("args").cloned().unwrap_or_else(|| json!({}));
            let res = ev.get("result").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(a) = args.get("app").and_then(|v| v.as_str()) {
                if !a.starts_with("waldo-") {
                    cur_app = a.to_string();
                }
            } else if tool == "see" {
                if let Some(rest) = res.strip_prefix("front app: ") {
                    if let Some(name) = rest.split(" (").next() {
                        let name = name.trim();
                        if !name.starts_with("waldo-") {
                            cur_app = name.to_string();
                        }
                    }
                }
            }
            if cur_app.is_empty() {
                continue;
            }
            let combined = format!("{args} {res}");
            if combined.contains("linkedin.com")
                || combined.contains("LinkedIn")
                || combined.contains("Send without a note")
            {
                cur_dom = "www.linkedin.com".into();
            } else if combined.contains("mail.google.com")
                || combined.contains("Veso Mail")
                || combined.contains("Message Body")
                || combined.contains("To recipients")
                || combined.contains("Discard draft")
            {
                cur_dom = "mail.google.com".into();
            } else if combined.contains("calendar.google.com")
                || (cur_app == "Google Chrome"
                    && (combined.contains("Add description") || combined.contains("Guests")))
            {
                cur_dom = "calendar.google.com".into();
            } else if cur_app != "Google Chrome" {
                cur_dom = "*".into();
            }

            if !matches!(tool, "click" | "type")
                || res.contains("No change was seen")
                || res.contains("nothing in ")
            {
                continue;
            }

            let (role, label) = parse_role_label(&args, res);
            if is_stable_control(&role, &label) {
                let sk = scope_key(&cur_app, &cur_dom);
                let ck = control_key(&role, &label);
                *self.controls.entry(sk).or_default().entry(ck).or_insert(0) += 1;
                imported += 1;
            }
            if tool == "type" && is_stable_control(&role, &label) {
                let fk = field_key(&cur_app, &cur_dom, &role, &label);
                if args.get("set").and_then(|v| v.as_bool()) == Some(true) {
                    if res.contains("Verified: the field holds the new text") {
                        self.fields.entry(fk.clone()).or_default().set_works = Some(true);
                    } else if res.contains("did not take the direct value") {
                        self.fields.entry(fk.clone()).or_default().set_works = Some(false);
                    }
                }
                if args.get("clear").and_then(|v| v.as_bool()) == Some(true) {
                    self.fields.entry(fk).or_default().needs_clear = true;
                }
            }
            if tool == "click" && label.contains("Discard draft") {
                let fk = field_key(&cur_app, &cur_dom, "textarea", "Message Body");
                self.fields.entry(fk).or_default().needs_clear = true;
            }
            self.last_app = Some(cur_app.clone());
        }
        if imported > 0 {
            self.save();
        }
        imported
    }
}

fn parse_role_label(args: &serde_json::Value, res: &str) -> (String, String) {
    let r = args.get("role").and_then(|v| v.as_str()).unwrap_or("");
    let l = args.get("label").and_then(|v| v.as_str()).unwrap_or("");
    if !r.is_empty() && !l.is_empty() {
        return (r.to_string(), l.to_string());
    }
    for marker in ["Pressed ", "Clicked ", "into ", "Set "] {
        if let Some(idx) = res.find(marker) {
            let after = &res[idx + marker.len()..];
            if let Some(q1) = after.find(" \"") {
                let role = after[..q1].trim();
                let rest = &after[q1 + 2..];
                if let Some(q2) = rest.find('"') {
                    let label = &rest[..q2];
                    return (role.to_string(), label.to_string());
                }
            }
        }
    }
    (r.to_string(), l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_extraction_handles_urls_and_empty() {
        assert_eq!(
            extract_domain(Some("https://mail.google.com/mail/u/0/#inbox")),
            "mail.google.com"
        );
        assert_eq!(
            extract_domain(Some("http://localhost:3000/invoices?id=1")),
            "localhost"
        );
        assert_eq!(extract_domain(None), "*");
        assert_eq!(extract_domain(Some("")), "*");
    }

    #[test]
    fn stable_control_filters_dynamic_clutter() {
        assert!(is_stable_control("button", "Compose"));
        assert!(is_stable_control("textarea", "Message Body"));
        assert!(is_stable_control("button", "Discard draft (⌘⇧D)"));
        assert!(!is_stable_control("webarea", "(4) LinkedIn"));
        assert!(!is_stable_control("tab", "Inbox (503) - Veso Mail"));
        assert!(!is_stable_control("securefield", "Password"));
        assert!(!is_stable_control("menuitem", "Invite Richard Lyons to connect"));
    }

    #[test]
    fn exhaust_records_and_persists_controls_and_fields() {
        let file = std::env::temp_dir().join(format!("waldo-exhaust-{}.json", std::process::id()));
        let _ = fs::remove_file(&file);

        let mut ex = Exhaust::load_from(&file);
        assert_eq!(ex.total_controls(), 0);

        let sk = scope_key("Google Chrome", "mail.google.com");
        let ck = control_key("button", "Compose");
        *ex.controls.entry(sk).or_default().entry(ck).or_insert(0) += 2;
        ex.fields.insert(
            field_key("Google Chrome", "mail.google.com", "textarea", "Message Body"),
            FieldMem { set_works: Some(false), needs_clear: true },
        );
        ex.last_app = Some("Google Chrome".into());
        ex.save_to(&file);

        let reloaded = Exhaust::load_from(&file);
        assert_eq!(
            reloaded.control_hits("Google Chrome", "mail.google.com", "button", "Compose"),
            2
        );
        let fm = reloaded
            .field("Google Chrome", "mail.google.com", "textarea", "Message Body")
            .expect("field memory saved");
        assert_eq!(fm.set_works, Some(false));
        assert!(fm.needs_clear);
        assert_eq!(reloaded.last_app(), Some("Google Chrome"));

        assert!(!enabled_with(|_| Some("off".into()), false));
        assert!(enabled_with(|_| Some("on".into()), true));
        assert!(!enabled_with(|_| None, true));
        assert!(enabled_with(|_| None, false));

        let _ = fs::remove_file(&file);
    }
}
