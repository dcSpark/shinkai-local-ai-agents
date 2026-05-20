// Prevents an extra console window on Windows release builds. Standard Tauri
// boilerplate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(debug_assertions)]
use std::{
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn main() {
    #[cfg(debug_assertions)]
    ensure_tauri_frontend_available();
    agent_tauri_lib::run()
}

#[cfg(debug_assertions)]
fn ensure_tauri_frontend_available() {
    if std::env::var_os("SHINKAI_SKIP_FRONTEND_CHECK").is_some() {
        return;
    }
    let dist_index = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/dist/index.html");
    if frontend_available("localhost:5173", &dist_index, Duration::from_secs(2)) {
        return;
    }

    eprintln!(
        "\
Shinkai's Tauri frontend is not available.
`cargo run --bin shinkai` starts only the Rust side, so the WebView needs either a Vite dev server or a built frontend.

Run the app with:
  npm --prefix crates/agent-tauri/frontend run tauri -- dev

Or build the frontend first, then run the binary:
  npm --prefix crates/agent-tauri/frontend run build
  cargo run -p agent-tauri --bin shinkai

For a manual dev-server flow:
  npm --prefix crates/agent-tauri/frontend run dev
  cargo run -p agent-tauri --bin shinkai

Set SHINKAI_SKIP_FRONTEND_CHECK=1 to bypass this debug preflight."
    );
    std::process::exit(78);
}

#[cfg(debug_assertions)]
fn frontend_available(dev_server_addr: &str, dist_index: &Path, timeout_budget: Duration) -> bool {
    dist_index.exists() || dev_server_reachable(dev_server_addr, timeout_budget)
}

#[cfg(debug_assertions)]
fn dev_server_reachable(addr: &str, timeout_budget: Duration) -> bool {
    let Ok(addresses) = addr.to_socket_addrs() else {
        return false;
    };
    let addresses = addresses.collect::<Vec<_>>();
    if addresses.is_empty() {
        return false;
    }

    let deadline = Instant::now() + timeout_budget;
    while Instant::now() < deadline {
        for address in &addresses {
            if TcpStream::connect_timeout(address, Duration::from_millis(150)).is_ok() {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn frontend_available_accepts_built_dist() {
        let dir =
            std::env::temp_dir().join(format!("shinkai-frontend-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let index = dir.join("index.html");
        std::fs::write(&index, "<!doctype html>").unwrap();

        assert!(frontend_available(
            "127.0.0.1:0",
            &index,
            Duration::from_millis(1)
        ));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn frontend_available_rejects_missing_dist_and_server() {
        let missing =
            std::env::temp_dir().join(format!("missing-shinkai-frontend-{}", std::process::id()));

        assert!(!frontend_available(
            "127.0.0.1:0",
            &missing,
            Duration::from_millis(1)
        ));
    }
}
