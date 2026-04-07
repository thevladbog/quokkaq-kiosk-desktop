//! QuokkaQ Kiosk: remote webview + local Go print agent sidecar.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

const AGENT_HTTP: &str = "http://127.0.0.1:17431";
const PROFILE_FILE: &str = "desktop-profile.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopProfile {
    api_base_url: String,
    access_token: String,
    unit_id: String,
    default_locale: String,
    app_base_url: String,
    /// From server terminal settings; missing in older profile files = false.
    #[serde(default)]
    kiosk_fullscreen: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapResponse {
    token: String,
    unit_id: String,
    default_locale: String,
    app_base_url: String,
    #[serde(default)]
    kiosk_fullscreen: bool,
}

/// If `QUOKKAQ_KIOSK_FULLSCREEN` is set to a non-empty value, it wins (1/true/yes/on → fullscreen).
fn effective_kiosk_fullscreen(from_profile: bool) -> bool {
    if let Ok(v) = std::env::var("QUOKKAQ_KIOSK_FULLSCREEN") {
        if !v.trim().is_empty() {
            return matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
    }
    from_profile
}

fn apply_kiosk_window_mode(win: &tauri::WebviewWindow, fullscreen: bool) {
    let _ = win.set_fullscreen(fullscreen);
    let _ = win.set_decorations(!fullscreen);
}

fn trim_slash(s: &str) -> &str {
    s.trim_end_matches('/')
}

fn config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))
}

fn read_desktop_profile(app: &AppHandle) -> Option<DesktopProfile> {
    let path = config_dir(app).ok()?.join(PROFILE_FILE);
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn profile_is_ready(p: &DesktopProfile) -> bool {
    !p.access_token.is_empty()
        && !p.unit_id.is_empty()
        && !p.app_base_url.is_empty()
        && !p.default_locale.is_empty()
}

fn write_desktop_profile(app: &AppHandle, profile: &DesktopProfile) -> Result<(), String> {
    let dir = config_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(PROFILE_FILE);
    let data =
        serde_json::to_string_pretty(profile).map_err(|e| format!("profile json: {e}"))?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

fn kiosk_url_from_profile(p: &DesktopProfile) -> Result<tauri::Url, String> {
    let base = trim_slash(p.app_base_url.trim());
    let loc = p.default_locale.trim();
    let loc = if loc.is_empty() { "en" } else { loc };
    let url = format!("{}/{}/kiosk/{}", base, loc, p.unit_id.trim());
    url.parse()
        .map_err(|e| format!("kiosk url parse error: {e}"))
}

/// Legacy single-line URL file (no token injection).
fn read_legacy_kiosk_url_file(app: &AppHandle) -> Option<tauri::Url> {
    let dir = config_dir(app).ok()?;
    let path = dir.join("kiosk-url.txt");
    let s = std::fs::read_to_string(path).ok()?;
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse().ok()
}

fn inject_token_later(win: tauri::WebviewWindow, token: String, locale: String) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1800));
        let tok_js = serde_json::to_string(&token).unwrap_or_else(|_| "\"\"".to_string());
        let loc_js = serde_json::to_string(&locale).unwrap_or_else(|_| "\"en\"".to_string());
        let script = format!(
            "try {{ localStorage.setItem('access_token', {tok_js}); localStorage.setItem('NEXT_LOCALE', {loc_js}); }} catch (e) {{ console.error(e); }}"
        );
        let _ = win.eval(script);
    });
}

/// `Some(fs)` only when navigation used `desktop-profile.json` (so window mode matches that profile).
fn apply_initial_navigation(
    app: &AppHandle,
    win: &tauri::WebviewWindow,
) -> Result<Option<bool>, String> {
    if let Ok(u) = std::env::var("QUOKKAQ_KIOSK_URL") {
        let t = u.trim();
        if !t.is_empty() {
            let url: tauri::Url = t
                .parse()
                .map_err(|e| format!("QUOKKAQ_KIOSK_URL parse error: {e}"))?;
            win.navigate(url).map_err(|e| e.to_string())?;
            return Ok(None);
        }
    }

    if let Some(p) = read_desktop_profile(app) {
        if profile_is_ready(&p) {
            let url = kiosk_url_from_profile(&p)?;
            win.navigate(url).map_err(|e| e.to_string())?;
            inject_token_later(win.clone(), p.access_token.clone(), p.default_locale.clone());
            return Ok(Some(p.kiosk_fullscreen));
        }
    }

    if let Some(url) = read_legacy_kiosk_url_file(app) {
        win.navigate(url).map_err(|e| e.to_string())?;
        return Ok(None);
    }

    Ok(None)
}

