//! Desktop app: a capture thread feeds the engine; the window reads a snapshot and gets updates.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anyhow::Context;
use acompanion::app::{Rect, Shared, Status, classify, on_screen, save_status};
use acompanion::events::Opcodes;
use acompanion::live::Engine;
use acompanion::state::State;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

/// Game server port and embedded opcode table, as in `re`.
const PORT: u16 = 13328;
const BUILD: &str = "default";

struct AppState {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    /// Raised by `meter_clear`; the capture loop empties the saved fights (the engine is its own).
    clear: Arc<AtomicBool>,
    capture: Mutex<Option<JoinHandle<()>>>,
    /// The main window is closing: the overlay going away with it is not the user switching it off.
    quitting: AtomicBool,
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

/// Off the main thread: it waits on the UAC prompt.
#[tauri::command(async)]
fn restart_as_admin(handle: AppHandle) -> Result<bool, String> {
    let accepted = acompanion::app::restart_as_admin().map_err(|e| format!("{e:#}"))?;
    if accepted {
        handle.exit(0);
    }
    Ok(accepted)
}

const OVERLAY: &str = "overlay";

/// The overlay window, created hidden: its saved position (or the centre, when that monitor is gone) is applied here and
/// the window shows or hides itself from then on (`Overlay`, auto-hide).
fn open_overlay(handle: &AppHandle, bounds: Option<[i32; 4]>) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(handle, OVERLAY, WebviewUrl::App("index.html".into()))
        .title("AcompanION overlay")
        .inner_size(420.0, 420.0)
        .min_inner_size(200.0, 120.0)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let monitors: Vec<Rect> = win
        .available_monitors()?
        .iter()
        .map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32))
        .collect();
    match bounds.filter(|&[x, y, w, h]| w > 0 && h > 0 && on_screen((x, y, w, h), &monitors)) {
        Some([x, y, w, h]) => {
            win.set_size(PhysicalSize::new(w as u32, h as u32))?;
            win.set_position(PhysicalPosition::new(x, y))?;
        }
        None => win.center()?,
    }
    Ok(win)
}

/// Open, close or update the overlay. Async: Windows deadlocks when a window is built on the main thread.
#[tauri::command(async)]
fn overlay(handle: AppHandle, open: bool, on_top: bool, click_through: bool, bounds: Option<[i32; 4]>) -> Result<(), String> {
    let existing = handle.get_webview_window(OVERLAY);
    if !open {
        return existing.map_or(Ok(()), |w| w.close()).map_err(|e| e.to_string());
    }
    let win = match existing {
        Some(w) => w,
        None => open_overlay(&handle, bounds).map_err(|e| e.to_string())?,
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

fn capture(handle: AppHandle, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>, clear: Arc<AtomicBool>, state: State, path: PathBuf, guard: File) {
    let ops = match Opcodes::embedded(BUILD) {
        Ok(ops) => ops,
        Err(e) => {
            drop(guard);
            return set_status(&handle, &shared, Status::failed("error", &e));
        }
    };
    let mut engine = Engine::new(ops, PORT, state, Some(path));
    let result = acompanion::live::run(
        None,
        &format!("tcp port {PORT}"),
        &mut engine,
        &stop,
        &clear,
        || set_status(&handle, &shared, Status::new("running")),
        |state, keys| {
            let changed: Vec<Value> = keys.iter().filter_map(|k| state.get(k)).map(|c| c.to_json()).collect();
            shared.lock().unwrap().characters = state.to_json()["characters"].clone();
            let _ = handle.emit("characters://update", changed);
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
    // Release before showing a restart button: the elevated replacement needs this lock.
    drop(guard);
    set_game(&handle, &shared, false);
    if let Err(e) = result {
        set_status(&handle, &shared, classify(&e, acompanion::npcap::is_elevated()));
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

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![characters, capture_status, game_detected, radar, meter, meter_history, meter_clear, overlay, restart_as_admin])
        .setup(move |app| {
            let thread = start.map(|(state, path, guard)| {
                let (handle, shared, stop, clear) = (app.handle().clone(), shared.clone(), stop.clone(), clear.clone());
                std::thread::spawn(move || capture(handle, shared, stop, clear, state, path, guard))
            });
            app.manage(AppState { shared, stop, clear, capture: Mutex::new(thread), quitting: AtomicBool::new(false) });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("cannot start AcompanION")
        .run(|handle, event| {
            match event {
                RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { .. }, .. } if label == "main" => {
                    // the overlay must not keep the app alive (or be remembered as switched off)
                    handle.state::<AppState>().quitting.store(true, Ordering::Relaxed);
                    if let Some(w) = handle.get_webview_window(OVERLAY) {
                        let _ = w.close();
                    }
                }
                RunEvent::WindowEvent { label, event: WindowEvent::Destroyed, .. } if label == OVERLAY => {
                    if !handle.state::<AppState>().quitting.load(Ordering::Relaxed) {
                        let _ = handle.emit_to("main", "overlay://closed", ());
                    }
                }
                RunEvent::Exit => {
                    let app = handle.state::<AppState>();
                    app.stop.store(true, Ordering::Relaxed);
                    // joining lets `live::run` flush the engine to disk
                    if let Some(t) = app.capture.lock().unwrap().take() {
                        let _ = t.join();
                    }
                }
                _ => {}
            }
        });
}
