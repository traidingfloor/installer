# App icons

Place a 1024×1024 PNG named `icon.png` in this folder and run:

```bash
npx @tauri-apps/cli icon icons/icon.png
```

That generates every platform-specific icon (`.icns` for macOS, `.ico` for
Windows, multiple PNG sizes for Linux) into this folder. Tauri's
`tauri.conf.json` references those generated files automatically.

For the initial public release, a simple wordmark / logomark works fine —
e.g. the letters "TF" in white on the project's green (#2ea043).
