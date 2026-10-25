//! Desktop app: a capture thread feeds the engine; the window reads a snapshot and gets updates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Context;
use acompanion::app::{Rect, Shared, Status, classify, on_screen, save_status};
use acompanion::events::Opcodes;
use acompanion::live::Engine;
use acompanion::state::{Key, State};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Game server port and embedded opcode table, as in `re`.
const PORT: u16 = 13328;
const BUILD: &str = "default";

struct AppState {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    /// Raised by `meter_clear`; the capture loop empties the saved fights (the engine is its own).
    clear: Arc<AtomicBool>,
    /// Characters `forget_character` queued; the capture loop removes them (the engine is its own).
    forget: Arc<Mutex<Vec<Key>>>,
    capture: Mutex<Option<JoinHandle<()>>>,
    /// The main window is closing: the overlay going away with it is not the user switching it off.
    quitting: AtomicBool,
    /// Closing the main window hides it instead (the frontend keeps a tray icon to bring it back).
    close_to_tray: AtomicBool,
    /// Reopen capture when it stops for a reason that may pass (see `app::retryable`).
    reconnect: Arc<AtomicBool>,
}

/// The update `check_update` found, kept for `install_update`.
struct PendingUpdate(Mutex<Option<Update>>);

/// Stop capture and wait for the engine to flush its state to disk. Safe to call twice.
fn stop_capture(handle: &AppHandle) {
    let app = handle.state::<AppState>();
    app.stop.store(true, Ordering::Relaxed);
    if let Some(t) = app.capture.lock().unwrap().take() {
        let _ = t.join();
    }
}

#[tauri::command]
fn characters(app: tauri::State<'_, AppState>) -> Value {
    app.shared.lock().unwrap().characters.clone()
}

#[tauri::command]
fn capture_status(app: tauri::State<'_, AppState>) -> Value {
    app.shared.lock().unwrap().status.to_json()
}

#[tauri::command]
fn game_detected(app: tauri::State<'_, AppState>) -> bool {
    app.shared.lock().unwrap().game
}

#[tauri::command]
fn radar(app: tauri::State<'_, AppState>) -> Value {
    app.shared.lock().unwrap().radar.clone()
}

#[tauri::command]
fn meter(app: tauri::State<'_, AppState>) -> Value {
    app.shared.lock().unwrap().meter.clone()
}

#[tauri::command]
fn meter_history(app: tauri::State<'_, AppState>) -> Value {
    app.shared.lock().unwrap().meter_history.clone()
}

#[tauri::command]
fn meter_clear(app: tauri::State<'_, AppState>) {
    app.clear.store(true, Ordering::Relaxed);
}

/// Remove a saved character; it comes back only if it logs in again.
#[tauri::command]
fn forget_character(app: tauri::State<'_, AppState>, server: u16, name: String) {
    app.forget.lock().unwrap().push((server, name));
}

/// The preferences the Rust side acts on; the frontend owns and stores them and sends them at start and on change.
#[tauri::command]
fn app_settings(app: tauri::State<'_, AppState>, close_to_tray: bool, reconnect: bool) {
    app.close_to_tray.store(close_to_tray, Ordering::Relaxed);
    app.reconnect.store(reconnect, Ordering::Relaxed);
}

/// Really quit (the close button only hides the window when closing to the tray).
#[tauri::command]
fn quit(handle: AppHandle) {
    handle.state::<AppState>().quitting.store(true, Ordering::Relaxed);
    close_overlays(&handle);
    handle.exit(0);
}

/// Off the main thread: it waits on the UAC prompt.
#[tauri::command(async)]
fn restart_as_admin(handle: AppHandle) -> Result<bool, String> {
    let accepted = acompanion::app::restart_as_admin().map_err(|e| format!("{e:#}"))?;
    if accepted {
        handle.exit(0);
    }
    Ok(accepted)
}

/// The newer version published on GitHub Releases, if any. Dev builds never ask.
#[tauri::command]
async fn check_update(handle: AppHandle, pending: tauri::State<'_, PendingUpdate>) -> Result<Option<String>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let update = handle.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let version = update.as_ref().map(|u| u.version.clone());
    *pending.0.lock().unwrap() = update;
    Ok(version)
}

/// Download the update, flush the state, then run the installer, which restarts the app.
#[tauri::command]
async fn install_update(handle: AppHandle, pending: tauri::State<'_, PendingUpdate>) -> Result<(), String> {
    let update = pending.0.lock().unwrap().clone().ok_or("no update to install")?;
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    // the installer ends this process without a RunEvent::Exit
    stop_capture(&handle);
    update.install(bytes).map_err(|e| e.to_string())?;
    handle.restart()
}

