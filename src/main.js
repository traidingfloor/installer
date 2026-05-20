// ─── TrAIding Floor installer — frontend ─────────────────────────────────
//
// This file is intentionally zero-build vanilla JS. With
// `app.withGlobalTauri = true` in tauri.conf.json, Tauri injects the API
// on window.__TAURI__ before the WebView loads our script, so no bundler
// is required. To debug the UI in a normal browser, mock these calls.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { openUrl } = window.__TAURI__.opener;

// ─── DOM helpers ──────────────────────────────────────────────────────────

const $ = (id) => document.getElementById(id);

function show(screenId) {
  document.querySelectorAll(".screen").forEach((s) => s.classList.remove("active"));
  $(screenId).classList.add("active");
}

function appendLog(kind, message) {
  const li = document.createElement("li");
  li.className = `${kind} fade-in`;
  const marker = { ok: "✓", warn: "!", error: "✗", info: "·" }[kind] || "·";
  li.innerHTML = `<span class="marker">${marker}</span><span>${escapeHtml(message)}</span>`;
  const log = $("progress-log");
  log.appendChild(li);
  log.scrollTop = log.scrollHeight;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  }[c]));
}

// ─── Backend wiring ───────────────────────────────────────────────────────

// Rust emits "installer:step" events for every status update. The progress
// screen subscribes once at startup and lets them flow into the log.
listen("installer:step", (event) => {
  const { kind, message } = event.payload;
  appendLog(kind, message);
});

// ─── Welcome → Docker check ───────────────────────────────────────────────

async function init() {
  try {
    const dir = await invoke("install_dir_path");
    $("install-dir-display").textContent = dir;
  } catch (_) {
    $("install-dir-display").textContent = "(unavailable)";
  }

  $("btn-start").addEventListener("click", goDockerCheck);

  // Background: silently check for an installer update on launch. v2: show
  // banner + one-click apply on Ready screen. v1 just logs it.
  silentUpdateCheck();
}

async function silentUpdateCheck() {
  try {
    if (!window.__TAURI__.updater) return;
    const upd = await window.__TAURI__.updater.check();
    if (upd) window.__tfUpdate = upd;
  } catch (_) {
    // Updater unavailable in dev/sideload builds is non-fatal.
  }
}

async function goDockerCheck() {
  show("screen-docker");
  await runDockerCheck();
}

async function runDockerCheck() {
  const card = $("docker-status");
  card.innerHTML = `<div class="row"><span class="icon info">·</span><div>Checking Docker…</div></div>`;
  let status;
  try {
    status = await invoke("check_docker");
  } catch (e) {
    card.innerHTML = `<div class="row"><span class="icon error">✗</span><div>${escapeHtml(String(e))}</div></div>`;
    return;
  }

  const rows = [];
  if (!status.present) {
    rows.push(row("error", "Docker Desktop is not installed."));
    rows.push(row("info", "Click “How do I install Docker?” below. Re-launch this app after Docker is installed."));
  } else {
    rows.push(row("ok", `Docker found ${status.version ? "(" + escapeHtml(status.version) + ")" : ""}.`));
    if (!status.running) {
      rows.push(row("warn", "Docker is installed but the daemon isn't running. Launch Docker Desktop, wait for the whale icon to settle, then click Retry."));
    } else {
      rows.push(row("ok", "Docker daemon is reachable."));
      if (status.compose_v2) {
        rows.push(row("ok", "Compose v2 plugin available."));
      } else {
        rows.push(row("warn", "Compose v2 not detected. Update Docker Desktop to the latest version."));
      }
    }
  }
  card.innerHTML = rows.join("");

  // If everything's green, auto-advance to the install step.
  if (status.present && status.running && status.compose_v2) {
    setTimeout(runInstall, 600);
  }
}

function row(kind, msg) {
  const marker = { ok: "✓", warn: "!", error: "✗", info: "·" }[kind] || "·";
  return `<div class="row"><span class="icon ${kind}">${marker}</span><div>${msg}</div></div>`;
}

$("btn-docker-retry").addEventListener("click", runDockerCheck);
$("btn-docker-help").addEventListener("click", async () => {
  await openUrl("https://www.docker.com/products/docker-desktop/");
});

// ─── Install flow ─────────────────────────────────────────────────────────

async function runInstall() {
  show("screen-progress");
  $("progress-log").innerHTML = "";

  try {
    await invoke("ensure_install_dir");
    await invoke("download_compose");
    await invoke("compose_up", { channel: "latest" });
    await invoke("wait_for_dashboard");
    show("screen-ready");
    maybePromptUpdate();
  } catch (e) {
    appendLog("error", `Install failed: ${e}`);
    appendLog("info", "Open the install folder to inspect logs, or report at github.com/traidingfloor/installer/issues.");
    show("screen-ready");
  }
}

// ─── Ready screen actions ─────────────────────────────────────────────────

$("btn-open").addEventListener("click", () => invoke("open_dashboard"));
$("btn-open-folder").addEventListener("click", () => invoke("open_install_dir"));
$("btn-stop").addEventListener("click", async () => {
  await invoke("compose_down");
});
$("link-source").addEventListener("click", async (e) => {
  e.preventDefault();
  await openUrl(e.currentTarget.href);
});

function maybePromptUpdate() {
  const upd = window.__tfUpdate;
  if (!upd) return;
  appendLog("info", `A new installer version is available (${upd.version}). Restart the app to upgrade automatically.`);
}

init();
