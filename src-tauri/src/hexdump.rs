use std::fmt::Write;

pub fn hexdump(data: &[u8]) -> String {
    let mut s = String::new();
    for (i, row) in data.chunks(16).enumerate() {
        write!(s, "{:06x}  ", i * 16).unwrap();
        for j in 0..16 {
            match row.get(j) {
                Some(b) => write!(s, "{b:02x} ").unwrap(),
                None => s.push_str("   "),
            }
            if j == 7 {
                s.push(' ');
            }
        }
        s.push(' ');
        s.extend(row.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }));
        s.push('\n');
    }
    s
}

/// Shannon entropy in bits per byte. ~8.0 means random/encrypted/well-compressed.
pub fn entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_has_offset_hex_and_ascii() {
        let s = hexdump(b"AB\x00");
        assert!(s.starts_with("000000  41 42 00 "));
        assert!(s.ends_with("AB.\n"));
    }

    #[test]
    fn long_input_wraps_every_16_bytes() {
        let s = hexdump(&[0u8; 33]);
        assert_eq!(s.lines().count(), 3);
        assert!(s.lines().nth(2).unwrap().starts_with("000020  "));
    }

    #[test]
    fn entropy_bounds() {
        assert_eq!(entropy(&[]), 0.0);
        assert_eq!(entropy(&[7u8; 100]), 0.0);
        let all: Vec<u8> = (0..=255u8).cycle().take(2560).collect();
        assert!((entropy(&all) - 8.0).abs() < 1e-9);
    }
}