/// Each overlay view is its own window, labelled `overlay-<view>`.
const OVERLAY_PREFIX: &str = "overlay-";
const OVERLAY_VIEWS: [&str; 3] = ["radar", "dailies", "meter"];

fn close_overlays(handle: &AppHandle) {
    for (label, w) in handle.webview_windows() {
        if label.starts_with(OVERLAY_PREFIX) {
            let _ = w.close();
        }
    }
}

/// An overlay window, created hidden: its saved position (or the centre, when that monitor is gone) is applied here and
/// the window shows or hides itself from then on (`Overlay`, auto-hide).
/// ponytail: overlays opened for the first time start stacked at the centre; cascade them if that bothers anyone.
fn open_overlay(handle: &AppHandle, label: &str, bounds: Option<[i32; 4]>) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into()))
        .title("AcompanION overlay")
        .inner_size(420.0, 420.0)
        .min_inner_size(200.0, 120.0)
        .decorations(false)
        .transparent(true)
        // Windows gives an undecorated window with a shadow a 1px white border: the overlay draws its own (on hover)
        .shadow(false)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let monitors: Vec<Rect> = win
        .available_monitors()?
        .iter()
        .map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32))
        .collect();
    match bounds.filter(|&[x, y, w, h]| w > 0 && h > 0 && on_screen((x, y, w, h), &monitors)) {
        // Position first: the window is born on the primary monitor, and moving it to a monitor with another scale
        // factor makes Windows rescale it, so a size set before the move came out multiplied by the ratio.
        Some([x, y, w, h]) => {
            win.set_position(PhysicalPosition::new(x, y))?;
            win.set_size(PhysicalSize::new(w as u32, h as u32))?;
        }
        None => win.center()?,
    }
    Ok(win)
}

/// Open, close or update the overlay. Async: Windows deadlocks when a window is built on the main thread.
#[tauri::command(async)]
fn overlay(handle: AppHandle, view: String, open: bool, on_top: bool, click_through: bool, bounds: Option<[i32; 4]>) -> Result<(), String> {
    if !OVERLAY_VIEWS.contains(&view.as_str()) {
        return Err(format!("unknown overlay view: {view}"));
    }
    let label = format!("{OVERLAY_PREFIX}{view}");
    let existing = handle.get_webview_window(&label);
    if !open {
        return existing.map_or(Ok(()), |w| w.close()).map_err(|e| e.to_string());
    }
    let win = match existing {
        Some(w) => w,
        None => open_overlay(&handle, &label, bounds).map_err(|e| e.to_string())?,
    };
    win.set_always_on_top(on_top).map_err(|e| e.to_string())?;
    win.set_ignore_cursor_events(click_through).map_err(|e| e.to_string())
}

fn set_status(handle: &AppHandle, shared: &Mutex<Shared>, status: Status) {
    let json = status.to_json();
    {
        let mut published = shared.lock().unwrap();
        if published.status == status { return; }
        published.status = status;
    }
    let _ = handle.emit("capture://status", json);
}

fn set_game(handle: &AppHandle, shared: &Mutex<Shared>, game: bool) {
    shared.lock().unwrap().game = game;
    let _ = handle.emit("capture://game", game);
}

/// How long to wait before reopening a capture that stopped.
/// ponytail: fixed; back off if reopening ever hammers a failing adapter. A route change the adapter does not report
/// (the capture stays open on the old one) is not followed either.
const RETRY_EVERY: Duration = Duration::from_secs(5);

