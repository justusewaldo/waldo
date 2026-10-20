//! The gate panel: a small Swift app that shows a hard stop, never takes the
//! keyboard, and takes only a real click. Its source is built into every copy
//! of Waldo, so an install from crates.io can build the panel too. `waldo
//! grant` does that once, when it finds no panel.

use crate::state;
use std::fs;
use std::path::{Path, PathBuf};

/// The panel's Swift source.
pub const SOURCE: &str = include_str!("../swift/ask.swift");

/// The panel's file name, next to the waldo binary or in the state folder.
pub const NAME: &str = "waldo-ask";

/// Where `waldo grant` builds the panel when none came with Waldo.
pub fn built_path() -> PathBuf {
    state::dir().join(NAME)
}

/// The panel Waldo uses: the one next to this binary (`make install` puts
/// it there), else the one `waldo grant` built in the state folder.
pub fn find() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    find_in(exe.parent(), &state::dir())
}

fn find_in(exe_dir: Option<&Path>, state_dir: &Path) -> Option<PathBuf> {
    exe_dir
        .map(|d| d.join(NAME))
        .into_iter()
        .chain(std::iter::once(state_dir.join(NAME)))
        .find(|p| p.is_file())
}

/// Builds the panel from [`SOURCE`] with swiftc, into `dest`. The Xcode
/// command line tools provide swiftc. The new panel replaces an old one in a
/// single rename, so a gate never finds half a file.
#[allow(clippy::disallowed_methods)] // swiftc builds the panel; only its error text is shown, never parsed
pub fn build(dest: &Path) -> Result<(), String> {
    let dir = dest.parent().ok_or("The panel needs a folder.")?;
    fs::create_dir_all(dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))?;
    let src = dir.join(format!("{NAME}.swift"));
    let tmp = dir.join(format!("{NAME}.building"));
    fs::write(&src, SOURCE).map_err(|e| format!("Could not write {}: {e}", src.display()))?;
    let run = std::process::Command::new("swiftc").arg("-O").arg(&src).arg("-o").arg(&tmp).output();
    let _ = fs::remove_file(&src);
    let out = match run {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("swiftc was not found. Install the Xcode command line tools with `xcode-select --install`, then run `waldo grant` again.".into())
        }
        Err(e) => return Err(format!("Could not run swiftc: {e}")),
    };
    if !out.status.success() {
        let _ = fs::remove_file(&tmp);
        let detail = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = detail.lines().rev().take(5).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("swiftc could not build the panel:\n{}", tail.join("\n")));
    }
    fs::rename(&tmp, dest).map_err(|e| format!("Could not move the panel into {}: {e}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("waldo-panel-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_panel_next_to_the_binary_comes_first_then_the_built_one() {
        let bin = temp("bin");
        let home = temp("home");
        assert_eq!(find_in(Some(&bin), &home), None);
        fs::write(home.join(NAME), "built").unwrap();
        assert_eq!(find_in(Some(&bin), &home), Some(home.join(NAME)));
        fs::write(bin.join(NAME), "installed").unwrap();
        assert_eq!(find_in(Some(&bin), &home), Some(bin.join(NAME)));
        let _ = fs::remove_dir_all(&bin);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn the_embedded_source_is_the_panel() {
        assert!(SOURCE.contains("NSPanel") || SOURCE.contains("NSWindow"));
        assert!(SOURCE.contains("Waldo"));
    }

    #[test]
    fn swiftc_builds_the_embedded_panel() {
        let dir = temp("build");
        let dest = dir.join(NAME);
        build(&dest).expect("swiftc builds the panel");
        let mode = fs::metadata(&dest).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "the panel is executable");
        assert!(!dir.join(format!("{NAME}.swift")).exists(), "the source is cleaned up");
        assert!(!dir.join(format!("{NAME}.building")).exists(), "nothing half built is left");
        let _ = fs::remove_dir_all(&dir);
    }
}
