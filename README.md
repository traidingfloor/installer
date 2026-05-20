# TrAIding Floor — installer

Native installer for [TrAIding Floor](https://github.com/traidingfloor/install).
A small Tauri app that wraps Docker — operators double-click a `.dmg`, `.exe`,
or `.deb`, click through three screens, and end up on a running dashboard.
No terminal, no `curl | sh`.

The installer is open-source (you can audit what it does to your machine).
The trading-floor product it installs is closed-source and runs entirely in
Docker on your own hardware.

## What it does

1. Checks for Docker Desktop. If missing, points the operator at the
   download page and stops cleanly.
2. Creates the install directory under the OS-standard data location
   (`~/Library/Application Support/TrAIdingFloor` on macOS,
   `%LOCALAPPDATA%\TrAIdingFloor` on Windows,
   `~/.local/share/TrAIdingFloor` on Linux).
3. Downloads `docker-compose.yml` and seeds `user-data/.env` from the
   public install repo at <https://github.com/traidingfloor/install>.
4. Runs `docker compose pull && docker compose up -d`.
5. Polls `http://localhost/dashboard` until it responds, then opens it in
   the default browser.
6. Stays available in the system as the "stop containers", "open dashboard",
   and "show install folder" launcher. Closing the window doesn't stop
   trading — that takes an explicit Stop click.

## Architecture

| Layer | Tech | Purpose |
|---|---|---|
| Window shell | [Tauri 2.x](https://tauri.app) | Native window + signed binaries per OS |
| Backend | Rust (`src-tauri/src/main.rs`) | Docker detection, compose orchestration, IPC commands |
| Frontend | Zero-build vanilla HTML/CSS/JS (`src/`) | Three screens: welcome → progress → ready |
| Build/release | GitHub Actions (`.github/workflows/release.yml`) | Cross-platform builds + GitHub Releases |

Bundle size is around 8 MB per platform. The installer itself isn't the
heavy thing — Docker is. The first `compose pull` downloads ~350 MB of
images and that's where most of the wait sits.

## Build it yourself

You need Rust ≥ 1.75 and Node ≥ 20 for the Tauri CLI.

```bash
# One-time setup
npm install                          # installs @tauri-apps/cli
rustup toolchain install stable      # if Rust isn't already present

# Dev — opens the app with hot reload
npm run dev

# Build a release for your current OS
npm run build
# Output lands in src-tauri/target/release/bundle/{dmg,msi,deb,appimage}/
```

On Linux you also need a few system libraries:

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

## Release flow

A `v*.*.*` tag triggers `.github/workflows/release.yml`, which builds on
macOS / Ubuntu / Windows runners in parallel and attaches the resulting
binaries to a GitHub Release. The same workflow can also run manually
from the Actions tab.

```bash
# Ship a new release
git tag v0.1.1
git push origin v0.1.1
# 5-10 min later, the .dmg / .msi / .deb live at:
# https://github.com/traidingfloor/installer/releases/latest
```

The Tauri auto-updater inside the installed app reads
`https://github.com/traidingfloor/installer/releases/latest/download/latest.json`
on launch. When operators run an older version, they get a one-click upgrade
prompt on the Ready screen.

## Code signing

Builds work unsigned — operators just see a one-time "this app is from an
unidentified developer" warning on first launch (Gatekeeper on macOS,
SmartScreen on Windows). To ship without that warning, set the signing
secrets in repo Settings → Secrets and variables → Actions. See
`.github/workflows/release.yml` for the full secret list.

The Tauri **updater** signing key is different from OS code signing. The
updater key is mandatory (otherwise auto-updates are insecure) and is free
to generate locally:

```bash
npx @tauri-apps/cli signer generate -w ~/.tauri/traidingfloor.key
# Copy the printed PUBLIC KEY into src-tauri/tauri.conf.json
# (plugins → updater → pubkey)
# Copy the contents of ~/.tauri/traidingfloor.key into the
# TAURI_SIGNING_PRIVATE_KEY secret
# Save the passphrase as TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

OS-level signing (Apple Developer ID + Windows Authenticode) costs around
$100/yr for Apple plus ~$300/yr for Windows. Worth doing before any real
distribution but skippable for initial dogfooding.

## Why open source the installer?

The installer is a thin shell around `docker compose`. Operators have
every reason to want to audit what's about to run on their machine,
especially for a trading product. Open-sourcing the installer means
anyone can read the Rust source, see exactly which commands it runs, and
verify it isn't phoning home or doing anything sneaky. The IP that
matters lives inside the published Docker images, not in this wrapper.

## License

MIT. The Docker images this installer launches are closed-source.
