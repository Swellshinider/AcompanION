use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::net::SocketAddrV4;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use acompanion::decode;
use acompanion::events::Opcodes;
use acompanion::live::Engine;
use acompanion::hexdump::{entropy, hexdump};
use acompanion::pcapfile::{LINKTYPE_ETHERNET, Reader, Writer};
use acompanion::state::{Key, State};
use acompanion::tcp::{Streams, parse_ethernet};
use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(about = "AcompanION protocol reverse-engineering tool")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List Npcap adapters and their IPv4 addresses
    Devices,
    /// Capture game traffic to <OUT>.pcap
    Record {
        /// Output prefix, e.g. captures/pets-1
        out: PathBuf,
        /// Adapter name from `re devices` (default: the adapter with the default route)
        #[arg(long)]
        device: Option<String>,
        #[arg(long, default_value = "tcp port 13328")]
        filter: String,
        #[arg(long, default_value_t = 120)]
        seconds: u64,
    },
    /// Reassemble a pcap and print each direction's byte stream
    Dump {
        pcap: PathBuf,
        /// Per-direction totals, entropy and stream head instead of every chunk
        #[arg(long)]
        summary: bool,
        #[arg(long, default_value_t = 256)]
        max_bytes: usize,
    },
    /// Decode a pcap and print one JSON object per event (ids and raw values). Output contains
    /// account data (character names): do not paste it anywhere.
    Decode {
        pcap: PathBuf,
        #[arg(long, default_value_t = 13328)]
        port: u16,
    },
    /// Follow live traffic (or replay a pcap) and keep per-character state, one JSON line per
    /// changed character. Output contains account data (character names): do not paste it anywhere.
    Live {
        /// Adapter name from `re devices` (default: the adapter with the default route)
        #[arg(long)]
        device: Option<String>,
        /// Capture filter (default: tcp port <PORT>)
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = 13328)]
        port: u16,
        /// State file (default: %APPDATA%\AcompanION\characters.json; a replay writes only if given)
        #[arg(long)]
        state: Option<PathBuf>,
        /// Stop after this many seconds (default: press Enter)
        #[arg(long)]
        seconds: Option<u64>,
        /// Read this pcap instead of capturing
        #[arg(long)]
        replay: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Devices => devices(),
        Cmd::Record { out, device, filter, seconds } => record(out, device, filter, seconds),
        Cmd::Dump { pcap, summary, max_bytes } => dump(pcap, summary, max_bytes),
        Cmd::Decode { pcap, port } => decode(pcap, port),
        Cmd::Live { device, filter, port, state, seconds, replay } => {
            live(device, filter, port, state, seconds, replay)
        }
    }
}

fn devices() -> Result<()> {
    let npcap = acompanion::npcap::Npcap::load()?;
    for (i, d) in npcap.devices()?.iter().enumerate() {
        let ips: Vec<String> = d.ipv4.iter().map(|a| a.to_string()).collect();
        println!("{i:2}  {}  [{}]  {}", d.name, ips.join(", "), d.description);
    }
    Ok(())
}

/// `<prefix><suffix>`, appended so dots inside the prefix are never treated as an extension.
fn with_suffix(prefix: &Path, suffix: &str) -> PathBuf {
    let mut s = prefix.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Create a new file; an existing one is an error, never truncated (captures are irreplaceable).
fn create_new(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("cannot create {} (already exists? choose another prefix)", path.display()))
}

fn record(out: PathBuf, device: Option<String>, filter: String, seconds: u64) -> Result<()> {
    let npcap = acompanion::npcap::Npcap::load()?;
    let device = match device {
        Some(d) => d,
        None => acompanion::live::default_device(&npcap)?,
    };
    let mut cap = npcap.open(&device, &filter)?;
    if cap.linktype() != LINKTYPE_ETHERNET as i32 {
        bail!("adapter link type {} is not Ethernet; pick another with --device", cap.linktype());
    }

    let pcap_path = with_suffix(&out, ".pcap");
    if let Some(dir) = pcap_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let pcap_file = create_new(&pcap_path)?;

    let mut w = Writer::new(BufWriter::new(pcap_file), LINKTYPE_ETHERNET)?;
    eprintln!("capturing {filter:?} on {device} for {seconds}s -> {}", pcap_path.display());
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut packets = 0u64;
    while Instant::now() < deadline {
        match cap.next_packet()? {
            Some((ts, frame)) => {
                w.write(ts, &frame)?;
                packets += 1;
            }
            None => w.flush()?,
        }
    }
    w.flush()?;
    eprintln!("done: {packets} packets");
    Ok(())
}

