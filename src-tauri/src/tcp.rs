use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddrV4;

use etherparse::{NetSlice, SlicedPacket, TransportSlice};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub src: SocketAddrV4,
    pub dst: SocketAddrV4,
    pub seq: u32,
    pub syn: bool,
    pub fin: bool,
    pub rst: bool,
    pub payload: Vec<u8>,
}

/// Parse an Ethernet II frame. `None` for anything that is not IPv4 TCP.
pub fn parse_ethernet(frame: &[u8]) -> Option<Segment> {
    let p = SlicedPacket::from_ethernet(frame).ok()?;
    let (NetSlice::Ipv4(ip), Some(TransportSlice::Tcp(tcp))) = (p.net.as_ref()?, p.transport.as_ref())
    else {
        return None;
    };
    let h = ip.header();
    Some(Segment {
        src: SocketAddrV4::new(h.source_addr(), tcp.source_port()),
        dst: SocketAddrV4::new(h.destination_addr(), tcp.destination_port()),
        seq: tcp.sequence_number(),
        syn: tcp.syn(),
        fin: tcp.fin(),
        rst: tcp.rst(),
        payload: tcp.payload().to_vec(),
    })
}

/// Out-of-order bytes held behind a gap before the gap is abandoned.
/// ponytail: 1 MiB; the missing bytes are assumed lost for good. Tune if captures show real loss.
const MAX_PENDING: usize = 1 << 20;

/// Reassembles one direction of one TCP connection.
#[derive(Default)]
pub struct Reassembler {
    base: Option<u32>,
    from_syn: bool,
    delivered: u64,
    skips: u32,
    pending: BTreeMap<u64, Vec<u8>>,
}

impl Reassembler {
    /// Bytes received but held back behind a gap (not yet deliverable).
    pub fn pending_bytes(&self) -> usize {
        self.pending.values().map(Vec::len).sum()
    }

    /// True when the stream was anchored on its SYN, so byte 0 is a frame boundary.
    pub fn from_syn(&self) -> bool {
        self.from_syn
    }

    /// How many times a gap was abandoned and the stream skipped forward. Whoever frames the
    /// bytes must resynchronise when this changes.
    pub fn skips(&self) -> u32 {
        self.skips
    }

    /// Feed one segment; returns the bytes that became contiguous (possibly empty).
    pub fn push(&mut self, seg: &Segment) -> Vec<u8> {
        let base = match self.base {
            Some(b) => b,
            None => {
                // Anchor on the SYN or on the first segment with data: an empty mid-stream ACK or
                // keepalive says nothing about where the data starts.
                if !seg.syn && seg.payload.is_empty() {
                    return Vec::new();
                }
                let b = seg.seq.wrapping_add(seg.syn as u32); // SYN uses one seq number
                self.base = Some(b);
                self.from_syn = seg.syn;
                b
            }
        };
        if seg.payload.is_empty() {
            return Vec::new();
        }
        // signed distance from the expected sequence number is wrap-safe within 2 GiB
        let expected = base.wrapping_add(self.delivered as u32);
        let delta = seg.seq.wrapping_sub(expected) as i32 as i64;
        let mut start = self.delivered as i64 + delta;
        let mut data = seg.payload.as_slice();
        if start < self.delivered as i64 {
            let skip = (self.delivered as i64 - start) as usize;
            if skip >= data.len() {
                return Vec::new(); // pure retransmit
            }
            data = &data[skip..];
            start = self.delivered as i64;
        }
        self.pending
            .entry(start as u64)
            .and_modify(|v| {
                if data.len() > v.len() {
                    *v = data.to_vec();
                }
            })
            .or_insert_with(|| data.to_vec());

        if self.pending_bytes() > MAX_PENDING
            && let Some((&first, _)) = self.pending.first_key_value()
            && first > self.delivered
        {
            // the missing bytes are not coming back: continue from the first chunk we hold
            self.delivered = first;
            self.skips += 1;
        }

        let mut out = Vec::new();
        while let Some((&off, _)) = self.pending.first_key_value() {
            if off > self.delivered {
                break;
            }
            let chunk = self.pending.pop_first().unwrap().1;
            let skip = (self.delivered - off) as usize;
            if skip < chunk.len() {
                out.extend_from_slice(&chunk[skip..]);
                self.delivered += (chunk.len() - skip) as u64;
            }
        }
        out
    }
}

