//! Live engine: Ethernet frames in, per-character state out. `Engine` has no Npcap in it, so
//! replays and tests drive it directly; `run` feeds it from a live adapter.

use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::net::{Ipv4Addr, UdpSocket};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
#[cfg(windows)]
use anyhow::{Context, bail};
use serde_json::Value;

use crate::decode::Decoder;
use crate::events::Opcodes;
use crate::meter::{Meter, history_path, load_history, save_history};
#[cfg(windows)]
use crate::npcap::{Capture, Npcap};
#[cfg(windows)]
use crate::pcapfile::LINKTYPE_ETHERNET;
use crate::radar::Radar;
use crate::state::{Key, State};
use crate::tcp::parse_ethernet;

/// How long a change may wait before it is written.
/// ponytail: fixed throttle; worst case a kill loses 5 s of changes.
pub const SAVE_EVERY: Duration = Duration::from_secs(5);

/// How long the game port may stay silent before the game counts as gone (the server pings ~20 times a second).
pub const GAME_IDLE: Duration = Duration::from_secs(5);

/// The least time between two radar updates handed to the app.
/// ponytail: fixed rate; a busy zone sends hundreds of moves a second and the map needs ~4 frames a second.
pub const RADAR_EVERY: Duration = Duration::from_millis(250);

/// The least time between two meter updates handed to the app.
/// ponytail: fixed rate; hits arrive in bursts and the overlay needs about four updates a second.
pub const METER_EVERY: Duration = Duration::from_millis(250);

/// Decoder + state + throttled persistence + the transient radar and combat meter.
pub struct Engine {
    decoder: Decoder,
    pub state: State,
    pub radar: Radar,
    pub meter: Meter,
    path: Option<PathBuf>,
    dirty: bool,
    saved_at: Instant,
    radar_dirty: bool,
    radar_at: Instant,
    meter_dirty: bool,
    meter_at: Instant,
    meter_sent: u64,
    /// `meter.json`, next to the state file.
    meter_path: Option<PathBuf>,
    meter_unsaved: bool,
    meter_closed_seen: u64,
    server_port: u16,
    syns: HashMap<(SocketAddrV4, SocketAddrV4), u32>,
    game_at: Option<Instant>,
}

impl Engine {
    /// `path: None` keeps the state in memory only.
    pub fn new(ops: Opcodes, server_port: u16, state: State, path: Option<PathBuf>) -> Self {
        let meter_path = path.as_deref().map(history_path);
        let meter = Meter::with_history(meter_path.as_deref().map(load_history).unwrap_or_default());
        // Loaded fights are handed out with the first update after a restart.
        let loaded = meter.history_json().as_array().is_some_and(|h| !h.is_empty());
        Self {
            decoder: Decoder::new(ops, server_port), state, radar: Radar::default(), path, dirty: false,
            saved_at: Instant::now(), radar_dirty: false, radar_at: Instant::now(), server_port,
            meter, meter_dirty: loaded, meter_at: Instant::now(), meter_sent: if loaded { u64::MAX } else { 0 },
            meter_path, meter_unsaved: false, meter_closed_seen: 0,
            syns: HashMap::new(), game_at: None,
        }
    }

    /// The meter as JSON when it changed and `METER_EVERY` has passed since the last one handed out.
    /// The finished fights ride along only when one closed since the last update.
    pub fn meter_update(&mut self) -> Option<Value> {
        if !self.meter_dirty || self.meter_at.elapsed() < METER_EVERY {
            return None;
        }
        self.meter_at = Instant::now();
        self.meter_dirty = false;
        let mut v = self.meter.to_json();
        if self.meter.closed() != self.meter_sent {
            self.meter_sent = self.meter.closed();
            v["history"] = self.meter.history_json();
        }
        Some(v)
    }

    /// Empty the saved fights: the next update carries `history: []` and the next save writes an empty `meter.json`.
    pub fn clear_meter(&mut self) {
        self.meter.clear_history();
        self.meter_unsaved = true;
        self.dirty = true;
        self.meter_dirty = true;
        self.meter_sent = u64::MAX; // never equals `closed()`, so the update carries the (now empty) history
    }

