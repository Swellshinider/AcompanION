//! Server → client framing: varint length, `ff ff` LZ4 containers, resync on the server ping.
//! Layout and evidence: docs/protocol.md ("Frame layout", "Body").

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddrV4;
use std::path::Path;

use anyhow::{Result, bail};

use crate::events::{Event, Opcodes};
use crate::pcapfile::{LINKTYPE_ETHERNET, Reader};
use crate::tcp::{Segment, Streams, parse_ethernet};

/// Frame header + opcode of the server ping (10-byte body, every ~50 ms): a frame boundary anchor.
const PING: [u8; 3] = [0x0e, 0x00, 0x36];
/// Consecutive well-formed frames required before a ping candidate is trusted as a boundary.
const CHAIN: usize = 4;
/// Sanity ceilings against corrupt length fields.
/// ponytail: raise if a real frame or container exceeds them.
const MAX_FRAME: usize = 4 << 20;
const MAX_INFLATED: usize = 16 << 20;
/// Allow every supported frame in a candidate chain to finish before abandoning it.
const SYNC_WINDOW: usize = CHAIN * MAX_FRAME;

enum Step {
    /// A whole frame: its body is `buf[body..next]`.
    Frame { body: usize, next: usize },
    /// The frame is not fully buffered yet.
    Partial,
    /// Not a frame boundary.
    Bad,
}

/// Parse the frame header at `buf[i]`: `varint(V)`, total length = len(varint) + V - 4.
fn step(buf: &[u8], i: usize) -> Step {
    let (mut v, mut n) = (0usize, 0usize);
    loop {
        let Some(&c) = buf.get(i + n) else { return Step::Partial };
        v |= usize::from(c & 0x7f) << (7 * n);
        n += 1;
        if c & 0x80 == 0 {
            break;
        }
        if n == 4 {
            return Step::Bad;
        }
    }
    if v < 4 {
        return Step::Bad;
    }
    let total = n + v - 4;
    if total > MAX_FRAME {
        return Step::Bad;
    }
    if i + total > buf.len() {
        return Step::Partial;
    }
    Step::Frame { body: i + n, next: i + total }
}

enum Anchor {
    /// A frame boundary: drop everything before it.
    Found(usize),
    /// A ping candidate that is not decided yet: keep buffering from here.
    Wait(usize),
    /// No candidate in the buffer.
    Absent,
}

/// First server ping from which `CHAIN` frames chain. Port of `sync()` in `tools/re/aion_re.py`.
fn find_sync(buf: &[u8]) -> Anchor {
    let mut from = 0;
    while let Some(off) = buf[from..].windows(PING.len()).position(|w| w == PING) {
        let i = from + off;
        let (mut pos, mut frames) = (i, 0);
        loop {
            if frames == CHAIN {
                return Anchor::Found(i);
            }
            match step(buf, pos) {
                Step::Frame { next, .. } => {
                    pos = next;
                    frames += 1;
                }
                Step::Partial if buf.len() - i <= SYNC_WINDOW => return Anchor::Wait(i),
                Step::Partial | Step::Bad => break,
            }
        }
        from = i + 1;
    }
    Anchor::Absent
}

/// Splits a reassembled server → client byte stream into frame bodies.
pub struct Framer {
    buf: Vec<u8>,
    synced: bool,
}

impl Framer {
    /// `synced`: the stream is known to start on a frame boundary (its SYN was seen).
    pub fn new(synced: bool) -> Self {
        Self { buf: Vec::new(), synced }
    }

    /// Bytes were lost: drop the partial frame and look for the next boundary.
    pub fn lose_sync(&mut self) {
        self.buf.clear();
        self.synced = false;
    }

