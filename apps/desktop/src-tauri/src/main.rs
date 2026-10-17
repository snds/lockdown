// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "shell")]
fn main() {
    lockdown_desktop_lib::run();
}

#[cfg(not(feature = "shell"))]
fn main() {
    eprintln!(
        "lockdown-desktop was built without the `shell` feature (no Tauri); \
         run `pnpm tauri dev` in apps/desktop, or build with `--features shell`"
    );
    std::process::exit(2);
}
