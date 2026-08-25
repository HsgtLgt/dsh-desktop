use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

/// Application-level readiness: TCP + HTTP status — never sniff HTML/doctype.
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
    let status_ok = head
        .lines()
        .next()
        .map(|line| {
            line.contains(" 200 ")
                || line.contains(" 301 ")
                || line.contains(" 302 ")
                || line.contains(" 304 ")
        })
        .unwrap_or(false);
    status_ok
}
