//! App glue that is testable without a window: capture status, its classification, and the
//! UAC relaunch.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::live::NoAdapter;
use crate::meter::{history_path, load_history};
use crate::npcap::NotInstalled;
use crate::state::{self, State};

/// A busy state is still readable, but only the lock owner may start capture.
pub fn load_state(path: &Path) -> Result<(State, Option<File>)> {
    let guard = state::try_lock(path)?;
    Ok((State::load(path)?, guard))
}

/// The file another instance creates next to the state file to ask the lock owner to close.
pub fn quit_path(state: &Path) -> PathBuf {
    let mut quit = state.as_os_str().to_owned();
    quit.push(".quit");
    PathBuf::from(quit)
}

/// The saved fights next to the state file, shown before (or without) capture; empty when the
/// state path is unknown.
pub fn saved_history(state_path: Option<&Path>) -> Value {
    Value::Array(state_path.map(|p| load_history(&history_path(p))).unwrap_or_default())
}

/// Capture status: a stable `id` (never display text) and the raw error chain, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub id: &'static str,
    pub detail: Option<String>,
}

impl Status {
    pub fn new(id: &'static str) -> Self {
        Self { id, detail: None }
    }

    pub fn failed(id: &'static str, err: &anyhow::Error) -> Self {
        Self { id, detail: Some(format!("{err:#}")) }
    }

    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "detail": self.detail})
    }
}

/// Map a `live::run` failure to a status. Elevation is asked (`npcap::is_elevated`), never parsed
/// from the message; a missing Npcap or adapter is not fixed by elevating, so those win.
pub fn classify(err: &anyhow::Error, elevated: bool) -> Status {
    let id = if err.downcast_ref::<NotInstalled>().is_some() {
        "npcap_missing"
    } else if err.downcast_ref::<NoAdapter>().is_some() {
        "no_adapter"
    } else if !elevated {
        "not_admin"
    } else {
        "error"
    };
    Status::failed(id, err)
}

/// Whether a stopped capture is worth reopening: a lost adapter or a transient error may come back,
/// a missing Npcap or missing rights will not until the user acts.
pub fn retryable(id: &str) -> bool {
    matches!(id, "no_adapter" | "error")
}

/// Capture continues after a failed save; only a completed write clears this warning.
pub fn save_status(result: Result<(), &anyhow::Error>) -> Status {
    match result {
        Ok(()) => Status::new("running"),
        Err(e) => Status::failed("save_failed", e),
    }
}

/// What the capture thread publishes and the commands read.
pub struct Shared {
    pub status: Status,
    /// `State::to_json()["characters"]`.
    pub characters: Value,
    /// The game's port is carrying traffic (`live::Engine::game_active`).
    pub game: bool,
    /// `Radar::to_json()` as last handed out (`null` before the first).
    pub radar: Value,
    /// `Engine::meter_update` without its `history`, as last handed out (`null` before the first).
    pub meter: Value,
    /// The finished fights, as last handed out (before the first: the saved ones, `saved_history`).
    pub meter_history: Value,
}

/// A rectangle in physical pixels: `(x, y, width, height)`.
pub type Rect = (i32, i32, i32, i32);

/// Whether a saved overlay position can still be grabbed: its title bar (top centre) is on a connected monitor.
pub fn on_screen(bounds: Rect, monitors: &[Rect]) -> bool {
    let (x, y, w, _) = bounds;
    let (px, py) = (x + w / 2, y + 16);
    monitors.iter().any(|&(mx, my, mw, mh)| px >= mx && px < mx + mw && py >= my && py < my + mh)
}

/// Where a window of `size` sits in a corner of the `work` area, `margin` pixels in from the edges.
/// `None` for anything but `top-left`, `top-right`, `bottom-left` and `bottom-right`.
pub fn corner_pos(work: Rect, size: (i32, i32), corner: &str, margin: i32) -> Option<(i32, i32)> {
    let (x, y, w, h) = work;
    let (left, top) = (x + margin, y + margin);
    let (right, bottom) = (x + w - size.0 - margin, y + h - size.1 - margin);
    match corner {
        "top-left" => Some((left, top)),
        "top-right" => Some((right, top)),
        "bottom-left" => Some((left, bottom)),
        "bottom-right" => Some((right, bottom)),
        _ => None,
    }
}

/// PowerShell that relaunches `exe` through UAC; it exits non-zero when the prompt is declined.
pub fn runas_script(exe: &str) -> String {
    format!("Start-Process -FilePath '{}' -Verb RunAs -ErrorAction Stop", exe.replace('\'', "''"))
}

