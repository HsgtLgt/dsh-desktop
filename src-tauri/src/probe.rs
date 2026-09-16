use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

/// Application-level readiness: TCP + HTTP status — never sniff HTML/doctype.
///
/// Any well-formed HTTP status counts as ready. Since dsh 0.1.6 the root
/// answers 401 until the browser presents its per-launch token, so requiring
/// 2xx/3xx would make a perfectly healthy service look "not ready".
pub fn probe(port: u16) -> bool {
    matches!(probe_status(port), Some(100..=599))
}

/// Same check as [`probe`], but returns the HTTP status code so callers can
/// tell a token-gated service (401) apart from an open one.
pub fn probe_status(port: u16) -> Option<u16> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(1200)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(1200)));
    s.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    // Status line like "HTTP/1.1 200 OK"
    head.lines()
        .next()
        .and_then(|line| line.strip_prefix("HTTP/"))?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()
}