/// Build label of the embedded opcode table (see `opcodes.json`).
const BUILD: &str = "default";

fn decode(pcap: PathBuf, port: u16) -> Result<()> {
    let (events, stats) = decode::decode_file(&pcap, Opcodes::embedded(BUILD)?, port)?;
    for d in &events {
        println!("{}", json!({"t_us": d.ts_us, "conn": d.conn, "event": d.event.to_json()}));
    }
    eprintln!(
        "{} events, {} frames, {} containers ({} failed to inflate)",
        events.len(),
        stats.frames,
        stats.containers,
        stats.bad_containers
    );
    Ok(())
}

fn print_changes(state: &State, keys: &[Key]) {
    for key in keys {
        if let Some(c) = state.get(key) {
            println!("{}", c.to_json());
        }
    }
}

fn replay_pcap(pcap: &Path, engine: &mut Engine) -> Result<()> {
    let file = File::open(pcap).with_context(|| format!("cannot open {}", pcap.display()))?;
    let reader = Reader::new(BufReader::new(file))?;
    if reader.linktype != LINKTYPE_ETHERNET {
        bail!("only Ethernet captures are supported (link type {})", reader.linktype);
    }
    for rec in reader {
        let rec = rec?;
        let keys = engine.push(rec.ts_us, &rec.data);
        print_changes(&engine.state, &keys);
    }
    engine.flush()
}

fn live(
    device: Option<String>,
    filter: Option<String>,
    port: u16,
    state: Option<PathBuf>,
    seconds: Option<u64>,
    replay: Option<PathBuf>,
) -> Result<()> {
    // a replay writes only to an explicit --state: never into the live state file by accident
    let path = match (&replay, state) {
        (_, Some(p)) => Some(p),
        (Some(_), None) => None,
        (None, None) => Some(acompanion::state::default_path().context("APPDATA is not set; pass --state")?),
    };
    let _guard = match &path {
        Some(p) => Some(acompanion::state::try_lock(p)?.context("state file is already in use by another writer")?),
        None => None,
    };
    let loaded = match &path {
        Some(p) => State::load(p)?,
        None => State::default(),
    };
    let mut engine = Engine::new(Opcodes::embedded(BUILD)?, port, loaded, path);
    match replay {
        Some(pcap) => replay_pcap(&pcap, &mut engine)?,
        None => {
            let stop = Arc::new(AtomicBool::new(false));
            let on_enter = stop.clone();
            std::thread::spawn(move || {
                // EOF (stdin closed) is not a request to stop
                if matches!(std::io::stdin().read_line(&mut String::new()), Ok(n) if n > 0) {
                    on_enter.store(true, Ordering::Relaxed);
                }
            });
            if let Some(s) = seconds {
                let on_timer = stop.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(s));
                    on_timer.store(true, Ordering::Relaxed);
                });
            }
            let filter = filter.unwrap_or_else(|| format!("tcp port {port}"));
            eprintln!("following {filter:?}; press Enter to stop");
            acompanion::live::run(device.as_deref(), &filter, &mut engine, &stop, &AtomicBool::new(false), &std::sync::Mutex::new(Vec::new()), || {}, print_changes, |_| {}, |_| {}, |_| {}, |_| {})?;
        }
    }
    eprintln!("{} events dropped before their connection reported an identity", engine.state.dropped);
    Ok(())
}

