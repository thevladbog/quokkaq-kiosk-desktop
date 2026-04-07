# QuokkaQ Kiosk (desktop)

Tauri 2 shell that opens the **hosted** Next.js kiosk in a webview (same `/[locale]/kiosk/[unitId]` URLs as the browser). A small **Go sidecar** listens on `127.0.0.1:17431` and forwards raw ESC/POS bytes to a TCP printer (network receipt printers, port 9100 by default).

## First run: splash and pairing

On first launch the app opens a **local splash** screen. Enter:

1. **API base URL** — origin of the Go API (e.g. `https://api.example.com`), same host you would use for `/auth/login` (no `/api` prefix unless your API is mounted there).
2. **Terminal pairing code** — created in **Admin → Desktop terminals** in the web app.

The app calls `POST {api}/auth/terminal/bootstrap`, stores `desktop-profile.json` in the app config directory, opens `{appBaseUrl}/{locale}/kiosk/{unitId}` from the API response, and sets `access_token` / `NEXT_LOCALE` in the webview’s `localStorage`.

The splash shows the bundled mascot asset under `src/splash-assets/quokka-logo.svg`.

## Configure the URL (resolution order)

1. Environment variable `QUOKKAQ_KIOSK_URL` (e.g. `https://app.example.com/ru/kiosk/<unitId>`) — **no** automatic token injection.
2. **`desktop-profile.json`** in the app config directory (written after successful pairing). Includes token, unit, locale, app base URL, and **`kioskFullscreen`** from **Admin → Desktop terminals**; kiosk URL is derived from that file.
3. Legacy text file `kiosk-url.txt` in the app config directory (first line = URL only, no token).
4. Otherwise the webview stays on the **splash** (`splash.html`) until pairing succeeds.

Config directory examples:

- **macOS:** `~/Library/Application Support/com.quokkaq.kiosk/`
- **Windows:** `%APPDATA%\com.quokkaq.kiosk\`
- **Linux:** `~/.config/com.quokkaq.kiosk/`

Optional:

- `QUOKKAQ_KIOSK_FULLSCREEN=1` — force fullscreen (or `0`/unset segment to rely on the terminal’s **Kiosk mode (fullscreen)** flag in the admin UI). If this variable is set to any **non-empty** value, only `1`/`true`/`yes`/`on` enable fullscreen; otherwise the app uses `kioskFullscreen` from `desktop-profile.json` after pairing.
- `QUOKKAQ_AGENT_LISTEN` — override agent bind address (default `127.0.0.1:17431`).

## Remote IPC (printing from your domain)

Edit [`src-tauri/capabilities/remote-kiosk.json`](src-tauri/capabilities/remote-kiosk.json) and set `remote.urls` to match the origins where your Next app is served (wildcards supported, see [Tauri capabilities](https://v2.tauri.app/security/capabilities/)). Rebuild the desktop app after changing this list.

## Build

Requirements: **Rust (stable)**, **Go 1.26+**, **Node** (for the Tauri CLI).

```bash
cd quokkaq-kiosk-desktop
npm install
npm run build:agent   # writes src-tauri/binaries/quokkaq-kiosk-agent-<triple>
npm run tauri dev     # or: npm run build  for release installers
```

`npm run build` runs `build:agent` then `tauri build`. For cross-platform installers, run that on each target OS (or use CI).

### GitHub Actions

- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) — `pull_request` and `push` to `main` / `master` (matrix: macOS, Windows, Linux).
- [`.github/workflows/release-prod-release.yml`](.github/workflows/release-prod-release.yml) — `push` to `prod-release`: bumps `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` (unless the commit message contains `[skip ci]`), tags `v*`, builds, publishes a [GitHub Release](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) with bundle artifacts. Bump type follows the latest commit: `[major]`, `[minor]` / `feat:`, else patch.

### App icon

Bundle icons (`.icns`, `.ico`, PNG sizes under `src-tauri/icons/`) are generated from the master image [`src/assets/app_icon.png`](src/assets/app_icon.png). After replacing that file, regenerate before building:

```bash
npm run tauri -- icon src/assets/app_icon.png
```

## Agent HTTP API

- `GET /health` — liveness.
- `GET /v1/printers` — `{ "printers": [ { "name": "...", "isDefault": bool } ], "error"?: "..." }` (lists OS print queues: CUPS on macOS/Linux, PowerShell `Get-Printer` on Windows).
- `POST /v1/print` — JSON `{ "mode": "tcp" | "system", "target": "host:port" | "QueueName", "payload": "<base64>" }`. Legacy `{ "address": "host:port", "payload": "..." }` is still accepted as TCP.

## Backend CORS

The webview loads your normal web origin; API calls use that origin. Ensure `CORS_ALLOWED_ORIGINS` on the Go API includes your deployed app URL (defaults in `cmd/api/main.go` include the QuokkaQ staging hosts plus localhost / 127.0.0.1).
