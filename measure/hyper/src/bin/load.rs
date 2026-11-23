//! Loads a server on 127.0.0.1 and says what it did, with the standard library
//! alone so that it is the same client for every server.
//! Usage: load PORT PID
//!   rate: 64 connections that each ask GET /users/7 one after another for 3 s,
//!         and the requests a second they got
//!   held: 1000 connections open at once, each answered, and the memory the
//!         server's process holds for each one beyond what it held before

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const REQUEST: &[u8] = b"GET /users/7 HTTP/1.1\r\nhost: localhost\r\n\r\n";

/// Reads one response, whose body is as long as its content-length says.
fn reply(stream: &mut TcpStream, buffer: &mut Vec<u8>) {
    buffer.clear();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).expect("read");
        assert!(n > 0, "the server closed the connection");
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map(|v| v.trim().parse().unwrap())
                .unwrap_or(0);
            if buffer.len() >= end + 4 + length {
                assert!(head.starts_with("http/1.1 200"), "{head}");
                return;
            }
        }
    }
}

/// A field of /proc/PID/status, in KB.
fn kb(pid: &str, field: &str) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let line = status.lines().find(|l| l.starts_with(field)).unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn main() {
    let port: u16 = std::env::args().nth(1).expect("a port").parse().unwrap();
    let pid = std::env::args().nth(2).expect("a pid");
    let rate = {
        let end = Instant::now() + Duration::from_secs(3);
        let handles: Vec<_> = (0..64)
            .map(|_| {
                std::thread::spawn(move || {
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                    stream.set_nodelay(true).unwrap();
                    let mut buffer = Vec::new();
                    let mut count = 0u64;
                    while Instant::now() < end {
                        stream.write_all(REQUEST).unwrap();
                        reply(&mut stream, &mut buffer);
                        count += 1;
                    }
                    count
                })
            })
            .collect();
        let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        total as f64 / 3.0
    };
    println!("requests a second: {rate:.0}");
    std::thread::sleep(Duration::from_millis(500));
    let before = kb(&pid, "VmRSS:");
    let mut streams: Vec<TcpStream> = (0..1000).map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap()).collect();
    for stream in streams.iter_mut() {
        stream.write_all(REQUEST).unwrap();
    }
    let mut buffer = Vec::new();
    for stream in streams.iter_mut() {
        reply(stream, &mut buffer);
    }
    let held = kb(&pid, "VmRSS:");
    println!("1000 connections held open, each answered");
    println!("memory a connection: {} bytes of resident memory", (held.saturating_sub(before)) * 1024 / 1000);
}
