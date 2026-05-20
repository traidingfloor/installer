// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Serialize;
use tauri::Manager;

const COMPOSE_URL: &str =
    "https://raw.githubusercontent.com/traidingfloor/install/main/docker-compose.yml";
const ENV_EXAMPLE_URL: &str =
    "https://raw.githubusercontent.com/traidingfloor/install/main/.env.example";
const DASHBOARD_URL: &str = "http://localhost";
const DASHBOARD_PROBE_URL: &str = "http://localhost/dashboard";

// ─── Status reporting ───────────────────────────────────────────────────────
//
// Every Rust step that the UI cares about is emitted as a tauri event so the
// frontend can render a live progress feed without polling. The shape stays
// stable across versions so we can add new step names without breaking old
// frontends.
#[derive(Clone, Serialize)]
struct Step {
    kind: String,    // "info" | "ok" | "warn" | "error"
    message: String,
}

fn emit(window: &tauri::Window, kind: &str, message: &str) {
    let _ = window.emit(
        "installer:step",
        Step {
            kind: kind.to_string(),
            message: message.to_string(),
        },
    );
}

// ─── Where files live ───────────────────────────────────────────────────────
//
// We store the operator's compose file + user-data under the OS-standard
// "Application Support" / "AppData" / "$XDG_DATA_HOME" location. That keeps
// the install discoverable, survives app updates, and isn't accidentally
// nuked by the installer's own update flow.
fn install_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| "Could not resolve OS data directory".to_string())?;
    Ok(base.join("TrAIdingFloor"))
}

// ─── Tauri commands invoked from the frontend ────────────────────────────────

#[tauri::command]
fn check_docker(window: tauri::Window) -> Result<DockerStatus, String> {
    emit(&window, "info", "Checking for Docker...");

    // 1) Is the docker binary on PATH?
    let docker_path = which::which("docker");
    let docker_present = docker_path.is_ok();
    if !docker_present {
        emit(
            &window,
            "warn",
            "Docker Desktop is not installed on this machine.",
        );
        return Ok(DockerStatus {
            present: false,
            running: false,
            compose_v2: false,
            version: None,
        });
    }
    emit(&window, "ok", "Found Docker binary.");

    // 2) Is the daemon reachable?
    let daemon_ok = Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !daemon_ok {
        emit(
            &window,
            "warn",
            "Docker is installed but the daemon isn't running. Please launch Docker Desktop and wait for the whale icon to settle, then click Continue.",
        );
        return Ok(DockerStatus {
            present: true,
            running: false,
            compose_v2: false,
            version: None,
        });
    }

    // 3) Compose v2 plugin available?
    let compose_v2 = Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    // 4) Version string (best-effort, used in the UI).
    let version = Command::new("docker")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    emit(&window, "ok", "Docker daemon is reachable.");
    Ok(DockerStatus {
        present: true,
        running: true,
        compose_v2,
        version,
    })
}

#[derive(Serialize)]
struct DockerStatus {
    present: bool,
    running: bool,
    compose_v2: bool,
    version: Option<String>,
}

