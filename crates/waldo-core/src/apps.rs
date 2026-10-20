//! Running apps: which one is in front, and how to open, raise, and hide them.
//!
//! Identity comes from one place. An [`AppInfo`] can only be made in this
//! module, and only from a live `NSRunningApplication`. Nothing here reads
//! the text output of another program.

use crate::ax::Element;
use crate::ffi::*;
use crate::gate::Ticket;
use std::ffi::{c_void, CStr};
use std::os::raw::{c_char, c_int};
use std::process::Command;

/// A running app, as macOS reports it right now.
///
/// The private field means no other module can build one, so every app a
/// tool acts on came from [`resolve`]:
///
/// ```compile_fail
/// let a = waldo_core::apps::AppInfo { pid: 1, name: "x".into(), bundle: "y".into(), regular: true };
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct AppInfo {
    pub pid: i32,
    pub name: String,
    pub bundle: String,
    regular: bool,
}

impl AppInfo {
    /// True for apps that show in the Dock. False for agents and dialogs.
    pub fn regular(&self) -> bool {
        self.regular
    }
}

/// What an observation found. `Unknown` is never the same as `No`.
#[derive(Clone, Debug, PartialEq)]
pub enum Seen<T> {
    Yes(T),
    No,
    Unknown(String),
}

type Id = *mut c_void;

extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
}

unsafe fn sel(name: &CStr) -> Id {
    sel_registerName(name.as_ptr())
}

unsafe fn msg_ptr() -> *const () {
    objc_msgSend as *const ()
}

unsafe fn send_id(obj: Id, s: Id) -> Id {
    let f: extern "C" fn(Id, Id) -> Id = std::mem::transmute(msg_ptr());
    f(obj, s)
}

unsafe fn send_bool(obj: Id, s: Id) -> bool {
    let f: extern "C" fn(Id, Id) -> bool = std::mem::transmute(msg_ptr());
    f(obj, s)
}

unsafe fn send_i64(obj: Id, s: Id) -> i64 {
    let f: extern "C" fn(Id, Id) -> i64 = std::mem::transmute(msg_ptr());
    f(obj, s)
}

unsafe fn send_u64_bool(obj: Id, s: Id, arg: u64) -> bool {
    let f: extern "C" fn(Id, Id, u64) -> bool = std::mem::transmute(msg_ptr());
    f(obj, s, arg)
}

unsafe fn running_app(pid: i32) -> Id {
    let cls = objc_getClass(c"NSRunningApplication".as_ptr());
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    let f: extern "C" fn(Id, Id, i32) -> Id = std::mem::transmute(msg_ptr());
    f(cls, sel(c"runningApplicationWithProcessIdentifier:"), pid)
}

unsafe fn ns_string(obj: Id) -> String {
    if obj.is_null() {
        return String::new();
    }
    let f: extern "C" fn(Id, Id) -> *const c_char = std::mem::transmute(msg_ptr());
    let p = f(obj, sel(c"UTF8String"));
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// Runs `f` with the app object for a pid inside an autorelease pool.
fn with_app<T>(pid: i32, f: impl FnOnce(Id) -> T) -> Option<T> {
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let app = running_app(pid);
        let out = if app.is_null() { None } else { Some(f(app)) };
        objc_autoreleasePoolPop(pool);
        out
    }
}

/// Every process id on the system, from the kernel.
fn all_pids() -> Vec<i32> {
    unsafe {
        let n = proc_listallpids(std::ptr::null_mut(), 0);
        if n <= 0 {
            return Vec::new();
        }
        let mut buf = vec![0i32; n as usize + 64];
        let got = proc_listallpids(buf.as_mut_ptr() as *mut c_void, (buf.len() * 4) as c_int);
        if got <= 0 {
            return Vec::new();
        }
        buf.truncate(got as usize);
        buf.retain(|p| *p > 0);
        buf
    }
}

/// The app with this pid, or None when the pid is not an app.
pub fn resolve(pid: i32) -> Option<AppInfo> {
    with_app(pid, |app| unsafe {
        AppInfo {
            pid,
            name: ns_string(send_id(app, sel(c"localizedName"))),
            bundle: ns_string(send_id(app, sel(c"bundleIdentifier"))),
            regular: send_i64(app, sel(c"activationPolicy")) == 0,
        }
    })
    .filter(|a| !a.name.is_empty() || !a.bundle.is_empty())
}

/// Every app on the system, Dock apps and agents alike, asked fresh. Two
/// witnesses: the kernel's process list and the window server's window
/// owners. An error means neither could be read, which is not the same as
/// "no such app".
pub fn all_checked() -> Result<Vec<AppInfo>, String> {
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let mut pids = all_pids();
        let from_kernel = pids.len();
        let windows = crate::screen::windows();
        let mut seen: std::collections::HashSet<i32> = pids.iter().copied().collect();
        for w in &windows {
            if w.pid > 0 && seen.insert(w.pid) {
                pids.push(w.pid);
            }
        }
        let apps: Vec<AppInfo> = pids.into_iter().filter_map(resolve).collect();
        if !apps.is_empty() {
            return Ok(apps);
        }
        last = format!(
            "the kernel listed {from_kernel} processes and the window server {} windows, but none resolved to an app",
            windows.len()
        );
    }
    Err(format!("could not list the running apps: {last}"))
}

