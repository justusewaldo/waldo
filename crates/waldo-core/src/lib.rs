//! Eyes and hands for an agent on macOS.
//!
//! `exec::Executor` is the whole public surface: give it a tool name and
//! JSON arguments, and it returns text and sometimes a screenshot. The MCP
//! server and the command line are both thin shells around it.
//!
//! Waldo drives macOS. On other systems the crate still builds, so MCP
//! directories that start servers on Linux can list the tools: `exec` is
//! then a stand-in whose every tool answers with [`MACOS_ONLY`].

pub mod geom;
pub mod state;
pub mod tools;

#[cfg(target_os = "macos")]
pub mod apps;
#[cfg(target_os = "macos")]
pub mod ax;
#[cfg(target_os = "macos")]
pub mod cf;
#[cfg(target_os = "macos")]
pub mod ffi;
#[cfg(target_os = "macos")]
pub mod gate;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
pub mod panel;
#[cfg(target_os = "macos")]
pub mod screen;
#[cfg(target_os = "macos")]
pub mod verify;

#[cfg_attr(not(target_os = "macos"), path = "exec_elsewhere.rs")]
pub mod exec;

/// What every tool and command says on a system other than macOS.
pub const MACOS_ONLY: &str = "Waldo runs on macOS only. It drives Mac apps through the macOS accessibility API. Install it on a Mac: https://justusewaldo.com";
