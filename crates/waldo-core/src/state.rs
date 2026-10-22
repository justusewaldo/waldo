//! Files that outlive one process: the stop switch, the bypass permissions
//! switch, and the audit log.
//!
//! Everything lives in `~/Library/Application Support/Waldo`. Set
//! `WALDO_HOME` to move it, which the tests do.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn dir() -> PathBuf {
    let base = match std::env::var_os("WALDO_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            home.join("Library/Application Support/Waldo")
        }
    };
    if !base.exists() {
        let _ = fs::DirBuilder::new().recursive(true).mode(0o700).create(&base);
    }
    base
}

pub fn tmp_dir() -> PathBuf {
    let d = dir().join("tmp");
    if !d.exists() {
        let _ = fs::DirBuilder::new().recursive(true).mode(0o700).create(&d);
    }
    d
}

fn stop_file() -> PathBuf {
    dir().join("STOP")
}

/// True when the person has hit the stop switch. Every tool checks this
/// before it does anything.
pub fn stopped() -> bool {
    stop_file().exists()
}

pub fn stop_reason() -> String {
    fs::read_to_string(stop_file()).unwrap_or_default().trim().to_string()
}

/// Writes a switch in one rename, so a reader sees all of it or none of it.
fn throw(file: &Path, reason: &str) {
    let tmp = file.with_extension("writing");
    if fs::write(&tmp, format!("{} {}\n", now(), reason)).is_ok() {
        let _ = fs::rename(&tmp, file);
    }
}

pub fn stop(reason: &str) {
    throw(&stop_file(), reason);
    audit(json!({ "event": "stop", "reason": reason }));
}

pub fn resume() -> bool {
    let was = stopped();
    let _ = fs::remove_file(stop_file());
    if was {
        audit(json!({ "event": "resume" }));
    }
    was
}

fn bypass_file() -> PathBuf {
    dir().join("BYPASS")
}

/// True when bypass permissions mode is on, from `WALDO_BYPASS` in the
/// environment or from `waldo bypass on`. Hard stops are logged and allowed
/// without asking.
pub fn bypassed() -> bool {
    bypassed_with(|k| std::env::var(k).ok(), bypass_file().exists())
}

pub fn bypassed_with(env: impl Fn(&str) -> Option<String>, file_exists: bool) -> bool {
    if let Some(v) = env("WALDO_BYPASS") {
        let v = v.trim().to_lowercase();
        if matches!(v.as_str(), "1" | "on" | "true" | "yes") {
            return true;
        }
        if matches!(v.as_str(), "0" | "off" | "false" | "no") {
            return false;
        }
    }
    file_exists
}

pub fn bypass_reason() -> String {
    if let Some(v) = std::env::var("WALDO_BYPASS").ok().filter(|v| !v.trim().is_empty()) {
        let low = v.trim().to_lowercase();
        if matches!(low.as_str(), "1" | "on" | "true" | "yes") {
            return "WALDO_BYPASS".into();
        }
    }
    fs::read_to_string(bypass_file()).unwrap_or_default().trim().to_string()
}

/// Turns bypass permissions mode on or off across sessions. Returns true if
/// the switch changed.
pub fn set_bypass(on: bool, reason: &str) -> bool {
    let was = bypass_file().exists();
    if on {
        throw(&bypass_file(), reason);
        if !was {
            audit(json!({ "event": "bypass", "state": "on", "reason": reason }));
        }
        !was
    } else {
        let _ = fs::remove_file(bypass_file());
        if was {
            audit(json!({ "event": "bypass", "state": "off", "reason": reason }));
        }
        was
    }
}

pub fn audit_path() -> PathBuf {
    dir().join("audit.jsonl")
}

/// Appends one line to the audit log. Never fails the caller.
pub fn audit(mut entry: Value) {
    if let Some(map) = entry.as_object_mut() {
        map.insert("ts".into(), json!(now()));
        map.insert("pid".into(), json!(std::process::id()));
    }
    let opened =
        fs::OpenOptions::new().create(true).append(true).mode(0o600).open(audit_path());
    if let Ok(mut f) = opened {
        let _ = writeln!(f, "{entry}");
    }
}

pub fn unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// UTC time as `2026-10-12T05:52:24Z`.
pub fn now() -> String {
    format_utc(unix_seconds())
}

pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // Civil date from a day count, after Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_resume_and_bypass_switches_work() {
        let home = std::env::temp_dir().join(format!("waldo-core-state-{}", std::process::id()));
        std::env::set_var("WALDO_HOME", &home);
        resume();
        assert!(!stopped());
        stop("a test");
        assert!(stopped());
        assert!(resume());
        assert!(!stopped());
        assert!(!bypassed());
        assert!(set_bypass(true, "a test"));
        assert!(bypassed());
        assert!(!bypassed_with(|_| Some("off".into()), true));
        assert!(bypassed_with(|_| Some("1".into()), false));
        assert!(set_bypass(false, "a test"));
        assert!(!bypassed());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn utc_format_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_791_784_344), "2026-10-12T05:52:24Z");
    }
}
