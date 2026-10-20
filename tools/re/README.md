# tools/re

`aion_re.py` analyses captures recorded with `re record` (see `docs/protocol.md` for the format). Stdlib only, plus `lz4` for container expansion.

```
uv run --with lz4 python tools/re/aion_re.py chain   captures/x.pcap          # do frames chain to the end of each stream?
uv run --with lz4 python tools/re/aion_re.py hist    captures/x.pcap          # opcode candidates and body lengths (server -> client)
uv run --with lz4 python tools/re/aion_re.py findstr captures/x.pcap TEXT     # UTF-8 / UTF-16LE string search
uv run --with lz4 python tools/re/aion_re.py findint captures/x.pcap 1234 --at T_US --within 2
uv run --with lz4 python tools/re/aion_re.py frame captures/x.pcap IDX --bytes 256
```

Notes:

- Only server → client frames are searched (client → server is encrypted).
- `frame` hex-dumps one flattened frame by the index that `findstr`/`findint` print.
- `findint` tries the value as u8/u16/u32/u64, little- and big-endian, inside decompressed inner frames; `--at/--within` restrict to a time window (in seconds) around an absolute capture time in microseconds (a `t_us` from `re decode`).
- Captures must contain the TCP handshake; a mid-stream capture has no framing anchor.
- The Rust `re dump` is the reference reassembler; for the opcodes in `events.rs` it has been superseded by `re decode`; this script stays the tool for ad-hoc searching (`findstr`/`findint`/`frame`).
