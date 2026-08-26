// Throwaway /health daemon — plan 002 POC only. Replaced by the real
// reload-test change 1
// fs3-daemon in phase 2. Zero dependencies: std TCP listener, hand-rolled
// HTTP. GET /health -> 200 {"status":"ok"}; anything else -> 404.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const BIND: &str = "0.0.0.0:8081";

fn handle(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf);
    let req = String::from_utf8_lossy(&buf);
    let ok = req.starts_with("GET /health");
    let body = if ok { "{\"status\":\"ok\"}" } else { "" };
    let status = if ok { "200 OK" } else { "404 Not Found" };
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
}

fn main() {
    let listener = TcpListener::bind(BIND).expect("bind 0.0.0.0:8081");
    eprintln!("fs3-poc-daemon listening on {BIND}");
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            std::thread::spawn(move || handle(s));
        }
    }
}