/// Relaunch elevated and wait for the prompt. `Ok(true)`: accepted, the new instance is starting.
pub fn restart_as_admin() -> Result<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    // absolute path: never run a planted powershell.exe
    let ps = PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let exe = std::env::current_exe()?;
    let status = std::process::Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-Command", &runas_script(&exe.to_string_lossy())])
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, anyhow};
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    /// Built the way `Npcap::load` and `default_device` build them.
    fn not_installed() -> anyhow::Error {
        Err::<(), _>(std::io::Error::other("module not found"))
            .with_context(|| NotInstalled(PathBuf::from(r"C:\Windows\System32\Npcap\wpcap.dll")))
            .unwrap_err()
    }

    fn no_adapter() -> anyhow::Error {
        None::<()>.with_context(|| NoAdapter(Ipv4Addr::new(10, 0, 0, 1))).unwrap_err()
    }

    #[test]
    fn a_saved_overlay_must_be_reachable_on_a_connected_monitor() {
        let main = (0, 0, 1920, 1080);
        let right = (1920, 0, 1920, 1080);
        assert!(on_screen((100, 100, 420, 420), &[main]));
        assert!(on_screen((2000, 50, 420, 420), &[main, right]));
        assert!(!on_screen((2000, 50, 420, 420), &[main]), "second monitor unplugged");
        assert!(!on_screen((100, -300, 420, 420), &[main]), "title bar above the screen");
        assert!(!on_screen((100, 100, 420, 420), &[]));
    }

    #[test]
    fn the_alert_sits_in_the_chosen_corner_of_the_work_area() {
        // a work area that does not start at the origin (a taskbar on the left and top)
        let work = (60, 40, 1860, 1040);
        let size = (320, 84);
        assert_eq!(corner_pos(work, size, "top-left", 16), Some((76, 56)));
        assert_eq!(corner_pos(work, size, "top-right", 16), Some((1860 + 60 - 320 - 16, 56)));
        assert_eq!(corner_pos(work, size, "bottom-left", 16), Some((76, 1040 + 40 - 84 - 16)));
        assert_eq!(corner_pos(work, size, "bottom-right", 16), Some((1860 + 60 - 320 - 16, 1040 + 40 - 84 - 16)));
        assert_eq!(corner_pos(work, size, "centre", 16), None);
    }

    #[test]
    fn a_missing_npcap_wins_over_elevation() {
        for elevated in [false, true] {
            assert_eq!(classify(&not_installed(), elevated).id, "npcap_missing");
        }
    }

    #[test]
    fn no_adapter_is_found_through_added_context() {
        assert_eq!(classify(&no_adapter().context("outer"), false).id, "no_adapter");
    }

    #[test]
    fn other_failures_are_not_admin_unless_elevated() {
        let e = anyhow!("pcap_open_live(x): denied (run as Administrator?)");
        assert_eq!(classify(&e, false).id, "not_admin");
        let s = classify(&e, true);
        assert_eq!((s.id, s.detail.as_deref()), ("error", Some("pcap_open_live(x): denied (run as Administrator?)")));
    }

    #[test]
    fn only_a_lost_adapter_or_a_transient_error_is_retried() {
        assert!(retryable("no_adapter") && retryable("error"));
        assert!(!retryable("npcap_missing") && !retryable("not_admin"));
    }

    #[test]
    fn status_json_has_id_and_detail() {
        assert_eq!(Status::new("running").to_json(), json!({"id": "running", "detail": null}));
    }

    #[test]
    fn a_save_failure_is_reported_and_success_clears_its_detail() {
        let e = anyhow!("disk full").context("cannot save state");
        assert_eq!(save_status(Err(&e)).to_json(), json!({
            "id": "save_failed", "detail": "cannot save state: disk full"
        }));
        assert_eq!(save_status(Ok(())).to_json(), Status::new("running").to_json());
    }

    #[test]
    fn a_busy_state_loads_for_display_and_an_unreadable_state_releases_the_lock() {
        let dir = std::env::temp_dir().join(format!("acompanion-app-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("characters.json");
        let saved = json!({"version": 1, "characters": [{
            "server": 7, "name": "Alpha", "class_code": 3, "level": 40,
            "combat_power": null, "item_level": null, "counters": [], "gains": {}, "pets": null,
            "last_seen_us": 1
        }]});
        State::from_json(&saved).unwrap().save(&path).unwrap();
        let (_, guard) = load_state(&path).unwrap();
        let (snapshot, busy) = load_state(&path).unwrap();
        assert!(busy.is_none());
        assert_eq!(snapshot.to_json(), saved);
        drop(guard);
        std::fs::write(&path, "invalid json").unwrap();
        assert!(load_state(&path).is_err());
        assert!(state::try_lock(&path).unwrap().is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid json");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saved_fights_are_read_next_to_the_state_file_and_none_without_a_path() {
        let dir = std::env::temp_dir().join(format!("acompanion-app-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("characters.json");
        assert_eq!(saved_history(Some(&path)), json!([]));
        std::fs::write(dir.join("meter.json"), r#"{"version":1,"fights":[{"start_us":1}]}"#).unwrap();
        assert_eq!(saved_history(Some(&path)), json!([{"start_us": 1}]));
        assert_eq!(saved_history(None), json!([]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_quit_request_sits_next_to_the_state_file() {
        assert_eq!(quit_path(Path::new(r"C:\data\characters.json")), PathBuf::from(r"C:\data\characters.json.quit"));
    }

    #[test]
    fn runas_script_quotes_the_path() {
        assert_eq!(
            runas_script(r"C:\It's here\acompanion.exe"),
            r"Start-Process -FilePath 'C:\It''s here\acompanion.exe' -Verb RunAs -ErrorAction Stop"
        );
    }
}
