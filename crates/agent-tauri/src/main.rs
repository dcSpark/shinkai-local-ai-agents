// Prevents an extra console window on Windows release builds. Standard Tauri
// boilerplate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agent_tauri_lib::run()
}