/// Every app on the system, or none when they cannot be listed.
pub fn all() -> Vec<AppInfo> {
    all_checked().unwrap_or_default()
}

/// The apps that show in the Dock.
pub fn running() -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = all().into_iter().filter(|a| a.regular).collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// Picks the running apps a query means: bundle id, exact name, then part of
/// a name. The best kind of match wins and the rest are dropped.
pub fn pick(list: &[AppInfo], query: &str) -> Vec<AppInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let by = |f: &dyn Fn(&AppInfo) -> bool| -> Vec<AppInfo> {
        list.iter().filter(|a| f(a)).cloned().collect()
    };
    let exact_bundle = by(&|a| a.bundle.to_lowercase() == q);
    if !exact_bundle.is_empty() {
        return exact_bundle;
    }
    let exact_name = by(&|a| a.name.to_lowercase() == q);
    if !exact_name.is_empty() {
        return exact_name;
    }
    by(&|a| a.name.to_lowercase().contains(&q))
}

/// Running apps a query names. Dock apps win. A system dialog or an agent
/// is found only by its exact name or bundle id. An error means the apps
/// could not be listed at all.
pub fn find(query: &str) -> Result<Vec<AppInfo>, String> {
    let everything = all_checked()?;
    let dock: Vec<AppInfo> = everything.iter().filter(|a| a.regular).cloned().collect();
    let found = pick(&dock, query);
    if !found.is_empty() {
        return Ok(found);
    }
    let q = query.trim().to_lowercase();
    Ok(everything
        .into_iter()
        .filter(|a| !a.regular && (a.name.to_lowercase() == q || a.bundle.to_lowercase() == q))
        .collect())
}

/// True when `app` is what `query` names.
pub fn matches(app: &AppInfo, query: &str) -> bool {
    !pick(std::slice::from_ref(app), query).is_empty()
}

fn looks_like_bundle_id(s: &str) -> bool {
    s.matches('.').count() >= 2 && !s.contains(' ') && !s.contains('/')
}

fn is_active(pid: i32) -> bool {
    with_app(pid, |app| unsafe { send_bool(app, sel(c"isActive")) }).unwrap_or(false)
}

/// The app in front. Two witnesses: the accessibility API's focused app,
/// then the app NSWorkspace marks active. Unknown when neither answers.
pub fn front() -> Seen<AppInfo> {
    if let Some((_, pid)) = crate::ax::focused_app() {
        if let Some(a) = resolve(pid) {
            return Seen::Yes(a);
        }
    }
    if let Some(a) = all().into_iter().find(|a| a.regular && is_active(a.pid)) {
        return Seen::Yes(a);
    }
    Seen::Unknown("neither the accessibility API nor NSWorkspace names an app in front".into())
}

/// The app in front, when it can be told.
pub fn frontmost() -> Option<AppInfo> {
    match front() {
        Seen::Yes(a) => Some(a),
        _ => None,
    }
}

/// Whether `pid` is the app in front.
pub fn front_is(pid: i32) -> Seen<()> {
    match front() {
        Seen::Yes(a) if a.pid == pid => Seen::Yes(()),
        Seen::Yes(_) => Seen::No,
        Seen::No => Seen::No,
        Seen::Unknown(why) => Seen::Unknown(why),
    }
}

/// Whether the app is hidden, as NSRunningApplication reports it.
pub fn hidden(pid: i32) -> Seen<()> {
    match with_app(pid, |app| unsafe { send_bool(app, sel(c"isHidden")) }) {
        Some(true) => Seen::Yes(()),
        Some(false) => Seen::No,
        None => Seen::Unknown("the app is gone".into()),
    }
}

// ---- ways to change an app. Callers check the result with `front_is` or
// `hidden`; a call returning Ok only means the request was accepted.

/// Raises an app through the accessibility API.
pub fn raise_ax(t: &Ticket, app: &AppInfo) -> Result<(), String> {
    Element::app(app.pid).set_flag(t, "AXFrontmost", true)
}

/// Raises an app through AppKit.
pub fn raise_appkit(_t: &Ticket, app: &AppInfo) -> Result<(), String> {
    // NSApplicationActivateAllWindows | NSApplicationActivateIgnoringOtherApps
    match with_app(app.pid, |a| unsafe { send_u64_bool(a, sel(c"activateWithOptions:"), 0b11) }) {
        Some(true) => Ok(()),
        _ => Err("AppKit refused".into()),
    }
}

/// Raises an app through LaunchServices, the way `open` does.
pub fn raise_launch_services(_t: &Ticket, app: &AppInfo) -> Result<(), String> {
    let key = if app.bundle.is_empty() { app.name.as_str() } else { app.bundle.as_str() };
    open(Some(key), None, false)
}