type Key = (SocketAddrV4, SocketAddrV4);

/// One `Reassembler` per (src, dst) direction. A SYN that opens a new connection on a known tuple
/// resets it.
#[derive(Default)]
pub struct Streams {
    by_dir: HashMap<Key, Reassembler>,
}

impl Streams {
    /// True for a SYN that opens a new connection on a tuple we already track. A retransmitted SYN
    /// has the same sequence number and must not reset a stream that already has data.
    pub fn starts_new_connection(&self, seg: &Segment) -> bool {
        seg.syn
            && self
                .by_dir
                .get(&(seg.src, seg.dst))
                .is_some_and(|r| r.base.is_some_and(|b| b != seg.seq.wrapping_add(1)))
    }

    pub fn push(&mut self, seg: &Segment) -> Option<Vec<u8>> {
        let key = (seg.src, seg.dst);
        if self.starts_new_connection(seg) {
            self.by_dir.remove(&key);
        }
        let bytes = self.by_dir.entry(key).or_default().push(seg);
        (!bytes.is_empty()).then_some(bytes)
    }

    /// Was this direction anchored on its SYN? False for a capture that began mid-stream.
    pub fn started_with_syn(&self, key: Key) -> bool {
        self.by_dir.get(&key).is_some_and(Reassembler::from_syn)
    }

    /// Gaps abandoned so far in this direction (see `Reassembler::skips`).
    pub fn skips(&self, key: Key) -> u32 {
        self.by_dir.get(&key).map_or(0, Reassembler::skips)
    }

