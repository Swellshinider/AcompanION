#!/usr/bin/env python3
"""Offline analysis of AION 2 captures recorded with `re record` (see docs/protocol.md).

    uv run --with lz4 python tools/re/aion_re.py chain   captures/x.pcap
    uv run --with lz4 python tools/re/aion_re.py hist    captures/x.pcap
    uv run --with lz4 python tools/re/aion_re.py findstr captures/x.pcap NAME
    uv run --with lz4 python tools/re/aion_re.py findint captures/x.pcap 1234 [--at T_US --within 2.0]
    uv run --with lz4 python tools/re/aion_re.py frame captures/x.pcap IDX [--bytes N]

Assumes the capture started before the game connected (SYN seen). Only server -> client
frames are analysed; client -> server is encrypted. Output shows a few context bytes per hit:
do not paste capture bytes into docs or commits.
"""
import argparse
import bisect
import collections
import struct
import sys

GAME_PORT = 13328
PING = b"\x0e\x00\x36"  # frame header + opcode of the server ping (10-byte body, every ~50 ms)


def read_pcap(path):
    data = open(path, "rb").read()
    if data[:4] != b"\xd4\xc3\xb2\xa1":
        sys.exit("not a little-endian microsecond pcap file")
    off = 24
    while off + 16 <= len(data):
        sec, usec, incl, _ = struct.unpack_from("<IIII", data, off)
        off += 16
        if off + incl > len(data):
            return  # truncated tail
        yield sec * 1_000_000 + usec, data[off:off + incl]
        off += incl


class Stream:
    """Reassembled bytes of one direction, with the capture time of each segment."""

    def __init__(self):
        self.data = bytearray()
        self.start = 0  # first frame boundary; > 0 when the capture began mid-stream
        self._offs, self._ts = [], []

    def add(self, ts, payload):
        self._offs.append(len(self.data))
        self._ts.append(ts)
        self.data += payload

    def ts_at(self, off):
        return self._ts[max(0, bisect.bisect_right(self._offs, off) - 1)]


def chains(buf, i):
    """True if frames starting at buf[i] chain to the end of buf (a partial last frame is fine)."""
    while i < len(buf):
        try:
            v, n = varint(buf, i)
        except IndexError:
            return True
        tot = n + v - 4
        if tot < n:
            return False
        if i + tot > len(buf):
            return True
        i += tot
    return True


def sync(buf):
    """Frame boundary of a stream captured mid-stream: first server ping that chains to the end."""
    i = buf.find(PING)
    while i >= 0:
        if chains(buf, i):
            return i
        i = buf.find(PING, i + 1)
    return None


def connections(path, port=GAME_PORT):
    """{local_port: {'c2s': Stream, 's2c': Stream}}. Without a SYN the stream starts at the first
    payload seen and the server direction is synced on a ping (client direction is not needed)."""
    conns, nxt, pend = {}, {}, {}
    for ts, f in read_pcap(path):
        if len(f) < 54 or f[12:14] != b"\x08\x00" or f[23] != 6:  # Ethernet, IPv4, TCP
            continue
        t = 14 + (f[14] & 0xF) * 4
        total = struct.unpack_from(">H", f, 16)[0]
        sp, dp, seq = struct.unpack_from(">HHI", f, t)
        if port not in (sp, dp):
            continue
        s2c = sp == port
        local, name = (dp, "s2c") if s2c else (sp, "c2s")
        key = (local, s2c)
        if f[t + 13] & 2:  # SYN: a new stream starts for this direction
            nxt[key] = (seq + 1) & 0xFFFFFFFF
            conns.setdefault(local, {})[name] = Stream()
            continue
        payload = f[t + (f[t + 12] >> 4) * 4:14 + total]
        if not payload:
            continue
        if key not in nxt:  # capture began mid-stream
            nxt[key] = seq
            conns.setdefault(local, {})[name] = Stream()
        pend.setdefault(key, []).append((seq, ts, payload))
        # deliver everything that became contiguous (handles reordering and retransmits)
        progressed = True
        while progressed:
            progressed = False
            for item in pend[key]:
                s_, t_, p_ = item
                delta = (s_ - nxt[key]) & 0xFFFFFFFF
                if delta >= 0x80000000:  # starts before what we already have
                    skip = (nxt[key] - s_) & 0xFFFFFFFF
                    if skip >= len(p_):
                        pend[key].remove(item)
                        progressed = True
                        break
                    p_, delta = p_[skip:], 0
                if delta == 0:
                    conns[local][name].add(t_, p_)
                    nxt[key] = (nxt[key] + len(p_)) & 0xFFFFFFFF
                    pend[key].remove(item)
                    progressed = True
                    break
    for d in conns.values():
        s = d.get("s2c")
        if s is not None and not s.start and not chains(s.data, 0):
            s.start = sync(s.data) or 0
    for (local, s2c), items in pend.items():
        if items:
            print(f"warning: {sum(len(p) for _, _, p in items)} bytes undelivered behind a gap on port "
                  f"{local} {'s2c' if s2c else 'c2s'}", file=sys.stderr)
    return conns