#[allow(clippy::too_many_arguments)]
fn capture(handle: AppHandle, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>, clear: Arc<AtomicBool>, forget: Arc<Mutex<Vec<Key>>>, reconnect: Arc<AtomicBool>, state: State, path: PathBuf, guard: File) {
    let ops = match Opcodes::embedded(BUILD) {
        Ok(ops) => ops,
        Err(e) => {
            drop(guard);
            return set_status(&handle, &shared, Status::failed("error", &e));
        }
    };
    // One engine across reopenings: the characters and the fights carry on, and the decoder resyncs over the gap.
    let mut engine = Engine::new(ops, PORT, state, Some(path));
    let last = loop {
        let result = acompanion::live::run(
            None,
            &format!("tcp port {PORT}"),
            &mut engine,
            &stop,
            &clear,
            &forget,
            || set_status(&handle, &shared, Status::new("running")),
            |state, keys| {
                let changed: Vec<Value> = keys.iter().filter_map(|k| state.get(k)).map(|c| c.to_json()).collect();
                // a key the state no longer has was removed
                let removed: Vec<Value> =
                    keys.iter().filter(|k| state.get(k).is_none()).map(|(server, name)| serde_json::json!({"server": server, "name": name})).collect();
                shared.lock().unwrap().characters = state.to_json()["characters"].clone();
                let _ = handle.emit("characters://update", changed);
                if !removed.is_empty() {
                    let _ = handle.emit("characters://removed", removed);
                }
            },
            |result| set_status(&handle, &shared, save_status(result)),
            |game| set_game(&handle, &shared, game),
            |radar| {
                shared.lock().unwrap().radar = radar.clone();
                let _ = handle.emit("radar://update", radar);
            },
            |meter| {
                let mut light = meter.clone();
                let history = light.as_object_mut().and_then(|o| o.remove("history"));
                {
                    let mut s = shared.lock().unwrap();
                    s.meter = light;
                    if let Some(h) = history {
                        s.meter_history = h;
                    }
                }
                let _ = handle.emit("meter://update", meter);
            },
        );
        set_game(&handle, &shared, false);
        let Err(e) = result else { break None };
        let status = classify(&e, acompanion::npcap::is_elevated());
        if stop.load(Ordering::Relaxed) || !reconnect.load(Ordering::Relaxed) || !acompanion::app::retryable(status.id) {
            break Some(status);
        }
        set_status(&handle, &shared, Status::failed("reconnecting", &e));
        let until = Instant::now() + RETRY_EVERY;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if stop.load(Ordering::Relaxed) {
            break None;
        }
    };
    // Release before showing a restart button: the elevated replacement needs this lock.
    drop(guard);
    if let Some(status) = last {
        set_status(&handle, &shared, status);
    }
}

fn main() {
    // An unreadable state file stops capture: the engine would overwrite it on its first save.
    let loaded = acompanion::state::default_path()
        .context("APPDATA is not set")
        .and_then(|p| Ok((acompanion::app::load_state(&p)?, p)));
    let (status, saved, start) = match loaded {
        Ok(((state, guard), path)) => {
            let saved = state.to_json()["characters"].clone();
            match guard {
                Some(guard) => (Status::new("starting"), saved, Some((state, path, guard))),
                None => (Status::new("state_in_use"), saved, None),
            }
        }
        Err(e) => (Status::failed("state_unreadable", &e), Value::Array(Vec::new()), None),
    };
    // the saved fights show even when capture never starts
    let fights = acompanion::app::saved_history(acompanion::state::default_path().as_deref());
    let shared = Arc::new(Mutex::new(Shared { status, characters: saved, game: false, radar: Value::Null, meter: Value::Null, meter_history: fights }));
    let stop = Arc::new(AtomicBool::new(false));
    let clear = Arc::new(AtomicBool::new(false));
    let forget = Arc::new(Mutex::new(Vec::new()));
    // on until the frontend says otherwise; it sends the saved preference as soon as it starts
    let reconnect = Arc::new(AtomicBool::new(true));

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![characters, capture_status, game_detected, radar, meter, meter_history, meter_clear, forget_character, overlay, restart_as_admin, check_update, install_update, app_settings, quit])
        .setup(move |app| {
            let reconnect_flag = reconnect.clone();
            let thread = start.map(|(state, path, guard)| {
                let (handle, shared, stop, clear, forget) = (app.handle().clone(), shared.clone(), stop.clone(), clear.clone(), forget.clone());
                std::thread::spawn(move || capture(handle, shared, stop, clear, forget, reconnect, state, path, guard))
            });
            app.manage(AppState { shared, stop, clear, forget, capture: Mutex::new(thread), quitting: AtomicBool::new(false), close_to_tray: AtomicBool::new(false), reconnect: reconnect_flag });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("cannot start AcompanION")
        .run(|handle, event| {
            match event {
                RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { api, .. }, .. } if label == "main" => {
                    // with a tray icon to bring it back, closing only hides the window (the overlay stays)
                    if handle.state::<AppState>().close_to_tray.load(Ordering::Relaxed) {
                        api.prevent_close();
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        return;
                    }
                    // the overlay must not keep the app alive (or be remembered as switched off)
                    handle.state::<AppState>().quitting.store(true, Ordering::Relaxed);
                    close_overlays(handle);
                }
                RunEvent::WindowEvent { label, event: WindowEvent::Destroyed, .. } => {
                    if let Some(view) = label.strip_prefix(OVERLAY_PREFIX)
                        && !handle.state::<AppState>().quitting.load(Ordering::Relaxed)
                    {
                        let _ = handle.emit_to("main", "overlay://closed", view);
                    }
                }
                // joining lets `live::run` flush the engine to disk
                RunEvent::Exit => stop_capture(handle),
                _ => {}
            }
        });
}