    /// Feed reassembled bytes; returns every complete frame body (opcode first).
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            if !self.synced {
                match find_sync(&self.buf) {
                    Anchor::Found(i) => {
                        self.buf.drain(..i);
                        self.synced = true;
                    }
                    Anchor::Wait(i) => {
                        self.buf.drain(..i);
                        return out;
                    }
                    Anchor::Absent => {
                        // a ping may straddle the next push
                        let cut = self.buf.len().saturating_sub(PING.len() - 1);
                        self.buf.drain(..cut);
                        return out;
                    }
                }
            }
            let mut pos = 0;
            let bad = loop {
                match step(&self.buf, pos) {
                    Step::Frame { body, next } => {
                        out.push(self.buf[body..next].to_vec());
                        pos = next;
                    }
                    Step::Partial => break false,
                    Step::Bad => break true,
                }
            };
            if !bad {
                self.buf.drain(..pos);
                return out;
            }
            // corrupt length: step past what parsed and search for the next boundary
            self.buf.drain(..pos + 1);
            self.synced = false;
        }
    }
}

/// Decompress one LZ4 block (no frame header) that must inflate to exactly `size` bytes.
pub fn lz4_block(src: &[u8], size: usize) -> Option<Vec<u8>> {
    // LZ4 cannot expand more than ~255:1, so a larger claim is corrupt; refuse before allocating
    if size > MAX_INFLATED || size > src.len().saturating_mul(255) {
        return None;
    }
    let mut out = Vec::with_capacity(size);
    let mut i = 0;
    while i < src.len() {
        let token = src[i];
        i += 1;
        let mut lit = usize::from(token >> 4);
        if lit == 15 {
            loop {
                let b = *src.get(i)?;
                i += 1;
                lit = lit.checked_add(usize::from(b))?;
                if b != 255 {
                    break;
                }
            }
        }
        out.extend_from_slice(src.get(i..i.checked_add(lit)?)?);
        i += lit;
        if out.len() > size {
            return None;
        }
        if i == src.len() {
            break; // the last sequence carries literals only
        }
        let off = usize::from(u16::from_le_bytes([*src.get(i)?, *src.get(i + 1)?]));
        i += 2;
        if off == 0 || off > out.len() {
            return None;
        }
        let mut len = usize::from(token & 15);
        if len == 15 {
            loop {
                let b = *src.get(i)?;
                i += 1;
                len = len.checked_add(usize::from(b))?;
                if b != 255 {
                    break;
                }
            }
        }
        len += 4;
        if out.len() + len > size {
            return None;
        }
        let start = out.len() - off;
        for k in 0..len {
            out.push(out[start + k]); // byte by byte: the match may overlap its own output
        }
    }
    (out.len() == size).then_some(out)
}

/// Frames of a flat buffer; stops at the first incomplete or invalid frame.
pub fn split_frames(buf: &[u8]) -> Vec<Vec<u8>> {
    let (mut out, mut pos) = (Vec::new(), 0);
    while let Step::Frame { body, next } = step(buf, pos) {
        out.push(buf[body..next].to_vec());
        pos = next;
    }
    out
}

/// One frame body as sent → the frame bodies it carries. A `ff ff` container
/// (`ff ff` + u32 LE inflated size + one LZ4 block) is inflated and split; anything else passes
/// through. A corrupt container yields nothing.
pub fn expand(body: Vec<u8>) -> Vec<Vec<u8>> {
    if !body.starts_with(&[0xff, 0xff]) {
        return vec![body];
    }
    let Some(size) = body.get(2..6).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize) else {
        return Vec::new();
    };
    lz4_block(&body[6..], size).map(|raw| split_frames(&raw)).unwrap_or_default()
}

/// One decoded event with where it came from.
#[derive(Debug)]
pub struct Decoded {
    /// Capture time of the segment that completed the frame.
    pub ts_us: u64,
    /// Client TCP port of the connection (identifies the game client).
    pub conn: u16,
    pub event: Event,
}

/// Counters for cross-checking against `tools/re/aion_re.py chain`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    /// Top-level frames (containers count as one).
    pub frames: u64,
    pub containers: u64,
    /// Containers that failed to inflate.
    pub bad_containers: u64,
}