def varint(b, i):
    v = s = n = 0
    while True:
        c = b[i + n]
        v |= (c & 0x7F) << s
        n += 1
        s += 7
        if not c & 0x80:
            return v, n


def split_frames(buf, i=0):
    """Frame = varint(V) body, total length = len(varint) + V - 4. Returns ([(offset, body)], end)."""
    out = []
    while i < len(buf):
        try:
            v, n = varint(buf, i)
        except IndexError:
            break
        tot = n + v - 4
        if tot < n or i + tot > len(buf):
            break
        out.append((i, bytes(buf[i + n:i + tot])))
        i += tot
    return out, i


def expand(stream):
    """Yield (ts_us, body) for every frame, flattening `ff ff` LZ4 containers."""
    import lz4.block

    frames, _ = split_frames(stream.data, stream.start)
    for off, body in frames:
        ts = stream.ts_at(off)
        if body[:2] == b"\xff\xff":
            raw = lz4.block.decompress(body[6:], uncompressed_size=struct.unpack_from("<I", body, 2)[0])
            for _, inner in split_frames(raw)[0]:
                yield ts, inner
        else:
            yield ts, body


def server_streams(path):
    for local, d in connections(path).items():
        if "s2c" in d:
            yield local, d["s2c"]


def cmd_chain(a):
    for local, d in connections(a.pcap).items():
        for name, s in d.items():
            frames, end = split_frames(s.data, s.start)
            boxes = sum(1 for _, b in frames if b[:2] == b"\xff\xff")
            print(f"port {local} {name}: {len(s.data)} bytes, {len(frames)} frames, "
                  f"chained to {end}/{len(s.data)}, {boxes} containers")


def cmd_hist(a):
    for local, s in server_streams(a.pcap):
        fr = [b for _, b in expand(s)]
        print(f"port {local} s2c: {len(fr)} frames after flattening containers")
        print(" first 2 bytes:", ", ".join(f"{h}x{n}" for h, n in
                                          collections.Counter(b[:2].hex() for b in fr).most_common(20)))
        print(" body lengths :", collections.Counter(len(b) for b in fr).most_common(10))


def in_window(ts, a):
    return a.at is None or abs(ts - a.at) <= a.within * 1_000_000


def show(local, idx, ts, body, pos, label):
    ctx = body[max(0, pos - 8):pos + 24].hex(" ")
    print(f"port {local} frame {idx} ts_us {ts} op {body[:2].hex()} len {len(body)} @ {pos} {label}\n    {ctx}")


def cmd_findstr(a):
    pats = {"utf8": a.text.encode(), "utf16le": a.text.encode("utf-16le")}
    for local, s in server_streams(a.pcap):
        for idx, (ts, body) in enumerate(expand(s)):
            if not in_window(ts, a):
                continue
            for label, p in pats.items():
                pos = body.find(p)
                while pos >= 0:
                    show(local, idx, ts, body, pos, label)
                    pos = body.find(p, pos + 1)


def cmd_findint(a):
    needles = []
    for w in (1, 2, 4, 8):
        if a.value < 256 ** w:
            needles += [(f"u{w * 8}le", a.value.to_bytes(w, "little")), (f"u{w * 8}be", a.value.to_bytes(w, "big"))]
    for local, s in server_streams(a.pcap):
        for idx, (ts, body) in enumerate(expand(s)):
            if not in_window(ts, a):
                continue
            for label, p in needles:
                pos = body.find(p)
                while pos >= 0:
                    show(local, idx, ts, body, pos, label)
                    pos = body.find(p, pos + 1)


def cmd_frame(a):
    """Hex dump of one flattened frame (IDX as printed by findstr/findint)."""
    for local, s in server_streams(a.pcap):
        for idx, (ts, body) in enumerate(expand(s)):
            if idx != a.index:
                continue
            print(f"port {local} frame {idx} ts_us {ts} op {body[:2].hex()} len {len(body)}")
            for o in range(0, min(len(body), a.bytes), 16):
                print(f"{o:5d}  {body[o:o + 16].hex(' ')}")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name, fn in (("chain", cmd_chain), ("hist", cmd_hist)):
        sub.add_parser(name).add_argument("pcap")
        sub.choices[name].set_defaults(fn=fn)
    for name, fn, arg, kind in (("findstr", cmd_findstr, "text", str), ("findint", cmd_findint, "value", int)):
        sp = sub.add_parser(name)
        sp.add_argument("pcap")
        sp.add_argument(arg, type=kind)
        sp.add_argument("--at", type=int, help="absolute time in µs (a t_us from `re decode`)")
        sp.add_argument("--within", type=float, default=2.0, help="± seconds around --at")
        sp.set_defaults(fn=fn)
    sf = sub.add_parser("frame")
    sf.add_argument("pcap")
    sf.add_argument("index", type=int)
    sf.add_argument("--bytes", type=int, default=256, help="how many bytes to dump")
    sf.set_defaults(fn=cmd_frame)
    a = p.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
