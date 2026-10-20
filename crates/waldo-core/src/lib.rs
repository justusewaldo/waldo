//! Eyes and hands for an agent on macOS.
//!
//! `exec::Executor` is the whole public surface: give it a tool name and
//! JSON arguments, and it returns text and sometimes a screenshot. The MCP
//! server and the command line are both thin shells around it.

pub mod apps;
pub mod ax;
pub mod cf;
pub mod exec;
pub mod ffi;
pub mod gate;
pub mod geom;
pub mod input;
pub mod screen;
pub mod state;
pub mod tools;
pub mod verify;