    /// Directions that still hold bytes behind a gap, with the byte count.
    pub fn gaps(&self) -> Vec<(Key, usize)> {
        self.by_dir
            .iter()
            .map(|(k, r)| (*k, r.pending_bytes()))
            .filter(|&(_, n)| n > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(last: u8, port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(std::net::Ipv4Addr::new(10, 0, 0, last), port)
    }

    fn seg(seq: u32, payload: &[u8]) -> Segment {
        Segment {
            src: addr(1, 5000),
            dst: addr(2, 13328),
            seq,
            syn: false,
            fin: false,
            rst: false,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn in_order() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        assert_eq!(r.push(&seg(103, b"def")), b"def");
    }

    #[test]
    fn out_of_order_is_held_until_gap_fills() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        assert_eq!(r.push(&seg(106, b"ghi")), b"");
        assert_eq!(r.push(&seg(103, b"def")), b"defghi");
    }

    #[test]
    fn pending_bytes_counts_data_behind_a_gap() {
        let mut r = Reassembler::default();
        r.push(&seg(100, b"abc"));
        r.push(&seg(106, b"ghi"));
        assert_eq!(r.pending_bytes(), 3);
        r.push(&seg(103, b"def"));
        assert_eq!(r.pending_bytes(), 0);
    }

    #[test]
    fn streams_report_gaps_only_when_bytes_are_stuck() {
        let mut s = Streams::default();
        s.push(&seg(100, b"abc"));
        assert!(s.gaps().is_empty());
        s.push(&seg(106, b"ghi"));
        assert_eq!(s.gaps(), vec![((addr(1, 5000), addr(2, 13328)), 3)]);
        s.push(&seg(103, b"def"));
        assert!(s.gaps().is_empty());
    }

    #[test]
    fn retransmit_and_overlap_are_not_duplicated() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abcd")), b"abcd");
        assert_eq!(r.push(&seg(100, b"abcd")), b"");
        assert_eq!(r.push(&seg(102, b"cdef")), b"ef");
    }

    #[test]
    fn syn_consumes_one_sequence_number() {
        let mut r = Reassembler::default();
        let mut s = seg(99, b"");
        s.syn = true;
        assert_eq!(r.push(&s), b"");
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
    }

    #[test]
    fn sequence_numbers_wrap() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(u32::MAX - 1, b"ab")), b"ab"); // seq MAX-1, MAX
        assert_eq!(r.push(&seg(0, b"cd")), b"cd");
    }

    #[test]
    fn new_syn_on_same_tuple_starts_a_fresh_stream() {
        let mut s = Streams::default();
        assert_eq!(s.push(&seg(100, b"old")).unwrap(), b"old");
        let mut syn = seg(5000, b"");
        syn.syn = true;
        assert!(s.push(&syn).is_none());
        assert_eq!(s.push(&seg(5001, b"new")).unwrap(), b"new");
    }

    #[test]
    fn parses_an_ethernet_ipv4_tcp_frame() {
        // Ethernet II + IPv4 + TCP (PSH|ACK) carrying "hi"; checksums not validated
        let frame: Vec<u8> = [
            &[0u8; 6][..], &[0u8; 6][..], &[0x08, 0x00][..], // eth: dst, src, ethertype IPv4
            &[0x45, 0, 0, 42, 0, 0, 0, 0, 64, 6, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2][..], // ipv4 (len 42)
            &[0x13, 0x88, 0x34, 0x10, 0, 0, 0, 100, 0, 0, 0, 0, 0x50, 0x18, 0xff, 0xff, 0, 0, 0, 0][..], // tcp 5000 -> 13328
            b"hi",
        ].concat();
        let s = parse_ethernet(&frame).unwrap();
        assert_eq!(s.src, addr(1, 5000));
        assert_eq!(s.dst, addr(2, 13328));
        assert_eq!(s.seq, 100);
        assert_eq!(s.payload, b"hi");
        assert!(!s.syn && !s.fin && !s.rst);
    }

    #[test]
    fn non_tcp_frames_are_ignored() {
        assert!(parse_ethernet(&[0u8; 10]).is_none());
    }

    #[test]
    fn retransmitted_syn_does_not_reset_a_stream_with_data() {
        let mut s = Streams::default();
        let mut syn = seg(99, b"");
        syn.syn = true;
        assert!(s.push(&syn).is_none());
        assert_eq!(s.push(&seg(100, b"abc")).unwrap(), b"abc");
        assert!(!s.starts_new_connection(&syn)); // same seq: a retransmit
        assert!(s.push(&syn).is_none());
        assert_eq!(s.push(&seg(103, b"def")).unwrap(), b"def");
    }

    #[test]
    fn empty_keepalive_does_not_anchor_the_stream() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(50, b"")), b"");
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        assert_eq!(r.push(&seg(103, b"def")), b"def");
        assert!(!r.from_syn());
    }

    #[test]
    fn from_syn_is_reported() {
        let mut r = Reassembler::default();
        let mut s = seg(99, b"");
        s.syn = true;
        r.push(&s);
        assert!(r.from_syn());
        let key = (addr(1, 5000), addr(2, 13328));
        let mut streams = Streams::default();
        streams.push(&s);
        assert!(streams.started_with_syn(key));
        assert_eq!(streams.skips(key), 0);
    }

    #[test]
    fn a_gap_over_the_cap_is_abandoned_and_counted() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        let big = vec![7u8; MAX_PENDING + 1];
        let out = r.push(&seg(200, &big)); // 97 bytes missing and never coming
        assert_eq!(out.len(), big.len());
        assert_eq!(r.skips(), 1);
        assert_eq!(r.pending_bytes(), 0);
    }

    #[test]
    fn a_filled_hole_that_crosses_the_cap_is_not_a_skip() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        let held = vec![7u8; MAX_PENDING];
        assert_eq!(r.push(&seg(106, &held)), b""); // behind a 3-byte hole, exactly at the cap
        let out = r.push(&seg(103, b"def")); // the hole-filler pushes pending over the cap
        let mut want = b"def".to_vec();
        want.extend(&held);
        assert_eq!(out, want);
        assert_eq!(r.skips(), 0);
    }

    #[test]
    fn overlap_with_delivered_bytes_after_a_wrap() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(u32::MAX - 1, b"ab")), b"ab"); // seq MAX-1, MAX
        assert_eq!(r.push(&seg(0, b"cd")), b"cd");
        assert_eq!(r.push(&seg(0, b"cd")), b""); // retransmit after the wrap
        assert_eq!(r.push(&seg(u32::MAX, b"bcde")), b"e"); // overlaps three delivered bytes
    }

    #[test]
    fn a_longer_chunk_at_the_same_start_replaces_the_shorter_one() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&seg(100, b"abc")), b"abc");
        assert_eq!(r.push(&seg(106, b"gh")), b"");
        assert_eq!(r.push(&seg(106, b"ghij")), b"");
        assert_eq!(r.push(&seg(103, b"def")), b"defghij");
    }
}