#[tauri::command]
fn install_dir_path() -> Result<String, String> {
    install_dir().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn ensure_install_dir(window: tauri::Window) -> Result<String, String> {
    let dir = install_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create install dir: {e}"))?;
    std::fs::create_dir_all(dir.join("user-data"))
        .map_err(|e| format!("Could not create user-data dir: {e}"))?;
    emit(
        &window,
        "ok",
        &format!("Install directory ready at {}", dir.display()),
    );
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
async fn download_compose(window: tauri::Window) -> Result<(), String> {
    let dir = install_dir()?;
    let compose_path = dir.join("docker-compose.yml");
    if compose_path.exists() {
        emit(
            &window,
            "info",
            "docker-compose.yml already present, leaving it in place.",
        );
        return Ok(());
    }

    emit(&window, "info", "Downloading docker-compose.yml...");
    let body = reqwest::get(COMPOSE_URL)
        .await
        .map_err(|e| format!("Network error: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Could not read response: {e}"))?;
    std::fs::write(&compose_path, body).map_err(|e| format!("Could not write file: {e}"))?;
    emit(&window, "ok", "Downloaded docker-compose.yml.");

    // Seed user-data/.env from the .env.example template, but only on first run.
    let env_path = dir.join("user-data").join(".env");
    if !env_path.exists() {
        emit(&window, "info", "Seeding .env from template...");
        let env_body = reqwest::get(ENV_EXAMPLE_URL)
            .await
            .map_err(|e| format!("Network error: {e}"))?
            .text()
            .await
            .map_err(|e| format!("Could not read response: {e}"))?;
        std::fs::write(&env_path, env_body)
            .map_err(|e| format!("Could not write .env: {e}"))?;
        emit(&window, "ok", "Created user-data/.env from template.");
    }

    Ok(())
}

#[tauri::command]
fn compose_up(window: tauri::Window, channel: Option<String>) -> Result<(), String> {
    let dir = install_dir()?;
    let channel = channel.unwrap_or_else(|| "latest".to_string());

    emit(
        &window,
        "info",
        &format!("Pulling images (channel = {channel})..."),
    );
    let pull = Command::new("docker")
        .args(["compose", "pull"])
        .current_dir(&dir)
        .env("IMAGE_TAG", &channel)
        .status()
        .map_err(|e| format!("Could not run docker compose pull: {e}"))?;
    if !pull.success() {
        emit(&window, "error", "docker compose pull failed.");
        return Err("docker compose pull failed".into());
    }
    emit(&window, "ok", "Images pulled.");

    emit(&window, "info", "Starting containers...");
    let up = Command::new("docker")
        .args(["compose", "up", "-d"])
        .current_dir(&dir)
        .env("IMAGE_TAG", &channel)
        .status()
        .map_err(|e| format!("Could not run docker compose up: {e}"))?;
    if !up.success() {
        emit(&window, "error", "docker compose up failed.");
        return Err("docker compose up failed".into());
    }
    emit(&window, "ok", "Containers running.");

    Ok(())
}

#[tauri::command]
fn compose_down(window: tauri::Window) -> Result<(), String> {
    let dir = install_dir()?;
    emit(&window, "info", "Stopping containers (data preserved)...");
    let status = Command::new("docker")
        .args(["compose", "down"])
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("Could not run docker compose down: {e}"))?;
    if !status.success() {
        return Err("docker compose down failed".into());
    }
    emit(&window, "ok", "Containers stopped.");
    Ok(())
}

#[tauri::command]
async fn wait_for_dashboard(window: tauri::Window) -> Result<(), String> {
    emit(&window, "info", "Waiting for the dashboard to come up...");
    for attempt in 1..=60 {
        if let Ok(resp) = reqwest::get(DASHBOARD_PROBE_URL).await {
            if resp.status().is_success() || resp.status().is_redirection() {
                emit(&window, "ok", "Dashboard is live.");
                return Ok(());
            }
        }
        if attempt % 5 == 0 {
            emit(
                &window,
                "info",
                &format!("Still waiting... ({} of 60 attempts)", attempt),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    emit(
        &window,
        "warn",
        "Dashboard didn't respond within 120s. Containers are likely still starting; try opening http://localhost in a minute.",
    );
    Err("Dashboard did not respond in time".into())
}

#[tauri::command]
fn open_dashboard() -> Result<(), String> {
    // The opener plugin handles cross-platform "open URL in default browser"
    // including the Windows nuance where shell-open via cmd.exe quotes wrong.
    open::that(DASHBOARD_URL).map_err(|e| format!("Could not open browser: {e}"))?;
    Ok(())
}

#[tauri::command]
fn open_install_dir() -> Result<(), String> {
    let dir = install_dir()?;
    open::that(&dir).map_err(|e| format!("Could not open folder: {e}"))?;
    Ok(())
}

// ─── App entry ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            check_docker,
            install_dir_path,
            ensure_install_dir,
            download_compose,
            compose_up,
            compose_down,
            wait_for_dashboard,
            open_dashboard,
            open_install_dir
        ])
        .setup(|app| {
            // Give the JS side an app handle so the updater plugin works.
            let _ = app.handle();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running TrAIding Floor installer");
}

fn main() {
    run();
}
