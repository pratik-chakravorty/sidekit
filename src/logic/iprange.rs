//! Expand IP ranges and CIDR blocks into addresses, or summarize a list of
//! addresses and ranges into the fewest CIDR blocks.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Expansion stops here so a /8 does not freeze the window.
pub const EXPAND_LIMIT: u128 = 65_536;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Family {
    V4,
    V6,
}

/// One input line as an inclusive range of integers.
fn parse_line(line: &str) -> Result<(Family, u128, u128), String> {
    let t = line.trim();
    let ip = |s: &str| -> Result<(Family, u128), String> {
        match s.trim().parse::<IpAddr>().map_err(|_| format!("\"{}\" is not an IP address", s.trim()))? {
            IpAddr::V4(a) => Ok((Family::V4, u32::from(a) as u128)),
            IpAddr::V6(a) => Ok((Family::V6, u128::from(a))),
        }
    };
    if let Some((a, p)) = t.split_once('/') {
        let (fam, n) = ip(a)?;
        let bits = if fam == Family::V4 { 32 } else { 128 };
        let p: u32 = p.trim().parse().ok().filter(|p| *p <= bits).ok_or(format!("\"{t}\" has an invalid prefix length"))?;
        let host_bits = bits - p;
        let mask = if host_bits == 128 { u128::MAX } else { (1u128 << host_bits) - 1 };
        return Ok((fam, n & !mask, (n & !mask) | mask));
    }
    if let Some((a, b)) = t.split_once('-') {
        let ((fa, x), (fb, y)) = (ip(a)?, ip(b)?);
        if fa != fb {
            return Err(format!("\"{t}\" mixes IPv4 and IPv6"));
        }
        if x > y {
            return Err(format!("\"{t}\" ends before it starts"));
        }
        return Ok((fa, x, y));
    }
    let (f, n) = ip(t)?;
    Ok((f, n, n))
}

fn fmt(f: Family, n: u128) -> String {
    match f {
        Family::V4 => Ipv4Addr::from(n as u32).to_string(),
        Family::V6 => Ipv6Addr::from(n).to_string(),
    }
}

fn lines(input: &str) -> impl Iterator<Item = &str> {
    input.lines().flat_map(|l| l.split(',')).map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Every address in the input, one per line. Returns (text, count, truncated).
pub fn expand(input: &str) -> Result<(String, u128, bool), String> {
    let mut out = Vec::new();
    let mut total: u128 = 0;
    for l in lines(input) {
        let (f, a, b) = parse_line(l)?;
        let size = b - a + 1;
        total = total.saturating_add(size);
        let room = EXPAND_LIMIT.saturating_sub(out.len() as u128);
        for n in a..=a.saturating_add(size.min(room).saturating_sub(1)) {
            if room == 0 {
                break;
            }
            out.push(fmt(f, n));
        }
    }
    Ok((out.join("\n"), total, total > out.len() as u128))
}

/// The fewest CIDR blocks covering exactly the input.
pub fn summarize(input: &str) -> Result<(String, usize), String> {
    let mut ranges: Vec<(Family, u128, u128)> = lines(input).map(parse_line).collect::<Result<_, _>>()?;
    ranges.sort_by_key(|r| (r.0 == Family::V6, r.1));
    // Merge overlapping and adjacent ranges.
    let mut merged: Vec<(Family, u128, u128)> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(m) if m.0 == r.0 && r.1 <= m.2.saturating_add(1) => m.2 = m.2.max(r.2),
            _ => merged.push(r),
        }
    }
    let mut blocks = Vec::new();
    for (f, mut a, b) in merged {
        let bits = if f == Family::V4 { 32 } else { 128 };
        loop {
            // The largest aligned block starting at `a` that stays within `b`.
            let mut size_bits = if a == 0 { bits } else { a.trailing_zeros().min(bits) };
            while size_bits > 0 && (size_bits == 128 || a + ((1u128 << size_bits) - 1) > b) {
                if size_bits == 128 && a == 0 && b == u128::MAX {
                    break;
                }
                size_bits -= 1;
            }
            blocks.push(format!("{}/{}", fmt(f, a), bits - size_bits));
            let last = if size_bits == 128 { u128::MAX } else { a + ((1u128 << size_bits) - 1) };
            if last >= b {
                break;
            }
            a = last + 1;
        }
    }
    let n = blocks.len();
    Ok((blocks.join("\n"), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanding() {
        let (text, n, cut) = expand("10.0.0.254 - 10.0.1.1\n192.168.1.0/31").unwrap();
        assert_eq!(text, "10.0.0.254\n10.0.0.255\n10.0.1.0\n10.0.1.1\n192.168.1.0\n192.168.1.1");
        assert_eq!((n, cut), (6, false));
        let (_, n, cut) = expand("10.0.0.0/8").unwrap();
        assert_eq!((n, cut), (16_777_216, true));
        assert!(expand("10.0.0.5-10.0.0.1").is_err());
        assert!(expand("10.0.0.1-::1").is_err());
    }

    #[test]
    fn summarizing() {
        let (text, n) = summarize("10.0.0.0\n10.0.0.1\n10.0.0.2\n10.0.0.3\n10.0.0.5\n10.0.1.0/24, 10.0.0.128/25").unwrap();
        assert_eq!(text, "10.0.0.0/30\n10.0.0.5/32\n10.0.0.128/25\n10.0.1.0/24");
        assert_eq!(n, 4);
        assert_eq!(summarize("192.168.0.0 - 192.168.3.255").unwrap().0, "192.168.0.0/22");
        assert_eq!(summarize("0.0.0.0/0").unwrap().0, "0.0.0.0/0");
        assert_eq!(summarize("2001:db8::/33\n2001:db8:8000::/33").unwrap().0, "2001:db8::/32");
    }
}
