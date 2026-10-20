//! Displays, on-screen windows, and screenshots.

use crate::cf::Cf;
use crate::ffi::*;
use crate::geom::Rect;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct Display {
    pub id: u32,
    /// 1-based. Display 1 is the main display.
    pub index: usize,
    /// Global screen points. The main display starts at 0,0.
    pub bounds: Rect,
    /// Real pixels per point.
    pub native: f64,
}

pub fn can_capture() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

pub fn request_capture() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

pub fn displays() -> Vec<Display> {
    let mut ids = [0u32; 16];
    let mut count: u32 = 0;
    let err = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) };
    if err != 0 || count == 0 {
        ids[0] = unsafe { CGMainDisplayID() };
        count = 1;
    }
    let main = unsafe { CGMainDisplayID() };
    let mut list: Vec<u32> = ids[..count as usize].to_vec();
    // The capture tool numbers the main display first.
    list.sort_by_key(|id| *id != main);
    list.iter()
        .enumerate()
        .map(|(i, &id)| {
            let b = unsafe { CGDisplayBounds(id) };
            let bounds =
                Rect { x: b.origin.x, y: b.origin.y, w: b.size.width, h: b.size.height };
            let native = unsafe {
                let mode = CGDisplayCopyDisplayMode(id);
                if mode.is_null() || bounds.w <= 0.0 {
                    1.0
                } else {
                    let px = CGDisplayModeGetPixelWidth(mode) as f64;
                    CGDisplayModeRelease(mode);
                    (px / bounds.w).max(1.0)
                }
            };
            Display { id, index: i + 1, bounds, native }
        })
        .collect()
}

/// The display that holds a point, or the main display.
pub fn display_at(list: &[Display], x: f64, y: f64) -> Display {
    list.iter().find(|d| d.bounds.contains(x, y)).unwrap_or(&list[0]).clone()
}

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: i32,
    pub app: String,
    pub title: String,
    pub frame: Rect,
    pub layer: i64,
}

/// On-screen windows, front to back. Titles need the Screen Recording grant.
pub fn windows() -> Vec<WindowInfo> {
    let raw = unsafe {
        CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            0,
        )
    };
    let Some(arr) = Cf::owned(raw) else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0..arr.array_len() {
        let Some(d) = Cf::retained(arr.array_get(i)) else { continue };
        let num = |k: &str| d.dict_get(k).and_then(|v| v.to_f64());
        let text = |k: &str| d.dict_get(k).and_then(|v| v.to_string()).unwrap_or_default();
        let frame = d
            .dict_get("kCGWindowBounds")
            .map(|b| {
                let n = |k: &str| b.dict_get(k).and_then(|v| v.to_f64()).unwrap_or(0.0);
                Rect { x: n("X"), y: n("Y"), w: n("Width"), h: n("Height") }
            })
            .unwrap_or_default();
        if num("kCGWindowAlpha").unwrap_or(1.0) <= 0.0 {
            continue;
        }
        out.push(WindowInfo {
            id: num("kCGWindowNumber").unwrap_or(0.0) as u32,
            pid: num("kCGWindowOwnerPID").unwrap_or(0.0) as i32,
            app: text("kCGWindowOwnerName"),
            title: text("kCGWindowName"),
            frame,
            layer: num("kCGWindowLayer").unwrap_or(0.0) as i64,
        });
    }
    out
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_path(dir: &Path, tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("shot-{}-{}-{}.jpg", std::process::id(), n, tag))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[allow(clippy::disallowed_methods)] // image resize with sips; output is a file, not parsed text
fn resize(src: &Path, dst: &Path, w: u32, h: u32) -> Result<(), String> {
    run(Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "70", "-z"])
        .arg(h.to_string())
        .arg(w.to_string())
        .arg(src)
        .arg("--out")
        .arg(dst))
}

fn grant_hint(err: String) -> String {
    if can_capture() {
        format!("screenshot failed: {err}")
    } else {
        "Screen Recording is not granted to the app that runs this tool. Run `waldo grant`.".into()
    }
}

/// One display as a JPEG of exactly `w` by `h` pixels, pointer included.
#[allow(clippy::disallowed_methods)] // screenshots come from screencapture; output is an image file
pub fn capture_display(d: &Display, w: u32, h: u32, dir: &Path) -> Result<Vec<u8>, String> {
    let raw = temp_path(dir, "raw");
    let small = temp_path(dir, "out");
    let shot = run(Command::new("/usr/sbin/screencapture")
        .args(["-x", "-C", "-t", "jpg"])
        .arg(format!("-D{}", d.index))
        .arg(&raw));
    let result = shot.and_then(|_| resize(&raw, &small, w, h)).and_then(|_| {
        std::fs::read(&small).map_err(|e| format!("could not read the screenshot: {e}"))
    });
    let _ = std::fs::remove_file(&raw);
    let _ = std::fs::remove_file(&small);
    result.map_err(grant_hint)
}

/// A rectangle of the screen at full detail, for reading small text.
/// `rect` is in global screen points. Returns the JPEG and its pixel size.
#[allow(clippy::disallowed_methods)] // screenshots come from screencapture; output is an image file
pub fn capture_region(
    rect: Rect,
    native: f64,
    max_edge: f64,
    dir: &Path,
) -> Result<(Vec<u8>, u32, u32), String> {
    let raw = temp_path(dir, "raw");
    let small = temp_path(dir, "out");
    let long = (rect.w.max(rect.h) * native).max(1.0);
    let k = (max_edge / long).min(1.0) * native;
    let (w, h) = ((rect.w * k).round().max(1.0) as u32, (rect.h * k).round().max(1.0) as u32);
    let shot = run(Command::new("/usr/sbin/screencapture")
        .args(["-x", "-t", "jpg", "-R"])
        .arg(format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
        .arg(&raw));
    let result = shot.and_then(|_| resize(&raw, &small, w, h)).and_then(|_| {
        std::fs::read(&small).map_err(|e| format!("could not read the screenshot: {e}"))
    });
    let _ = std::fs::remove_file(&raw);
    let _ = std::fs::remove_file(&small);
    result.map(|bytes| (bytes, w, h)).map_err(grant_hint)
}