/// Hides an app through the accessibility API.
pub fn hide_ax(t: &Ticket, app: &AppInfo) -> Result<(), String> {
    Element::app(app.pid).set_flag(t, "AXHidden", true)
}

/// Hides an app through AppKit.
pub fn hide_appkit(_t: &Ticket, app: &AppInfo) -> Result<(), String> {
    match with_app(app.pid, |a| unsafe { send_bool(a, sel(c"hide")) }) {
        Some(true) => Ok(()),
        _ => Err("AppKit refused".into()),
    }
}

/// Opens an app by name or bundle id, with an optional file or URL.
#[allow(clippy::disallowed_methods)] // LaunchServices through `open`; its output is not parsed.
pub fn open(app: Option<&str>, target: Option<&str>, background: bool) -> Result<(), String> {
    let mut cmd = Command::new("/usr/bin/open");
    if background {
        cmd.arg("-g");
    }
    if let Some(a) = app {
        cmd.arg(if looks_like_bundle_id(a) { "-b" } else { "-a" }).arg(a);
    }
    if let Some(t) = target {
        cmd.arg(t);
    }
    if app.is_none() && target.is_none() {
        return Err("give a name, a bundle, or a target to open".into());
    }
    let out = cmd.output().map_err(|e| format!("could not run open: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Asks an app to quit the polite way. It may ask the person to save.
pub fn quit(_t: &Ticket, pid: i32) -> bool {
    with_app(pid, |app| unsafe { send_bool(app, sel(c"terminate")) }).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: i32, name: &str, bundle: &str) -> AppInfo {
        AppInfo { pid, name: name.into(), bundle: bundle.into(), regular: true }
    }

    #[test]
    fn pick_prefers_the_tightest_match() {
        let apps = vec![app(1, "Code", "com.microsoft.VSCode"), app(2, "Xcode", "com.apple.dt.Xcode")];
        assert_eq!(pick(&apps, "code").len(), 1);
        assert_eq!(pick(&apps, "code")[0].pid, 1);
        assert_eq!(pick(&apps, "com.apple.dt.xcode")[0].pid, 2);
        assert_eq!(pick(&apps, "cod").len(), 2);
        assert!(pick(&apps, "safari").is_empty());
    }

    #[test]
    fn bundle_ids_are_told_from_names() {
        assert!(looks_like_bundle_id("com.apple.calculator"));
        assert!(!looks_like_bundle_id("Google Chrome"));
        assert!(!looks_like_bundle_id("agent.exe"));
    }

    #[test]
    fn the_kernel_lists_this_process() {
        assert!(all_pids().contains(&(std::process::id() as i32)));
    }

    /// The text parser this module used to rely on, kept to show the bug.
    mod legacy {
        pub fn dock_names(text: &str) -> Vec<String> {
            let mut out = Vec::new();
            let mut cur: Option<(String, i32)> = None;
            let mut fg = false;
            let quoted = |s: &str| s.split('"').nth(1).unwrap_or("").to_string();
            for line in text.lines() {
                let t = line.trim_start();
                let header = t.split(')').next().is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                    && t.contains("ASN:");
                if header {
                    if let Some((n, pid)) = cur.take() {
                        if pid > 0 && fg {
                            out.push(n);
                        }
                    }
                    fg = false;
                    cur = Some((quoted(t), 0));
                } else if let Some(c) = cur.as_mut() {
                    if let Some(rest) = t.strip_prefix("pid = ") {
                        c.1 = rest.split_whitespace().next().and_then(|p| p.parse().ok()).unwrap_or(0);
                        fg = t.contains("type=\"Foreground\"");
                    }
                }
            }
            if let Some((n, pid)) = cur {
                if pid > 0 && fg {
                    out.push(n);
                }
            }
            out
        }
    }

    /// `lsappinfo list` as it printed LibreOffice right after its first
    /// launch through the Gatekeeper prompt: the pid only inside `token=`.
    /// The values are made up; the shape is what macOS printed.
    const GATEKEEPER_LAUNCH: &str = r#"76) "LibreOffice" ASN:0x0-0x123123: (in front)
    bundleID="org.libreoffice.script"
    bundle path="/Applications/LibreOffice.app"
    executable path="/Applications/LibreOffice.app/Contents/MacOS/soffice"
 token=[sess=100000 pid=4321 uid:501,501,501 g:20,20 pV:10000] type="Foreground" flavor=3 Version="26.8" fileType="APPL" creator="LibO" Arch=ARM64
77) "Finder" ASN:0x0-0x10010:
    bundleID="com.apple.finder"
    pid = 672 type="Foreground" flavor=3 Version="26.5" fileType="FNDR" creator="MACS" Arch=ARM64
"#;

    #[test]
    fn the_old_text_parser_lost_an_app_on_screen() {
        let names = legacy::dock_names(GATEKEEPER_LAUNCH);
        assert_eq!(names, vec!["Finder".to_string()], "LibreOffice was on screen but parsed away");
    }
}
