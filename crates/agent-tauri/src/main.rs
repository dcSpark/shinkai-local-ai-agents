// Prevents an extra console window on Windows release builds. Standard Tauri
// boilerplate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(debug_assertions)]
use std::{
    net::{TcpStream, ToSocketAddrs},
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
    let reason = match frontend_available("localhost:5173", Duration::from_secs(2)) {
        Ok(_) => return,
        Err(reason) => reason,
    };

    eprintln!(
        "\
Shinkai's Tauri frontend is not available for this direct cargo launch.
Reason: {reason}

`cargo run --bin shinkai` starts only the Rust side, so the debug WebView needs the Vite dev server from tauri.conf.json.

Run the app with:
  npm --prefix crates/agent-tauri/frontend run tauri -- dev

For a manual dev-server flow:
  npm --prefix crates/agent-tauri/frontend run dev
  cargo run -p agent-tauri --bin shinkai

For a production-style smoke test, build the frontend and run the release binary:
  npm --prefix crates/agent-tauri/frontend run build
  cargo run --release -p agent-tauri --bin shinkai

Set SHINKAI_SKIP_FRONTEND_CHECK=1 to bypass this debug preflight."
    );
    std::process::exit(78);
}

#[cfg(debug_assertions)]
fn frontend_available(dev_server_addr: &str, timeout_budget: Duration) -> Result<(), String> {
    if dev_server_reachable(dev_server_addr, timeout_budget) {
        return Ok(());
    }
    Err(format!(
        "Vite dev server is not reachable at http://{dev_server_addr}; start it with `npm --prefix crates/agent-tauri/frontend run tauri -- dev` or `npm --prefix crates/agent-tauri/frontend run dev`."
    ))
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
    fn frontend_available_accepts_reachable_dev_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        assert_eq!(frontend_available(&addr, Duration::from_millis(1)), Ok(()));
    }

    #[test]
    fn frontend_available_rejects_missing_dev_server() {
        let err = frontend_available("127.0.0.1:0", Duration::from_millis(1)).unwrap_err();
        assert!(err.contains("Vite dev server"));
    }
}
