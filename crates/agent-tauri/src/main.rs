// Prevents an extra console window on Windows release builds. Standard Tauri
// boilerplate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(debug_assertions)]
use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime},
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
    let frontend_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend");
    let reason = match frontend_available("localhost:5173", &frontend_dir, Duration::from_secs(2)) {
        Ok(_) => return,
        Err(reason) => reason,
    };

    eprintln!(
        "\
Shinkai's Tauri frontend is not available for this direct cargo launch.
Reason: {reason}

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
fn frontend_available(
    dev_server_addr: &str,
    frontend_dir: &Path,
    timeout_budget: Duration,
) -> Result<FrontendSource, String> {
    if dev_server_reachable(dev_server_addr, timeout_budget) {
        return Ok(FrontendSource::DevServer);
    }
    validate_built_frontend(frontend_dir).map(|_| FrontendSource::BuiltDist)
}

#[cfg(debug_assertions)]
#[derive(Debug, Eq, PartialEq)]
enum FrontendSource {
    DevServer,
    BuiltDist,
}

#[cfg(debug_assertions)]
fn validate_built_frontend(frontend_dir: &Path) -> Result<(), String> {
    let dist_dir = frontend_dir.join("dist");
    let dist_index = dist_dir.join("index.html");
    let index_metadata = fs::metadata(&dist_index).map_err(|_| {
        format!(
            "{} is missing; run the Tauri dev wrapper or build the frontend first",
            dist_index.display()
        )
    })?;
    if !index_metadata.is_file() {
        return Err(format!("{} is not a file", dist_index.display()));
    }

    let index_html = fs::read_to_string(&dist_index)
        .map_err(|err| format!("failed to read {}: {err}", dist_index.display()))?;
    let missing_assets = referenced_dist_assets(&index_html)
        .into_iter()
        .filter(|asset| !dist_dir.join(asset).is_file())
        .collect::<Vec<_>>();
    if !missing_assets.is_empty() {
        return Err(format!(
            "built frontend is incomplete; missing dist asset(s): {}",
            missing_assets.join(", ")
        ));
    }

    let dist_mtime = index_metadata
        .modified()
        .map_err(|err| format!("failed to inspect {}: {err}", dist_index.display()))?;
    if let Some((source_mtime, source_path)) = newest_frontend_source(frontend_dir) {
        if source_mtime > dist_mtime {
            return Err(format!(
                "built frontend is stale; {} is newer than {}",
                source_path.display(),
                dist_index.display()
            ));
        }
    }

    Ok(())
}

#[cfg(debug_assertions)]
fn referenced_dist_assets(index_html: &str) -> Vec<String> {
    let mut assets = index_html
        .split(['"', '\''])
        .filter_map(|value| value.strip_prefix("./"))
        .map(|value| value.split(['?', '#']).next().unwrap_or("").trim())
        .filter(|value| !value.is_empty() && !value.contains(".."))
        .map(str::to_string)
        .collect::<Vec<_>>();
    assets.sort();
    assets.dedup();
    assets
}

#[cfg(debug_assertions)]
fn newest_frontend_source(frontend_dir: &Path) -> Option<(SystemTime, PathBuf)> {
    let mut newest = None;
    for relative in [
        "index.html",
        "package.json",
        "tsconfig.json",
        "vite.config.ts",
        "public",
        "src",
    ] {
        collect_newest_file_mtime(&frontend_dir.join(relative), &mut newest);
    }
    newest
}

#[cfg(debug_assertions)]
fn collect_newest_file_mtime(path: &Path, newest: &mut Option<(SystemTime, PathBuf)>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_newest_file_mtime(&entry.path(), newest);
        }
        return;
    }
    if !metadata.is_file() {
        return;
    }
    let Ok(modified) = metadata.modified() else {
        return;
    };
    if newest
        .as_ref()
        .map(|(current, _)| modified > *current)
        .unwrap_or(true)
    {
        *newest = Some((modified, path.to_path_buf()));
    }
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
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dist/assets")).unwrap();
        std::fs::write(
            dir.join("dist/index.html"),
            r#"<!doctype html><script src="./assets/app.js"></script><link href="./assets/app.css" rel="stylesheet">"#,
        )
        .unwrap();
        std::fs::write(dir.join("dist/assets/app.js"), "console.log('ok')").unwrap();
        std::fs::write(dir.join("dist/assets/app.css"), "body{}").unwrap();

        assert_eq!(
            frontend_available("127.0.0.1:0", &dir, Duration::from_millis(1)),
            Ok(FrontendSource::BuiltDist)
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn frontend_available_rejects_incomplete_built_dist() {
        let dir = std::env::temp_dir().join(format!(
            "incomplete-shinkai-frontend-check-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dist")).unwrap();
        std::fs::write(
            dir.join("dist/index.html"),
            r#"<!doctype html><script src="./assets/missing.js"></script>"#,
        )
        .unwrap();

        let err = frontend_available("127.0.0.1:0", &dir, Duration::from_millis(1)).unwrap_err();
        assert!(err.contains("missing dist asset"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn frontend_available_rejects_missing_dist_and_server() {
        let missing =
            std::env::temp_dir().join(format!("missing-shinkai-frontend-{}", std::process::id()));

        assert!(frontend_available("127.0.0.1:0", &missing, Duration::from_millis(1)).is_err());
    }
}