/// TCP segments in, typed events out. Only the server → client direction is decoded (the
/// client → server direction is encrypted and carries nothing we need).
pub struct Decoder {
    ops: Opcodes,
    server_port: u16,
    streams: Streams,
    /// Framer per direction, with the `skips` value it last saw.
    framers: HashMap<(SocketAddrV4, SocketAddrV4), (Framer, u32)>,
    stats: Stats,
}

impl Decoder {
    pub fn new(ops: Opcodes, server_port: u16) -> Self {
        Self { ops, server_port, streams: Streams::default(), framers: HashMap::new(), stats: Stats::default() }
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn push(&mut self, ts_us: u64, seg: &Segment) -> Vec<Decoded> {
        if seg.src.port() != self.server_port {
            return Vec::new();
        }
        let key = (seg.src, seg.dst);
        if self.streams.starts_new_connection(seg) {
            self.framers.remove(&key);
        }
        let Some(bytes) = self.streams.push(seg) else { return Vec::new() };
        let skips = self.streams.skips(key);
        let synced = self.streams.started_with_syn(key);
        // A SYN-anchored stream starts on a boundary only if nothing was skipped: when the first
        // bytes we deliver already follow an abandoned gap they begin mid-frame.
        let (framer, seen) = self.framers.entry(key).or_insert_with(|| (Framer::new(synced && skips == 0), skips));
        if *seen != skips {
            framer.lose_sync(); // a gap was abandoned: the next bytes are not on a boundary
            *seen = skips;
        }
        let conn = seg.dst.port();
        let mut out = Vec::new();
        for frame in framer.push(&bytes) {
            self.stats.frames += 1;
            let container = frame.starts_with(&[0xff, 0xff]);
            let inner = expand(frame);
            if container {
                self.stats.containers += 1;
                if inner.is_empty() {
                    self.stats.bad_containers += 1;
                }
            }
            for f in inner {
                out.extend(self.ops.parse_all(&f).map(|event| Decoded { ts_us, conn, event }));
            }
        }
        out
    }
}

/// Decode a whole Ethernet pcap written by `re record`.
pub fn decode_file(path: &Path, ops: Opcodes, server_port: u16) -> Result<(Vec<Decoded>, Stats)> {
    let reader = Reader::new(BufReader::new(File::open(path)?))?;
    if reader.linktype != LINKTYPE_ETHERNET {
        bail!("only Ethernet captures are supported (link type {})", reader.linktype);
    }
    let mut decoder = Decoder::new(ops, server_port);
    let mut out = Vec::new();
    for rec in reader {
        let rec = rec?;
        if let Some(seg) = parse_ethernet(&rec.data) {
            out.extend(decoder.push(rec.ts_us, &seg));
        }
    }
    Ok((out, decoder.stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint_bytes(mut v: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// `varint(len + 4) body`: total length = len(varint) + V - 4.
    fn frame(body: &[u8]) -> Vec<u8> {
        let mut f = varint_bytes(body.len() + 4);
        f.extend_from_slice(body);
        f
    }

    /// 10-byte body: opcode `00 36` + u64 time, framed as `0e 00 36 …`.
    fn ping() -> Vec<u8> {
        frame(&[0x00, 0x36, 1, 2, 3, 4, 5, 6, 7, 8])
    }

    #[test]
    fn frames_split_across_pushes() {
        let all = [frame(b"\x01\x02hello"), frame(b"\x03\x04world!")].concat();
        let mut f = Framer::new(true);
        let mut got = Vec::new();
        for chunk in all.chunks(3) {
            got.extend(f.push(chunk));
        }
        assert_eq!(got, vec![b"\x01\x02hello".to_vec(), b"\x03\x04world!".to_vec()]);
    }

    #[test]
    fn two_byte_varint_header() {
        let body = vec![7u8; 200];
        let mut f = Framer::new(true);
        assert_eq!(f.push(&frame(&body)), vec![body]);
    }

    #[test]
    fn resyncs_on_the_first_ping_that_chains() {
        let mut stream = vec![0x9a, 0x13, 0x55, 0x01, 0x02]; // tail of a frame we never saw
        for i in 0..6 {
            stream.extend(ping());
            stream.extend(frame(&[0x10, 0x20, i]));
        }
        let mut f = Framer::new(false);
        let got = f.push(&stream);
        assert_eq!(got.len(), 12);
        assert_eq!(got[0][..2], [0x00, 0x36]);
    }

    #[test]
    fn a_ping_split_across_pushes_is_still_found() {
        for inside in [1, 2] {
            // cut after the first or second byte of the 3-byte ping header
            let mut stream = vec![0xaa, 0xbb];
            let cut = stream.len() + inside;
            for i in 0..6 {
                stream.extend(ping());
                stream.extend(frame(&[0x10, 0x20, i]));
            }
            let mut f = Framer::new(false);
            let mut got = f.push(&stream[..cut]);
            assert!(got.is_empty());
            got.extend(f.push(&stream[cut..]));
            assert_eq!(got.len(), 12);
            assert_eq!(got[0][..2], [0x00, 0x36]);
        }
    }

    #[test]
    fn an_undecided_candidate_waits_for_more_bytes() {
        let mut stream = vec![0xaa, 0xbb];
        stream.extend(ping());
        stream.extend(frame(&[0x10, 0x20, 0]));
        stream.extend(frame(&[0x10, 0x20, 1])); // only 3 frames chain so far: not yet trusted
        let mut f = Framer::new(false);
        assert!(f.push(&stream).is_empty());
        let mut more = Vec::new();
        for _ in 0..3 {
            more.extend(ping());
        }
        assert_eq!(f.push(&more).len(), 6);
    }

    #[test]
    fn a_fake_ping_that_does_not_chain_is_skipped() {
        let mut stream = frame(&[0x00, 0x36, 1, 2, 3, 4, 5, 6, 7, 8]); // looks like a ping…
        stream.extend([0xff; 6]); // …but is followed by an impossible length
        for _ in 0..5 {
            stream.extend(ping());
        }
        let mut f = Framer::new(false);
        assert_eq!(f.push(&stream).len(), 5);
    }

    #[test]
    fn lose_sync_drops_the_partial_frame_and_waits_for_a_ping() {
        let mut f = Framer::new(true);
        let half = frame(&[1u8; 50]);
        assert!(f.push(&half[..10]).is_empty());
        f.lose_sync();
        let mut s = vec![0xaa, 0xbb];
        for _ in 0..5 {
            s.extend(ping());
        }
        assert_eq!(f.push(&s).len(), 5);
    }

    #[test]
    fn a_corrupt_length_mid_stream_triggers_a_resync() {
        let mut s = [frame(b"\x01\x01a"), frame(b"\x02\x02b")].concat();
        s.extend([0xff; 4]);
        for _ in 0..5 {
            s.extend(ping());
        }
        let mut f = Framer::new(true);
        assert_eq!(f.push(&s).len(), 2 + 5);
    }

    #[test]
    fn lz4_literals_only() {
        assert_eq!(lz4_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], 5).unwrap(), b"hello");
    }

    #[test]
    fn lz4_overlapping_match() {
        // 3 literals, a 9-byte match at offset 3 (overlaps itself), 5 closing literals
        let src = [&[0x35][..], b"abc", &[0x03, 0x00, 0x50], b"defgh"].concat();
        assert_eq!(lz4_block(&src, 17).unwrap(), b"abcabcabcabcdefgh");
    }

    #[test]
    fn lz4_long_literal_run() {
        let mut src = vec![0xF0, 5]; // 15 + 5 literals
        src.extend([b'A'; 20]);
        assert_eq!(lz4_block(&src, 20).unwrap(), [b'A'; 20]);
    }

    #[test]
    fn lz4_rejects_malformed_input() {
        assert!(lz4_block(&[0x50, b'h', b'e'], 5).is_none()); // literals run past the input
        assert!(lz4_block(&[0x10, b'a', 0x05, 0x00, 0x00], 9).is_none()); // offset before the output start
        assert!(lz4_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], 6).is_none()); // size mismatch
        assert!(lz4_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], MAX_INFLATED + 1).is_none());
    }

    #[test]
    fn container_is_flattened_and_plain_frames_pass_through() {
        let inner = [frame(b"\x01\x01aa"), frame(b"\x02\x02bb")].concat();
        assert!(inner.len() < 15); // one-nibble literal length
        let mut body = vec![0xff, 0xff];
        body.extend((inner.len() as u32).to_le_bytes());
        body.push((inner.len() as u8) << 4);
        body.extend(&inner);
        assert_eq!(expand(body), vec![b"\x01\x01aa".to_vec(), b"\x02\x02bb".to_vec()]);
        assert_eq!(expand(b"\x0b\x61x".to_vec()), vec![b"\x0b\x61x".to_vec()]);
        assert!(expand(vec![0xff, 0xff, 9, 0, 0, 0, 0x50]).is_empty()); // corrupt container
    }

    #[test]
    fn a_container_claiming_far_more_than_lz4_can_expand_is_rejected() {
        let mut body = vec![0xff, 0xff];
        body.extend((MAX_INFLATED as u32).to_le_bytes()); // 16 MiB from a 1-byte block
        body.push(0x00);
        assert!(expand(body).is_empty());
        assert!(lz4_block(&[0x00], 256).is_none()); // over 255:1
        assert!(lz4_block(&[0x00], 0).is_some()); // an empty block still inflates to nothing
    }

    use std::net::{Ipv4Addr, SocketAddrV4};

    fn seg_from(src: SocketAddrV4, dst: SocketAddrV4, seq: u32, syn: bool, payload: Vec<u8>) -> Segment {
        Segment { src, dst, seq, syn, fin: false, rst: false, payload }
    }

    #[test]
    fn decoder_emits_events_from_a_syn_started_stream() {
        let server = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 13328);
        let client = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 50000);
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        assert!(d.push(1, &seg_from(server, client, 99, true, vec![])).is_empty());

        let mut power = vec![0x56, 0x36, 0, 0, 0, 0, 0, 0, 0, 0];
        power.extend(12345u32.to_le_bytes());
        power.extend([0u8; 4]);
        let mut bytes = ping();
        bytes.extend(frame(&power));
        let (a, b) = bytes.split_at(15); // the second frame is cut mid-header-body
        assert!(d.push(2, &seg_from(server, client, 100, false, a.to_vec())).is_empty());
        let got = d.push(3, &seg_from(server, client, 100 + a.len() as u32, false, b.to_vec()));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].event, Event::CombatPower(12345));
        assert_eq!((got[0].conn, got[0].ts_us), (50000, 3));
        assert_eq!(d.stats().frames, 2);
    }

    #[test]
    fn decoder_ignores_the_client_direction() {
        let server = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 13328);
        let client = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 50000);
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        assert!(d.push(1, &seg_from(client, server, 100, false, ping())).is_empty());
        assert_eq!(d.stats().frames, 0);
    }

    fn power_frame(value: u32) -> Vec<u8> {
        let mut body = vec![0x56, 0x36, 0, 0, 0, 0, 0, 0, 0, 0];
        body.extend(value.to_le_bytes());
        body.extend([0u8; 4]);
        frame(&body)
    }

    /// Valid ping frames until `len` bytes are exceeded, then one combat-power frame.
    fn pings_then_power(len: usize, value: u32) -> Vec<u8> {
        let mut v = Vec::new();
        while v.len() <= len {
            v.extend(ping());
        }
        v.extend(power_frame(value));
        v
    }

    const SERVER: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 13328);
    const CLIENT: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 50000);

    fn events(got: Vec<Decoded>) -> Vec<Event> {
        got.into_iter().map(|d| d.event).collect()
    }

    #[test]
    fn resync_retains_large_frames_across_pushes() {
        let mut bytes = vec![0xaa, 0xbb]; // tail of a frame captured without its beginning
        bytes.extend(ping());
        bytes.extend(frame(&vec![0x10; 100 << 10]));
        bytes.extend(power_frame(12345));
        bytes.extend(ping());

        let mut whole = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        let expected = events(whole.push(1, &seg_from(SERVER, CLIENT, 5000, false, bytes.clone())));
        assert_eq!(expected, vec![Event::CombatPower(12345)]);
        assert_eq!(whole.stats().frames, 4);

        let mut split = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        let cut = 70 << 10; // the large second frame is incomplete beyond the old 64 KiB window
        assert!(split.push(1, &seg_from(SERVER, CLIENT, 5000, false, bytes[..cut].to_vec())).is_empty());
        let got = split.push(2, &seg_from(SERVER, CLIENT, 5000 + cut as u32, false, bytes[cut..].to_vec()));
        assert_eq!(events(got), expected);
        assert_eq!(split.stats(), whole.stats());
    }

    #[test]
    fn a_skip_after_decoded_frames_resyncs_on_a_ping() {
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        d.push(1, &seg_from(SERVER, CLIENT, 99, true, vec![]));
        let first = [ping(), power_frame(1)].concat();
        let n = first.len() as u32;
        assert_eq!(events(d.push(2, &seg_from(SERVER, CLIENT, 100, false, first))), vec![Event::CombatPower(1)]);
        // 40 bytes never arrive; what follows is over the pending cap, so the gap is abandoned
        let big = pings_then_power(1 << 20, 2);
        let got = d.push(3, &seg_from(SERVER, CLIENT, 100 + n + 40, false, big));
        assert_eq!(events(got), vec![Event::CombatPower(2)]);
    }

    #[test]
    fn a_stream_whose_first_bytes_follow_an_abandoned_gap_is_not_on_a_boundary() {
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        d.push(1, &seg_from(SERVER, CLIENT, 99, true, vec![]));
        // the SYN was seen but the first data sits behind a hole; it begins mid-frame, with the
        // tail of a frame header that claims ~2 MiB
        let mut big = vec![0xff, 0xff, 0x7f];
        big.extend(pings_then_power(1 << 20, 3));
        let got = d.push(2, &seg_from(SERVER, CLIENT, 100 + 40, false, big));
        assert_eq!(events(got), vec![Event::CombatPower(3)]);
    }

    #[test]
    fn a_new_syn_on_the_same_tuple_drops_the_old_framer() {
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        d.push(1, &seg_from(SERVER, CLIENT, 99, true, vec![]));
        let mut old = [ping(), power_frame(1)].concat();
        old.extend(&power_frame(9)[..5]); // the connection died mid-frame
        assert_eq!(events(d.push(2, &seg_from(SERVER, CLIENT, 100, false, old))), vec![Event::CombatPower(1)]);

        assert!(d.push(3, &seg_from(SERVER, CLIENT, 7000, true, vec![])).is_empty());
        let fresh = [ping(), power_frame(2)].concat();
        assert_eq!(events(d.push(4, &seg_from(SERVER, CLIENT, 7001, false, fresh))), vec![Event::CombatPower(2)]);
    }

    #[test]
    fn decoder_resyncs_on_a_stream_that_began_mid_frame() {
        let server = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 13328);
        let client = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 50000);
        let mut d = Decoder::new(Opcodes::embedded("default").unwrap(), 13328);
        let mut power = vec![0x56, 0x36, 0, 0, 0, 0, 0, 0, 0, 0];
        power.extend(7u32.to_le_bytes());
        power.extend([0u8; 4]);
        let mut bytes = vec![0x55, 0x66, 0x77]; // end of a frame we never saw (no SYN)
        for _ in 0..5 {
            bytes.extend(ping());
        }
        bytes.extend(frame(&power));
        let got = d.push(9, &seg_from(server, client, 5000, false, bytes));
        assert_eq!(got.iter().map(|e| e.event.clone()).collect::<Vec<_>>(), vec![Event::CombatPower(7)]);
    }
}
