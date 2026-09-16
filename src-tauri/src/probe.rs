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
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(1200)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(1200)));
    if s.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 512];
    let Ok(n) = s.read(&mut buf) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    // Status line like "HTTP/1.1 200 OK"
    head.lines()
        .next()
        .and_then(|line| line.strip_prefix("HTTP/"))
        .and_then(|rest| {
            let mut it = rest.split_whitespace();
            it.next()?;
            it.next()?.parse::<u16>().ok()
        })
        .map(|code| (100..=599).contains(&code))
        .unwrap_or(false)
}
