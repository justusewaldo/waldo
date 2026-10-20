//! The Waldo bar: type a task, and Claude Code does it on this Mac. The bar
//! is a native macOS window, so on any other system it only says so.

#[cfg(target_os = "macos")]
mod bar;

#[cfg(target_os = "macos")]
fn main() {
    bar::main();
}

#[cfg(not(target_os = "macos"))]
fn main() -> std::process::ExitCode {
    eprintln!("{}", waldo_core::MACOS_ONLY);
    std::process::ExitCode::from(1)
}