fn dump(path: PathBuf, summary: bool, max_bytes: usize) -> Result<()> {
    let reader = Reader::new(BufReader::new(File::open(&path)?))?;
    if reader.linktype != LINKTYPE_ETHERNET {
        bail!("only Ethernet captures are supported (link type {})", reader.linktype);
    }
    let mut streams = Streams::default();
    let mut per_dir: BTreeMap<(SocketAddrV4, SocketAddrV4), Vec<u8>> = BTreeMap::new();
    let mut t0 = None;
    for rec in reader {
        let rec = rec?;
        let first = t0.is_none();
        let t0 = *t0.get_or_insert(rec.ts_us);
        if first && !summary {
            println!("# t0_us={t0}");
        }
        let Some(seg) = parse_ethernet(&rec.data) else { continue };
        let (src, dst) = (seg.src, seg.dst);
        let Some(bytes) = streams.push(&seg) else { continue };
        if summary {
            per_dir.entry((src, dst)).or_default().extend_from_slice(&bytes);
        } else {
            let t = (rec.ts_us as i64 - t0 as i64) as f64 / 1e6;
            println!("{t:10.3} ({})  {src} -> {dst}  {} bytes", rec.ts_us, bytes.len());
            print!("{}", hexdump(&bytes[..bytes.len().min(max_bytes)]));
        }
    }
    for ((src, dst), bytes) in per_dir {
        let head = &bytes[..bytes.len().min(4096)];
        println!("{src} -> {dst}: {} bytes, entropy(first {}) = {:.3}", bytes.len(), head.len(), entropy(head));
        print!("{}", hexdump(&bytes[..bytes.len().min(64)]));
    }
    for ((src, dst), n) in streams.gaps() {
        println!("WARNING: {src} -> {dst}: {n} bytes undelivered behind a gap (lost or unparsed packets)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn suffix_is_appended_not_substituted() {
        assert_eq!(with_suffix(Path::new("captures/pets-1.5"), ".pcap"), PathBuf::from("captures/pets-1.5.pcap"));
        assert_eq!(with_suffix(Path::new("captures/pets-1"), ".pcap"), PathBuf::from("captures/pets-1.pcap"));
        assert_ne!(with_suffix(Path::new("x.5"), ".pcap"), with_suffix(Path::new("x.6"), ".pcap"));
    }

    fn tcp_frame(from_server: bool, seq: u32, syn: bool, payload: &[u8]) -> Vec<u8> {
        let (src, dst) = if from_server { (13328, 50000) } else { (50000, 13328) };
        let mut builder = etherparse::PacketBuilder::ethernet2([0; 6], [1; 6])
            .ipv4([10, 0, 0, 2], [10, 0, 0, 1], 64)
            .tcp(src, dst, seq, 65535);
        if syn {
            builder = builder.syn();
        }
        let mut out = Vec::new();
        builder.write(&mut out, payload).unwrap();
        out
    }

    /// A pcap of one connection: SYN, then one counter update and the identity that trails it (invented values).
    fn synthetic_pcap(path: &Path) {
        let mut ident = vec![0x33, 0x36];
        ident.extend([0u8; 7]);
        ident.push(5);
        ident.extend(b"Alpha");
        ident.extend(7u16.to_le_bytes());
        ident.extend(3u32.to_le_bytes());
        ident.push(0);
        ident.extend(40u32.to_le_bytes());
        ident.extend([0u8; 8]); // item level pair
        let counter = [0x0c, 0x61, 0x00, 0x0c, 5, 0, 0, 0, 4, 0x0c, 0x03];
        let mut data = Vec::new();
        for body in [&counter[..], &ident[..]] { // the identity trails the data it claims
            data.push(body.len() as u8 + 4);
            data.extend_from_slice(body);
        }
        let mut w = Writer::new(File::create(path).unwrap(), LINKTYPE_ETHERNET).unwrap();
        w.write(1_000_000, &tcp_frame(true, 99, true, &[])).unwrap();
        w.write(2_000_000, &tcp_frame(true, 100, false, &data)).unwrap();
    }

    #[test]
    fn replay_persists_only_when_a_state_path_is_given() {
        let dir = std::env::temp_dir().join(format!(
            "acompanion-re-live-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let pcap = dir.join("synthetic.pcap");
        synthetic_pcap(&pcap);

        // a replay without --state must not touch the live state file either
        let default = acompanion::state::default_path();
        let stamp = || default.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| (m.len(), m.modified().ok()));
        let before = stamp();
        live(None, None, 13328, None, None, Some(pcap.clone())).unwrap();
        assert_eq!(stamp(), before);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1); // only the pcap

        let state = dir.join("characters.json");
        let guard = acompanion::state::try_lock(&state).unwrap().unwrap();
        for replay in [None, Some(pcap.clone())] {
            let error = live(None, None, 13328, Some(state.clone()), None, replay).unwrap_err();
            assert!(error.to_string().contains("already in use"));
            assert!(!state.exists());
        }
        drop(guard);
        live(None, None, 13328, Some(state.clone()), None, Some(pcap)).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
        let chars = json["characters"].as_array().unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!((chars[0]["name"].as_str(), chars[0]["server"].as_u64(), chars[0]["level"].as_u64()), (Some("Alpha"), Some(7), Some(40)));
        assert_eq!(chars[0]["counters"][0]["id"], 5);
        assert_eq!(chars[0]["counters"][0]["value"], 4);
        assert!(acompanion::state::try_lock(&state).unwrap().is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