    /// The radar as JSON when it changed and `RADAR_EVERY` has passed since the last one handed out.
    pub fn radar_update(&mut self) -> Option<Value> {
        if !self.radar_dirty || self.radar_at.elapsed() < RADAR_EVERY {
            return None;
        }
        self.radar_at = Instant::now();
        self.radar_dirty = false;
        Some(self.radar.to_json())
    }

    /// Whether the game port carried traffic within `GAME_IDLE`.
    /// ponytail: any traffic on the game port counts (not the process, no memory reads); a launcher
    /// or another client on that port would also read as the game.
    pub fn game_active(&self) -> bool {
        self.game_at.is_some_and(|t| t.elapsed() < GAME_IDLE)
    }

    /// One captured Ethernet frame; returns the characters it changed (deduplicated).
    /// ponytail: connection tracking survives FIN for late server data; add sequence-aware
    /// eviction if long runs grow the decoder maps, SYN history or retained state bindings.
    pub fn push(&mut self, ts_us: u64, frame: &[u8]) -> Vec<Key> {
        let Some(seg) = parse_ethernet(frame) else { return Vec::new() };
        self.meter_dirty |= self.meter.advance(ts_us);
        let client = if seg.src.port() == self.server_port {
            Some(seg.dst.port())
        } else if seg.dst.port() == self.server_port {
            Some(seg.src.port())
        } else {
            None
        };
        if client.is_some() {
            self.game_at = Some(Instant::now());
        }
        if seg.syn && let Some(port) = client {
            // A repeated SYN belongs to the same connection, even after its identity arrived.
            if self.syns.insert((seg.src, seg.dst), seg.seq) != Some(seg.seq) {
                self.state.end_session(port);
                self.end_radar();
                self.end_meter();
            }
        }
        let mut keys = Vec::new();
        for d in self.decoder.push(ts_us, &seg) {
            self.radar_dirty |= self.radar.apply(&d);
            self.meter_dirty |= self.meter.apply(&d);
            if let Some(key) = self.state.apply(&d)
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        // FIN may precede missing data or only half-close the connection: keep its binding.
        if seg.rst && let Some(port) = client {
            self.state.end_session(port);
            self.end_radar();
            self.end_meter();
        }
        let closed = self.meter.closed();
        if closed != self.meter_closed_seen {
            self.meter_closed_seen = closed;
            self.meter_unsaved = true;
            self.dirty = true; // the same throttle and flush as the characters
        }
        self.dirty |= !keys.is_empty();
        keys
    }

    /// The connection ended: close the fight.
    fn end_meter(&mut self) {
        self.meter_dirty |= self.meter.end_connection();
    }

    /// The connection ended: nothing in view any more.
    fn end_radar(&mut self) {
        self.radar = Radar::default();
        self.radar_dirty = true;
    }

    /// Save if due; `true` means a save completed, `false` means no attempt was made.
    pub fn tick(&mut self) -> Result<bool> {
        if self.dirty && self.saved_at.elapsed() >= SAVE_EVERY {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Save if something changed (shutdown, end of a replay).
    /// ponytail: the open fight is not closed, so quitting mid-fight loses that fight; close it here
    /// (`Meter::end_connection`) if that matters.
    pub fn flush(&mut self) -> Result<()> {
        if self.dirty { self.save() } else { Ok(()) }
    }

    fn save(&mut self) -> Result<()> {
        // a failing save stays dirty and retries after SAVE_EVERY, not on every tick
        self.saved_at = Instant::now();
        if let Some(path) = &self.path {
            self.state.save(path)?;
        }
        if self.meter_unsaved
            && let Some(path) = &self.meter_path
        {
            save_history(path, &self.meter)?;
            self.meter_unsaved = false;
        }
        self.dirty = false;
        Ok(())
    }
}

/// Local IPv4 of the default route. A UDP connect sends nothing; it only selects the route.
#[cfg(windows)]
fn default_route_ip() -> Result<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0")?;
    s.connect("8.8.8.8:80")?;
    match s.local_addr()?.ip() {
        std::net::IpAddr::V4(ip) => Ok(ip),
        other => bail!("default route is not IPv4: {other}"),
    }
}

/// No Npcap adapter owns the default route's address; `app::classify` finds it with `downcast_ref`.
#[cfg(windows)]
#[derive(Debug)]
pub struct NoAdapter(pub Ipv4Addr);

#[cfg(windows)]
impl std::fmt::Display for NoAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no adapter owns {}; pass --device", self.0)
    }
}