fn spawn_print_agent(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let shell = app.shell();
        let sidecar = match shell.sidecar("quokkaq-kiosk-agent") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[quokkaq-kiosk] sidecar init failed: {e}");
                return;
            }
        };
        let (mut rx, _child) = match sidecar.spawn() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[quokkaq-kiosk] sidecar spawn failed: {e}");
                return;
            }
        };
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    eprintln!("[agent] {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stderr(line) => {
                    eprintln!("[agent] {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Error(err) => {
                    eprintln!("[quokkaq-kiosk] sidecar error: {err}");
                }
                CommandEvent::Terminated(status) => {
                    eprintln!("[quokkaq-kiosk] sidecar terminated: {status:?}");
                    break;
                }
                _ => {}
            }
        }
    });
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrintReceiptArgs {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    target: String,
    /// Legacy TCP target (host:port); used when mode/target omitted.
    #[serde(default)]
    address: Option<String>,
    payload_base64: String,
}

/// Send a print job to the local agent (`tcp` → host:port, `system` → OS queue name).
#[tauri::command]
fn print_receipt(args: PrintReceiptArgs) -> Result<(), String> {
    let mut mode = args.mode.trim().to_string();
    let mut target = args.target.trim().to_string();
    if mode.is_empty() && target.is_empty() {
        if let Some(addr) = args.address.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            mode = "tcp".to_string();
            target = addr.to_string();
        }
    }
    if mode.is_empty() {
        mode = "tcp".to_string();
    }
    if target.is_empty() {
        return Err("target or address is required".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "mode": mode,
        "target": target,
        "payload": args.payload_base64,
    });

    let resp = client
        .post(format!("{AGENT_HTTP}/v1/print"))
        .json(&body)
        .send()
        .map_err(|e| format!("agent request failed: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(format!("agent error: {text}"));
    }

    Ok(())
}

/// JSON string: `{ "printers": [...], "error"?: string }` from the local agent.
#[tauri::command]
fn list_printers() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!("{AGENT_HTTP}/v1/printers"))
        .send()
        .map_err(|e| format!("agent request failed: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(format!("agent error: {text}"));
    }

    resp.text().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairTerminalArgs {
    api_base_url: String,
    pairing_code: String,
}

#[tauri::command]
fn pair_terminal(app: AppHandle, args: PairTerminalArgs) -> Result<(), String> {
    let base = trim_slash(args.api_base_url.trim());
    if base.is_empty() {
        return Err("apiBaseUrl is required".to_string());
    }
    let code = args.pairing_code.trim();
    if code.is_empty() {
        return Err("pairing code is required".to_string());
    }

    let url = format!("{}/auth/terminal/bootstrap", base);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("API {status}: {text}"));
    }

    let body: BootstrapResponse =
        serde_json::from_str(&text).map_err(|e| format!("invalid API response: {e}"))?;

    let profile = DesktopProfile {
        api_base_url: base.to_string(),
        access_token: body.token.clone(),
        unit_id: body.unit_id.clone(),
        default_locale: body.default_locale.clone(),
        app_base_url: trim_slash(body.app_base_url.trim()).to_string(),
        kiosk_fullscreen: body.kiosk_fullscreen,
    };
    write_desktop_profile(&app, &profile)?;

    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let kiosk = kiosk_url_from_profile(&profile)?;
    win.navigate(kiosk).map_err(|e| e.to_string())?;
    apply_kiosk_window_mode(&win, effective_kiosk_fullscreen(profile.kiosk_fullscreen));
    inject_token_later(win, body.token, body.default_locale);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            print_receipt,
            list_printers,
            pair_terminal
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            spawn_print_agent(handle.clone());

            if let Some(win) = app.get_webview_window("main") {
                let profile_kiosk_fs = apply_initial_navigation(&handle, &win)?;
                apply_kiosk_window_mode(
                    &win,
                    effective_kiosk_fullscreen(profile_kiosk_fs.unwrap_or(false)),
                );
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
