use std::io::{self, Read, Write};

pub const LINKTYPE_ETHERNET: u32 = 1;
const MAGIC: [u8; 4] = 0xa1b2c3d4u32.to_le_bytes(); // little-endian, µs timestamps

pub struct Record {
    pub ts_us: u64,
    pub data: Vec<u8>,
}

pub struct Writer<W: Write> {
    out: W,
}

impl<W: Write> Writer<W> {
    pub fn new(mut out: W, linktype: u32) -> io::Result<Self> {
        out.write_all(&MAGIC)?;
        out.write_all(&2u16.to_le_bytes())?; // version major
        out.write_all(&4u16.to_le_bytes())?; // version minor
        out.write_all(&0i32.to_le_bytes())?; // thiszone
        out.write_all(&0u32.to_le_bytes())?; // sigfigs
        out.write_all(&262144u32.to_le_bytes())?; // snaplen
        out.write_all(&linktype.to_le_bytes())?;
        Ok(Self { out })
    }

    pub fn write(&mut self, ts_us: u64, data: &[u8]) -> io::Result<()> {
        let len = (data.len() as u32).to_le_bytes();
        self.out.write_all(&((ts_us / 1_000_000) as u32).to_le_bytes())?;
        self.out.write_all(&((ts_us % 1_000_000) as u32).to_le_bytes())?;
        self.out.write_all(&len)?; // captured length
        self.out.write_all(&len)?; // original length
        self.out.write_all(data)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

pub struct Reader<R: Read> {
    input: R,
    pub linktype: u32,
}

impl<R: Read> Reader<R> {
    pub fn new(mut input: R) -> io::Result<Self> {
        let mut h = [0u8; 24];
        input.read_exact(&mut h)?;
        if h[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a little-endian microsecond pcap file",
            ));
        }
        let linktype = u32::from_le_bytes(h[20..24].try_into().unwrap());
        Ok(Self { input, linktype })
    }
}

impl<R: Read> Iterator for Reader<R> {
    type Item = io::Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut h = [0u8; 16];
        match self.input.read_exact(&mut h) {
            Ok(()) => {}
            // clean end, or a capture cut short mid-record: stop quietly
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return None,
            Err(e) => return Some(Err(e)),
        }
        let sec = u32::from_le_bytes(h[0..4].try_into().unwrap()) as u64;
        let usec = u32::from_le_bytes(h[4..8].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
        // no pre-allocation from the untrusted length: a short read means a truncated tail
        let mut data = Vec::new();
        match (&mut self.input).take(len as u64).read_to_end(&mut data) {
            Ok(_) if data.len() < len => return None,
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }
        Some(Ok(Record { ts_us: sec * 1_000_000 + usec, data }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut buf = Vec::new();
        {
            let mut w = Writer::new(&mut buf, LINKTYPE_ETHERNET).unwrap();
            w.write(1_700_000_000_123_456, &[1, 2, 3]).unwrap();
            w.write(1_700_000_001_000_000, &[]).unwrap();
        }
        let r = Reader::new(&buf[..]).unwrap();
        assert_eq!(r.linktype, LINKTYPE_ETHERNET);
        let recs: Vec<Record> = r.map(|x| x.unwrap()).collect();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].ts_us, 1_700_000_000_123_456);
        assert_eq!(recs[0].data, vec![1, 2, 3]);
        assert_eq!(recs[1].data, Vec::<u8>::new());
    }

    #[test]
    fn writer_emits_classic_pcap_bytes() {
        let mut buf = Vec::new();
        Writer::new(&mut buf, LINKTYPE_ETHERNET)
            .unwrap()
            .write(1_000_002, &[0xAA, 0xBB])
            .unwrap();
        let expected: Vec<u8> = [
            // global header
            &[0xd4, 0xc3, 0xb2, 0xa1][..],
            &[0x02, 0x00],
            &[0x04, 0x00],
            &[0x00, 0x00, 0x00, 0x00],
            &[0x00, 0x00, 0x00, 0x00],
            &[0x00, 0x00, 0x04, 0x00],
            &[0x01, 0x00, 0x00, 0x00],
            // record header: sec=1, usec=2, incl=2, orig=2
            &[0x01, 0x00, 0x00, 0x00],
            &[0x02, 0x00, 0x00, 0x00],
            &[0x02, 0x00, 0x00, 0x00],
            &[0x02, 0x00, 0x00, 0x00],
            // body
            &[0xAA, 0xBB],
        ]
        .concat();
        assert_eq!(buf, expected);
    }

    #[test]
    fn oversized_declared_length_ends_iteration() {
        let mut buf = Vec::new();
        Writer::new(&mut buf, LINKTYPE_ETHERNET).unwrap();
        buf.extend_from_slice(&[0u8; 8]); // ts_sec, ts_usec
        buf.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // incl_len
        buf.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // orig_len
        buf.extend_from_slice(&[1, 2, 3]); // only 3 body bytes
        let n = Reader::new(&buf[..]).unwrap().count();
        assert_eq!(n, 0);
    }

    #[test]
    fn rejects_other_formats() {
        let junk = [0u8; 24];
        assert!(Reader::new(&junk[..]).is_err());
    }

    #[test]
    fn truncated_tail_ends_iteration() {
        let mut buf = Vec::new();
        Writer::new(&mut buf, LINKTYPE_ETHERNET).unwrap().write(5, &[9; 4]).unwrap();
        buf.extend_from_slice(&[0u8; 7]); // partial record header
        let n = Reader::new(&buf[..]).unwrap().count();
        assert_eq!(n, 1);
    }
}
