//! Shared fixtures for private-source tests.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// A request the mock saw: path and the Authorization header, if any.
pub type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

pub struct Mock {
    pub base: String,
    pub host: String,
    pub seen: Seen,
}

/// An in-process HTTP server. `route(path, authorization)` returns
/// (status, content type, body, extra headers).
pub fn serve<F>(route: F) -> Mock
where
    F: Fn(&str, Option<&str>) -> (u16, String, Vec<u8>, Vec<(String, String)>) + Send + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Default::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut c) = stream else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = c.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf).to_string();
            let path = head.lines().next().unwrap_or("").split(' ').nth(1).unwrap_or("").to_string();
            let auth = head.lines().find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                    .map(|(_, v)| v.trim().to_string())
            });
            log.lock().unwrap().push((path.clone(), auth.clone()));
            let (status, ctype, body, extra) = route(&path, auth.as_deref());
            let mut resp = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (k, v) in extra {
                resp.push_str(&format!("{k}: {v}\r\n"));
            }
            resp.push_str("\r\n");
            let _ = c.write_all(resp.as_bytes());
            let _ = c.write_all(&body);
            let _ = c.flush();
            let _ = c.shutdown(std::net::Shutdown::Write);
        }
    });
    Mock {
        base: format!("http://{addr}"),
        host: addr.to_string(),
        seen,
    }
}

pub fn tgz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
    for (n, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append_data(&mut h, n, *data).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

/// A zip of `files`, for a wheel or sdist artifact in a test.
pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn write(p: &std::path::Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}