/// Name of the adapter that owns the default route's address.
/// ponytail: the route to 8.8.8.8, not to the game server; pass a device explicitly on a
/// multi-homed machine until the adapter is chosen by the route to the server.
#[cfg(windows)]
pub fn default_device(npcap: &Npcap) -> Result<String> {
    let ip = default_route_ip()?;
    npcap
        .devices()?
        .into_iter()
        .find(|d| d.ipv4.contains(&ip))
        .map(|d| d.name)
        .with_context(|| NoAdapter(ip))
}

/// Capture on `device` (default: `default_device`) until `stop` is set, feeding `engine`.
/// `on_start` runs once the adapter is open; `on_change` gets the state and the keys each frame
/// changed. `on_save` reports each periodic write attempt, including successful recovery.
/// `on_game` gets `Engine::game_active` each time it flips (the game starts as "not detected").
/// `on_radar` gets `Engine::radar_update` whenever the radar changed (at most every `RADAR_EVERY`).
/// `on_meter` gets `Engine::meter_update` whenever the meter changed (at most every `METER_EVERY`).
/// `clear` is raised by the app to empty the saved fights; the loop lowers it and calls `Engine::clear_meter`
/// (the engine belongs to this thread).
/// A failed save is logged and retried; whatever ends the loop, the engine is flushed before
/// returning (the loop's error wins over the flush's).
// ponytail: six callbacks as separate arguments; fold them into one observer trait if another appears.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub fn run(
    device: Option<&str>,
    filter: &str,
    engine: &mut Engine,
    stop: &AtomicBool,
    clear: &AtomicBool,
    on_start: impl FnOnce(),
    mut on_change: impl FnMut(&State, &[Key]),
    mut on_save: impl FnMut(Result<(), &anyhow::Error>),
    mut on_game: impl FnMut(bool),
    mut on_radar: impl FnMut(&Value),
    mut on_meter: impl FnMut(&Value),
) -> Result<()> {
    // Capture is !Send and borrows Npcap: both live and die in the thread that runs the loop
    let npcap = Npcap::load()?;
    let device = match device {
        Some(d) => d.to_string(),
        None => default_device(&npcap)?,
    };
    let mut cap = npcap.open(&device, filter)?;
    if cap.linktype() != LINKTYPE_ETHERNET as i32 {
        bail!("adapter link type {} is not Ethernet; pick another with --device", cap.linktype());
    }
    on_start();
    let looped =
        capture_loop(&mut cap, engine, stop, clear, &mut on_change, &mut on_save, &mut on_game, &mut on_radar, &mut on_meter);
    let flushed = engine.flush();
    looped.and(flushed)
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn capture_loop(
    cap: &mut Capture<'_>,
    engine: &mut Engine,
    stop: &AtomicBool,
    clear: &AtomicBool,
    on_change: &mut impl FnMut(&State, &[Key]),
    on_save: &mut impl FnMut(Result<(), &anyhow::Error>),
    on_game: &mut impl FnMut(bool),
    on_radar: &mut impl FnMut(&Value),
    on_meter: &mut impl FnMut(&Value),
) -> Result<()> {
    let mut game = false;
    while !stop.load(Ordering::Relaxed) {
        if clear.swap(false, Ordering::Relaxed) {
            engine.clear_meter();
        }
        // `None` is the 100 ms read timeout: the loop's tick
        if let Some((ts_us, frame)) = cap.next_packet()? {
            let keys = engine.push(ts_us, &frame);
            if !keys.is_empty() {
                on_change(&engine.state, &keys);
            }
        }
        tick_and_report(engine, on_save);
        report_game(engine, &mut game, on_game);
        if let Some(radar) = engine.radar_update() {
            on_radar(&radar);
        }
        if let Some(meter) = engine.meter_update() {
            on_meter(&meter);
        }
    }
    Ok(())
}

/// Calls `on_game` only when the engine's view of the game differs from the last one reported.
#[cfg(any(windows, test))]
fn report_game(engine: &Engine, reported: &mut bool, on_game: &mut impl FnMut(bool)) {
    let now = engine.game_active();
    if now != *reported {
        *reported = now;
        on_game(now);
    }
}

#[cfg(any(windows, test))]
fn tick_and_report(engine: &mut Engine, on_save: &mut impl FnMut(Result<(), &anyhow::Error>)) {
    match engine.tick() {
        Ok(true) => on_save(Ok(())),
        Ok(false) => {},
        Err(e) => {
            eprintln!("cannot save state (will retry): {e:#}");
            on_save(Err(&e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const PORT: u16 = 13328;
    const CLIENT_PORT: u16 = 50000;

    /// A TCP segment from the server (`true`) or the client, as an Ethernet frame.
    fn eth(from_server: bool, seq: u32, syn: bool, fin: bool, payload: &[u8]) -> Vec<u8> {
        eth_flags(from_server, seq, syn, fin, false, payload)
    }

    fn eth_flags(from_server: bool, seq: u32, syn: bool, fin: bool, rst: bool, payload: &[u8]) -> Vec<u8> {
        let (src, dst) = if from_server { (PORT, CLIENT_PORT) } else { (CLIENT_PORT, PORT) };
        let (a, b) = if from_server { ([10, 0, 0, 2], [10, 0, 0, 1]) } else { ([10, 0, 0, 1], [10, 0, 0, 2]) };
        let mut builder = etherparse::PacketBuilder::ethernet2([0; 6], [1; 6]).ipv4(a, b, 64).tcp(src, dst, seq, 65535);
        if syn {
            builder = builder.syn();
        }
        if rst {
            builder = builder.rst();
        }
        if fin {
            builder = builder.fin();
        }
        let mut out = Vec::new();
        builder.write(&mut out, payload).unwrap();
        out
    }

    /// `varint(len + 4) body`; the test bodies are short enough for a one-byte varint.
    fn frame(body: &[u8]) -> Vec<u8> {
        let mut f = vec![body.len() as u8 + 4];
        f.extend_from_slice(body);
        f
    }

    fn identity(name: &str, server: u16, level: u32) -> Vec<u8> {
        let mut b = vec![0x33, 0x36];
        b.extend([0u8; 7]);
        b.push(name.len() as u8);
        b.extend(name.as_bytes());
        b.extend(server.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        b.push(0);
        b.extend(level.to_le_bytes());
        b.extend([0u8; 8]); // item level pair
        frame(&b)
    }

    fn counter(id: u32, value: u8) -> Vec<u8> {
        let mut b = vec![0x0c, 0x61, 0x00, 0x0c];
        b.extend(id.to_le_bytes());
        b.extend([value, 0x0c, 0x03]);
        frame(&b)
    }

    fn engine(path: Option<PathBuf>) -> Engine {
        Engine::new(Opcodes::embedded("default").unwrap(), PORT, State::default(), path)
    }

    /// Engine with one bound session; returns it and the next server sequence number. The
    /// identity trails its data on the wire, so the counter is held until the identity arrives;
    /// a second counter then applies at once (value 3).
    fn bound() -> (Engine, u32) {
        let mut e = engine(None);
        let alpha = vec![(7, "Alpha".to_string())];
        assert!(e.push(0, &eth(false, 499, true, false, &[])).is_empty());
        assert!(e.push(1, &eth(true, 99, true, false, &[])).is_empty());
        let data = [counter(5, 4), identity("Alpha", 7, 40)].concat();
        assert_eq!(e.push(2, &eth(true, 100, false, false, &data)), alpha);
        let next = 100 + data.len() as u32;
        let live = counter(5, 3);
        assert_eq!(e.push(3, &eth(true, next, false, false, &live)), alpha);
        (e, next + live.len() as u32)
    }

    fn temp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("acompanion-live-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn identity_then_counter_updates_the_state_and_reports_one_key() {
        let (e, _) = bound();
        let c = e.state.get(&(7, "Alpha".to_string())).unwrap();
        assert_eq!((c.level, c.counters[&5].value), (40, 3));
    }

    #[test]
    fn a_client_fin_preserves_the_session_for_server_updates() {
        let (mut e, next) = bound();
        assert!(e.push(4, &eth(false, 500, false, true, &[])).is_empty());
        assert_eq!(e.push(5, &eth(true, next, false, false, &counter(5, 1))), vec![(7, "Alpha".to_string())]);
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 1);
    }

    #[test]
    fn a_server_fin_before_a_missing_counter_preserves_the_session() {
        let (mut e, next) = bound();
        let update = counter(5, 1);
        assert!(e.push(4, &eth(true, next + update.len() as u32, false, true, &[])).is_empty());
        assert_eq!(e.push(5, &eth(true, next, false, false, &update)), vec![(7, "Alpha".to_string())]);
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 1);
    }

    #[test]
    fn a_fin_with_data_behind_a_gap_applies_after_the_missing_counter() {
        let (mut e, next) = bound();
        let missing = counter(5, 1);
        assert!(e.push(4, &eth(true, next + missing.len() as u32, false, true, &counter(5, 0))).is_empty());
        assert_eq!(e.push(5, &eth(true, next, false, false, &missing)), vec![(7, "Alpha".to_string())]);
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 0);
    }

    #[test]
    fn an_in_order_fin_applies_its_counter() {
        let (mut e, next) = bound();
        assert_eq!(e.push(4, &eth(true, next, false, true, &counter(5, 1))), vec![(7, "Alpha".to_string())]);
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 1);
    }

    #[test]
    fn retransmitted_syns_preserve_the_session() {
        for (from_server, seq) in [(true, 99), (false, 499)] {
            let (mut e, next) = bound();
            assert!(e.push(4, &eth(from_server, seq, true, false, &[])).is_empty());
            assert_eq!(e.push(5, &eth(true, next, false, false, &counter(5, 1))), vec![(7, "Alpha".to_string())]);
            assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 1);
        }
    }

    #[test]
    fn a_new_server_syn_rebinds_without_changing_the_previous_character() {
        let (mut e, _) = bound();
        assert!(e.push(4, &eth(true, 999, true, false, &[])).is_empty());
        let data = [counter(5, 9), identity("Beta", 7, 20)].concat();
        assert_eq!(e.push(5, &eth(true, 1000, false, false, &data)), vec![(7, "Beta".to_string())]);
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 3);
        assert_eq!(e.state.get(&(7, "Beta".to_string())).unwrap().counters[&5].value, 9);
    }

    #[test]
    fn a_syn_on_a_reused_client_port_unbinds_the_session() {
        let (mut e, next) = bound();
        e.push(3, &eth(false, 900, true, false, &[]));
        assert!(e.push(4, &eth(true, next, false, false, &counter(5, 1))).is_empty());
    }

    #[test]
    fn a_client_rst_unbinds_the_session() {
        let (mut e, next) = bound();
        assert!(e.push(3, &eth_flags(false, 500, false, false, true, &[])).is_empty());
        assert!(e.push(4, &eth(true, next, false, false, &counter(5, 1))).is_empty());
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 3);
    }

    #[test]
    fn client_to_server_frames_are_ignored() {
        let mut e = engine(None);
        let data = [identity("Alpha", 7, 40), counter(5, 4)].concat();
        assert!(e.push(1, &eth(false, 500, false, false, &data)).is_empty());
        assert!(e.state.get(&(7, "Alpha".to_string())).is_none());
        assert_eq!(e.state.dropped, 0);
    }

    /// A `41 36` monster spawn: entity 5, a template, a position in centimetres.
    fn spawn(template: u32, x: f32, y: f32) -> Vec<u8> {
        let mut b = vec![0x41, 0x36, 0x05, 0x04, 0x20, 0x00];
        b.extend(template.to_le_bytes());
        b.extend([0x00, 0x02]);
        for v in [x, y, 1.0] {
            b.extend(v.to_le_bytes());
        }
        frame(&b)
    }

    #[test]
    fn radar_updates_are_throttled_and_a_new_connection_clears_the_radar() {
        let (mut e, next) = bound();
        let monsters = |e: &mut Engine| {
            e.radar_at = Instant::now().checked_sub(RADAR_EVERY).unwrap(); // the throttle has passed
            e.radar_update().map(|r| r["monsters"].clone())
        };
        let data = spawn(2_100_025, 10_000.0, -20_000.0);
        e.push(4, &eth(true, next, false, false, &data));
        assert!(e.radar_update().is_none()); // changed, but handed out too recently
        assert_eq!(monsters(&mut e), Some(serde_json::json!([{"entity": 5, "template": 2_100_025, "x": 10_000, "y": -20_000}])));
        assert_eq!(monsters(&mut e), None); // nothing changed since
        // the character is untouched by radar traffic
        assert_eq!(e.state.get(&(7, "Alpha".to_string())).unwrap().counters[&5].value, 3);
        e.push(5, &eth(true, 999, true, false, &[])); // a new connection from the server
        assert_eq!(monsters(&mut e), Some(serde_json::json!([])));
    }

    /// A `04 38` hit by entity 0 (the test identity's own entity) on entity 5, `amount` below 128.
    fn hit(amount: u8) -> Vec<u8> {
        let mut b = vec![0x04, 0x38, 0x05, 0x04, 0x00, 0x00];
        b.extend(14_090_130u32.to_le_bytes());
        b.extend([0x40, 0x02]);
        b.extend(1_400_000_000u32.to_le_bytes());
        b.extend([1, 0, 0, 0, 0xca, 0x61, amount, 0x01, 0x00]);
        frame(&b)
    }

    #[test]
    fn meter_updates_are_throttled_and_a_new_connection_closes_the_fight() {
        let (mut e, next) = bound();
        let update = |e: &mut Engine| {
            e.meter_at = Instant::now().checked_sub(METER_EVERY).unwrap(); // the throttle has passed
            e.meter_update()
        };
        e.push(4, &eth(true, next, false, false, &hit(100)));
        assert!(e.meter_update().is_none()); // changed, but handed out too recently
        let first = update(&mut e).unwrap();
        assert_eq!(first["current"]["players"][0]["dealt"]["amount"], 100);
        assert!(first.get("history").is_none()); // no fight has closed yet
        assert!(update(&mut e).is_none()); // nothing changed since
        e.push(5, &eth(true, 999, true, false, &[])); // a new connection from the server
        let after = update(&mut e).unwrap();
        assert_eq!(after["current"], Value::Null);
        assert_eq!(after["history"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn clearing_the_meter_sends_and_saves_an_empty_history() {
        let dir = temp_dir();
        let path = dir.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        e.push(3, &eth(true, 100 + identity("Alpha", 7, 40).len() as u32, false, false, &hit(100)));
        e.push(4, &eth(true, 1, true, false, &[])); // a new connection closes the fight
        e.flush().unwrap();
        e.meter_at = Instant::now().checked_sub(METER_EVERY).unwrap();
        assert_eq!(e.meter_update().unwrap()["history"].as_array().unwrap().len(), 1);
        e.clear_meter();
        e.meter_at = Instant::now().checked_sub(METER_EVERY).unwrap();
        let after = e.meter_update().unwrap();
        assert_eq!(after["history"], serde_json::json!([]));
        assert_eq!(after["fights"], 0);
        e.flush().unwrap();
        assert!(engine(Some(path)).meter.history_json().as_array().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_closed_fight_is_saved_with_the_state_and_reloaded() {
        let dir = temp_dir();
        let path = dir.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        e.push(3, &eth(true, 100 + identity("Alpha", 7, 40).len() as u32, false, false, &hit(100)));
        e.push(4, &eth(true, 1, true, false, &[])); // a new connection closes the fight
        e.flush().unwrap();
        assert!(dir.join("meter.json").exists());
        let mut again = engine(Some(path));
        assert_eq!(again.meter.history_json().as_array().unwrap().len(), 1);
        // the first update after the restart hands the loaded fights out, with no fight closed yet
        again.meter_at = Instant::now().checked_sub(METER_EVERY).unwrap();
        let first = again.meter_update().unwrap();
        assert_eq!(first["history"].as_array().unwrap().len(), 1);
        assert_eq!(first["fights"], 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn flush_writes_only_when_dirty() {
        let dir = temp_dir();
        let path = dir.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.flush().unwrap();
        assert!(!path.exists());
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        e.flush().unwrap();
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
        e.flush().unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tick_waits_for_the_save_interval() {
        let dir = temp_dir();
        let path = dir.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        assert!(!e.tick().unwrap());
        assert!(!path.exists());
        e.saved_at = Instant::now().checked_sub(SAVE_EVERY).unwrap();
        assert!(e.tick().unwrap());
        assert!(path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_failed_save_stays_dirty_and_is_retried_after_the_interval() {
        let dir = temp_dir();
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"a file where a directory is needed").unwrap();
        let path = blocker.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        e.saved_at = Instant::now().checked_sub(SAVE_EVERY).unwrap();
        assert!(e.tick().is_err());
        assert!(!e.tick().unwrap()); // throttled again: the next try waits a whole interval
        assert!(e.flush().is_err()); // still dirty
        std::fs::remove_file(&blocker).unwrap();
        e.flush().unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_game_is_active_while_its_port_keeps_talking_and_reported_once_per_flip() {
        let mut e = engine(None);
        let mut flips = Vec::new();
        let mut reported = false;
        report_game(&e, &mut reported, &mut |g| flips.push(g));
        assert!(!e.game_active());
        assert!(flips.is_empty()); // starts as "not detected": nothing to report
        e.push(1, &eth(true, 99, true, false, &[]));
        report_game(&e, &mut reported, &mut |g| flips.push(g));
        report_game(&e, &mut reported, &mut |g| flips.push(g));
        assert!(e.game_active());
        assert_eq!(flips, [true]);
        e.game_at = Instant::now().checked_sub(GAME_IDLE);
        report_game(&e, &mut reported, &mut |g| flips.push(g));
        assert!(!e.game_active());
        assert_eq!(flips, [true, false]);
    }

    #[test]
    fn traffic_on_other_ports_is_not_the_game() {
        let mut e = engine(None);
        let other = etherparse::PacketBuilder::ethernet2([0; 6], [1; 6])
            .ipv4([10, 0, 0, 2], [10, 0, 0, 1], 64)
            .tcp(443, 50000, 1, 65535);
        let mut frame = Vec::new();
        other.write(&mut frame, &[]).unwrap();
        e.push(1, &frame);
        assert!(!e.game_active());
    }

    #[test]
    fn save_notifications_keep_the_failure_until_a_write_really_succeeds() {
        let dir = temp_dir();
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"a file where a directory is needed").unwrap();
        let path = blocker.join("characters.json");
        let mut e = engine(Some(path.clone()));
        e.push(1, &eth(true, 99, true, false, &[]));
        e.push(2, &eth(true, 100, false, false, &identity("Alpha", 7, 40)));
        let mut notifications = Vec::new();
        tick_and_report(&mut e, &mut |r| notifications.push(r.is_ok()));
        assert!(notifications.is_empty());
        e.saved_at = Instant::now().checked_sub(SAVE_EVERY).unwrap();
        tick_and_report(&mut e, &mut |r| notifications.push(r.is_ok()));
        assert!(e.dirty);
        assert_eq!(notifications, [false]);
        tick_and_report(&mut e, &mut |r| notifications.push(r.is_ok()));
        assert_eq!(notifications, [false]); // skipped attempts cannot clear the warning
        std::fs::remove_file(&blocker).unwrap();
        e.saved_at = Instant::now().checked_sub(SAVE_EVERY).unwrap();
        tick_and_report(&mut e, &mut |r| notifications.push(r.is_ok()));
        assert_eq!(notifications, [false, true]);
        assert!(!e.dirty);
        assert_eq!(State::load(&path).unwrap().to_json(), e.state.to_json());
        tick_and_report(&mut e, &mut |r| notifications.push(r.is_ok()));
        assert_eq!(notifications, [false, true]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
