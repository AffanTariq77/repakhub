//! Download engine (M3, part 2): fetch and verify manifests, work out which files changed, and
//! download them into a staging folder. Blocking: callers run it on a worker thread.
//!
//! Every hop of every request is HTTPS to a host the signed manifest lists (redirects are followed
//! by hand so each one is checked). Downloads resume from `.part` files with HTTP range requests,
//! retry with backoff, stop at the manifest's size, and are SHA-256 verified before they leave
//! staging. Content that is an executable is refused unless the manifest's key is the rights holder's.

use crate::manifest::{self, FileEntry, Manifest};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use ureq::http::Response;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const ATTEMPTS: u32 = 3;
/// A transfer that receives nothing for this long has stalled. Only stalls time out: a large file on a
/// slow connection may take hours.
const STALL: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    Manifest(#[from] manifest::ManifestError),
    #[error("network error: {0}")]
    Network(String),
    #[error("{url}: unexpected HTTP status {status}")]
    Status { url: String, status: u16 },
    #[error("refused to fetch {0}: not https, or not a host the manifest lists")]
    Refused(String),
    #[error("{0} is larger than the manifest says")]
    Oversize(String),
    #[error("{0} failed SHA-256 verification")]
    Hash(String),
    #[error("{0} is an executable, which only the rights holder's key may ship")]
    Executable(String),
    #[error("not enough disk space: {need} bytes needed, {free} free")]
    Space { need: u64, free: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl DownloadError {
    /// Worth another attempt: the network or the server hiccupped, or a resumed file came out wrong.
    fn retryable(&self) -> bool {
        match self {
            DownloadError::Network(_) | DownloadError::Hash(_) => true,
            DownloadError::Status { status, .. } => *status >= 500 || *status == 429,
            _ => false,
        }
    }
}

pub struct Transport {
    agent: ureq::Agent,
    https_only: bool,
}

impl Transport {
    pub fn new() -> Self {
        Self::with_https_only(true)
    }

    /// `https_only: false` exists for tests against a local plain-HTTP server; manifests reject http anyway.
    fn with_https_only(https_only: bool) -> Self {
        Self::configured(https_only, STALL)
    }

    fn configured(https_only: bool, stall: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .max_redirects(0) // followed by hand in `get`, so each hop is checked
            .https_only(https_only)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            // Per read, not `timeout_recv_body`: that is a budget for the whole body and would cut off
            // every download that takes longer than it.
            .timeout_per_read(Some(stall))
            .build()
            .into();
        Transport { agent, https_only }
    }

    /// GET `url`, following up to `MAX_REDIRECTS` redirects; every hop must be https (unless testing)
    /// and, when `allowed` is given, on one of those hosts.
    fn get(&self, url: &str, allowed: Option<&[String]>, range_from: u64) -> Result<Response<ureq::Body>, DownloadError> {
        let mut url = url::Url::parse(url).map_err(|_| DownloadError::Refused(url.to_string()))?;
        for _ in 0..=MAX_REDIRECTS {
            let host_ok = allowed.is_none_or(|hosts| url.host_str().is_some_and(|h| hosts.iter().any(|a| a == h)));
            if (self.https_only && url.scheme() != "https") || !host_ok {
                return Err(DownloadError::Refused(url.to_string()));
            }
            let mut req = self.agent.get(url.as_str());
            if range_from > 0 {
                req = req.header("Range", format!("bytes={range_from}-"));
            }
            let resp = req.call().map_err(|e| DownloadError::Network(e.to_string()))?;
            if !resp.status().is_redirection() {
                return Ok(resp);
            }
            let location = resp.headers().get("location").and_then(|v| v.to_str().ok());
            url = location.and_then(|l| url.join(l).ok()).ok_or_else(|| DownloadError::Refused(url.to_string()))?;
        }
        Err(DownloadError::Refused(format!("{url} (too many redirects)")))
    }
}

/// Fetch a manifest envelope (capped at 1 MB) and verify it against the compiled-in keys.
pub fn fetch_manifest(t: &Transport, url: &str) -> Result<Manifest, DownloadError> {
    fetch_manifest_with(t, url, &manifest::trusted_keys())
}

fn fetch_manifest_with(t: &Transport, url: &str, keys: &[manifest::TrustedKey]) -> Result<Manifest, DownloadError> {
    Ok(manifest::verify_with(keys, &fetch_text(t, url, MAX_MANIFEST_BYTES)?)?)
}

/// GET a small text document over HTTPS, refusing anything larger than `max` bytes.
pub fn fetch_text(t: &Transport, url: &str, max: u64) -> Result<String, DownloadError> {
    let resp = t.get(url, None, 0)?;
    if resp.status().as_u16() != 200 {
        return Err(DownloadError::Status { url: url.into(), status: resp.status().as_u16() });
    }
    resp.into_body().with_config().limit(max).read_to_string().map_err(|e| DownloadError::Network(e.to_string()))
}

/// Files of `m` that are missing or differ from what is in `install_dir` (size first, then SHA-256).
pub fn plan<'m>(install_dir: &Path, m: &'m Manifest) -> io::Result<Vec<&'m FileEntry>> {
    let mut needed = vec![];
    for f in &m.files {
        let local = install_dir.join(&f.path);
        let same = local.metadata().is_ok_and(|md| md.is_file() && md.len() == f.size) && sha256_file(&local)? == f.sha256;
        if !same {
            needed.push(f);
        }
    }
    Ok(needed)
}

/// Refuse to start unless `dir`'s volume has room for `need` bytes plus 5%.
pub fn check_space(dir: &Path, need: u64) -> Result<(), DownloadError> {
    let free = fs4::available_space(dir)?;
    if free < need.saturating_add(need / 20) {
        return Err(DownloadError::Space { need, free });
    }
    Ok(())
}

/// Download `files` into `staging` with up to `workers` in parallel. Setting `cancel` stops all
/// workers; `.part` files stay, so calling again resumes. `progress(path, bytes_done)` is called as
/// data arrives. The first error stops the rest and is returned.
pub fn download_all(
    t: &Transport,
    m: &Manifest,
    files: &[&FileEntry],
    staging: &Path,
    workers: usize,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&str, u64) + Sync),
) -> Result<(), DownloadError> {
    fs::create_dir_all(staging)?;
    check_space(staging, files.iter().map(|f| f.size).sum())?;
    let queue = Mutex::new(files.iter());
    let first_error: Mutex<Option<DownloadError>> = Mutex::new(None);
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        for _ in 0..workers.max(1) {
            s.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let Some(f) = queue.lock().unwrap().next() else { break };
                    let halt = |p: &AtomicBool| p.load(Ordering::Relaxed);
                    let result = download_file(t, m, f, staging, &|| halt(cancel) || halt(&stop), &|done| progress(&f.path, done));
                    if let Err(e) = result {
                        stop.store(true, Ordering::Relaxed);
                        first_error.lock().unwrap().get_or_insert(e);
                    }
                }
            });
        }
    });
    match first_error.into_inner().unwrap() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Download one file into `staging`, retrying transient failures with backoff (1 s, 2 s).
pub fn download_file(
    t: &Transport,
    m: &Manifest,
    f: &FileEntry,
    staging: &Path,
    cancelled: &dyn Fn() -> bool,
    progress: &dyn Fn(u64),
) -> Result<PathBuf, DownloadError> {
    let dest = staging.join(&f.path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut part = OsString::from(dest.as_os_str());
    part.push(".part");
    let part = PathBuf::from(part);

    let mut attempt = 0;
    loop {
        match try_download(t, m, f, &part, cancelled, progress) {
            Ok(()) => break,
            Err(e) if e.retryable() && attempt + 1 < ATTEMPTS => {
                std::thread::sleep(Duration::from_secs(1 << attempt));
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
    fs::rename(&part, &dest)?;
    Ok(dest)
}

fn try_download(
    t: &Transport,
    m: &Manifest,
    f: &FileEntry,
    part: &Path,
    cancelled: &dyn Fn() -> bool,
    progress: &dyn Fn(u64),
) -> Result<(), DownloadError> {
    let mut have = part.metadata().map(|md| md.len()).unwrap_or(0);
    if have > f.size {
        fs::remove_file(part)?;
        have = 0;
    }
    if have < f.size {
        let mut resp = t.get(&f.url, Some(&m.hosts), have)?;
        let status = resp.status().as_u16();
        let resumed = status == 206
            && resp.headers().get("content-range").and_then(|v| v.to_str().ok()).is_some_and(|r| r.starts_with(&format!("bytes {have}-")));
        if !resumed {
            if status != 200 {
                return Err(DownloadError::Status { url: f.url.clone(), status });
            }
            have = 0; // the server sent the whole file: start over
        }
        let mut out = OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(part)?;
        let mut body = resp.body_mut().as_reader();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if cancelled() {
                return Err(DownloadError::Cancelled);
            }
            let n = body.read(&mut buf).map_err(|e| DownloadError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            have += n as u64;
            if have > f.size {
                drop(out);
                fs::remove_file(part)?;
                return Err(DownloadError::Oversize(f.path.clone()));
            }
            out.write_all(&buf[..n])?;
            progress(have);
        }
        if have < f.size {
            return Err(DownloadError::Network(format!("{}: connection closed early", f.path)));
        }
    }
    if sha256_file(part)? != f.sha256 {
        fs::remove_file(part)?; // retry starts clean
        return Err(DownloadError::Hash(f.path.clone()));
    }
    if !m.rights_holder {
        let mut head = [0u8; 4];
        let n = File::open(part)?.read(&mut head)?;
        if manifest::is_executable_content(&head[..n]) {
            fs::remove_file(part)?;
            return Err(DownloadError::Executable(f.path.clone()));
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// What the test server does with a request: (status line, extra headers, body).
    type Handler = dyn Fn(&str, Option<u64>) -> (String, Vec<String>, Vec<u8>) + Send + Sync;

    /// A tiny HTTP/1.1 server on 127.0.0.1. Returns the base URL and a request counter.
    fn serve(handler: Arc<Handler>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut stream = stream;
                let mut req = vec![0u8; 8192];
                let n = stream.read(&mut req).unwrap_or(0);
                let req = String::from_utf8_lossy(&req[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let range = req
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.trim_end_matches('-').parse().ok()))
                    .flatten();
                let (status, headers, body) = handler(&path, range);
                let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
                for h in headers {
                    head.push_str(&h);
                    head.push_str("\r\n");
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        (base, hits)
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn entry(base: &str, path: &str, content: &[u8]) -> FileEntry {
        FileEntry { path: path.into(), size: content.len() as u64, sha256: sha(content), url: format!("{base}/{path}") }
    }

    fn manifest_for(files: Vec<FileEntry>, rights_holder: bool) -> Manifest {
        Manifest {
            schema: 1,
            source: "test-source".into(),
            game_id: "g".into(),
            name: "G".into(),
            version: "1.0".into(),
            changelog: String::new(),
            hosts: vec!["127.0.0.1".into()],
            files,
            rights_holder,
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repakhub-dl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Serves `content` at every path, honouring Range unless `ignore_range`.
    fn file_server(content: Vec<u8>, ignore_range: bool) -> (String, Arc<AtomicUsize>) {
        serve(Arc::new(move |_, range| match range.filter(|_| !ignore_range) {
            Some(from) => (
                "206 Partial Content".into(),
                vec![format!("Content-Range: bytes {from}-{}/{}", content.len() - 1, content.len())],
                content[from as usize..].to_vec(),
            ),
            None => ("200 OK".into(), vec![], content.clone()),
        }))
    }

    const NEVER: &dyn Fn() -> bool = &|| false;

    /// Serves `content` honouring Range, in `chunk`-byte writes with `gap` between them, and records where
    /// each request asked to start. `cut: Some((request, bytes))` drops that request (0-based) after
    /// `bytes`, having promised the whole file, the way a dropped connection looks to the client.
    fn stream_server(content: Vec<u8>, chunk: usize, gap: Duration, cut: Option<(usize, usize)>) -> (String, Arc<Mutex<Vec<u64>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let starts = Arc::new(Mutex::new(Vec::new()));
        let seen = starts.clone();
        std::thread::spawn(move || {
            for (i, stream) in listener.incoming().flatten().enumerate() {
                let mut stream = stream;
                let mut req = vec![0u8; 8192];
                let n = stream.read(&mut req).unwrap_or(0);
                let from: usize = String::from_utf8_lossy(&req[..n])
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.trim_end_matches('-').parse().unwrap_or(0)))
                    .unwrap_or(0);
                seen.lock().unwrap().push(from as u64);
                let body = &content[from..];
                let status = if from > 0 {
                    format!("206 Partial Content\r\nContent-Range: bytes {from}-{}/{}", content.len() - 1, content.len())
                } else {
                    "200 OK".into()
                };
                let _ = stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                let limit = cut.filter(|(req, _)| *req == i).map_or(body.len(), |(_, bytes)| bytes.min(body.len()));
                for piece in body[..limit].chunks(chunk) {
                    if stream.write_all(piece).is_err() {
                        break;
                    }
                    std::thread::sleep(gap);
                }
            }
        });
        (base, starts)
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn interrupted_downloads_resume_where_they_stopped() {
        let content = pattern(400_000);
        let (base, starts) = stream_server(content.clone(), 16 * 1024, Duration::ZERO, Some((0, 150_000)));
        let staging = tmp("interrupted");
        let t = Transport::with_https_only(false);
        let path = download_file(&t, &manifest_for(vec![], false), &entry(&base, "a.pak", &content), &staging, NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(path).unwrap(), content);
        let starts = starts.lock().unwrap().clone();
        assert_eq!(starts.len(), 2, "one retry");
        assert!(starts[0] == 0 && starts[1] > 0 && starts[1] <= 150_000, "the retry asked for the rest, not the whole file: {starts:?}");
    }

    #[test]
    fn paused_downloads_keep_what_they_have_and_resume() {
        let content = pattern(1024 * 1024);
        let (base, starts) = stream_server(content.clone(), 64 * 1024, Duration::from_millis(5), None);
        let staging = tmp("paused");
        let (t, m, f) = (Transport::with_https_only(false), manifest_for(vec![], false), entry(&base, "a.pak", &content));
        let pause = AtomicBool::new(false);
        let paused = download_file(&t, &m, &f, &staging, &|| pause.load(Ordering::SeqCst), &|done| {
            if done >= 128 * 1024 {
                pause.store(true, Ordering::SeqCst);
            }
        });
        assert!(matches!(paused, Err(DownloadError::Cancelled)));
        let kept = fs::metadata(staging.join("a.pak.part")).unwrap().len();
        assert!((128 * 1024..content.len() as u64).contains(&kept), "partial file kept: {kept}");

        let path = download_file(&t, &m, &f, &staging, NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(path).unwrap(), content);
        assert_eq!(starts.lock().unwrap()[1], kept, "resumed from the kept bytes");
    }

    #[test]
    fn a_corrupted_partial_file_is_caught_and_downloaded_again() {
        let content = pattern(200_000);
        let (base, starts) = stream_server(content.clone(), 16 * 1024, Duration::ZERO, None);
        let staging = tmp("corrupt-part");
        fs::write(staging.join("a.pak.part"), vec![0xAA; 100_000]).unwrap(); // right length, wrong bytes
        let t = Transport::with_https_only(false);
        let path = download_file(&t, &manifest_for(vec![], false), &entry(&base, "a.pak", &content), &staging, NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(path).unwrap(), content, "the resumed file failed verification, so it started over");
        assert_eq!(*starts.lock().unwrap(), [100_000, 0]);
    }

    #[test]
    fn slow_downloads_continue_and_only_stalls_time_out() {
        let content = pattern(200_000);
        let stall = Duration::from_millis(400);
        let t = Transport::configured(false, stall);
        let m = manifest_for(vec![], false);

        // About 1.5 s in total, more than the stall limit several times over, but never quiet for long.
        let (base, _) = stream_server(content.clone(), 20_000, Duration::from_millis(150), None);
        let path = download_file(&t, &m, &entry(&base, "slow.pak", &content), &tmp("slow"), NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(path).unwrap(), content);

        // Quiet for longer than the limit: a network error, which download_file retries.
        let (base, _) = stream_server(content.clone(), 100_000, Duration::from_secs(2), None);
        let part = tmp("stalled").join("stalled.pak.part");
        let stalled = try_download(&t, &m, &entry(&base, "stalled.pak", &content), &part, NEVER, &|_| {});
        assert!(matches!(&stalled, Err(e @ DownloadError::Network(_)) if e.retryable()), "{stalled:?}");
        assert_eq!(fs::metadata(&part).unwrap().len(), 100_000, "what arrived before the stall is kept for the retry");
    }

    #[test]
    fn large_downloads_stream_to_disk_with_progress() {
        let content = pattern(32 * 1024 * 1024);
        let (base, _) = stream_server(content.clone(), 1024 * 1024, Duration::ZERO, None);
        let staging = tmp("large");
        let m = manifest_for(vec![entry(&base, "big.pak", &content)], false);
        let last = std::sync::atomic::AtomicU64::new(0);
        let monotonic = AtomicBool::new(true);
        let files: Vec<&FileEntry> = m.files.iter().collect();
        download_all(&Transport::with_https_only(false), &m, &files, &staging, 2, &AtomicBool::new(false), &|_, done| {
            if done < last.swap(done, Ordering::SeqCst) {
                monotonic.store(false, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert!(monotonic.load(Ordering::SeqCst), "progress never goes backwards");
        assert_eq!(last.load(Ordering::SeqCst), content.len() as u64);
        assert_eq!(sha256_file(&staging.join("big.pak")).unwrap(), sha(&content));
    }

    #[test]
    fn downloads_resumes_and_restarts() {
        let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let t = Transport::with_https_only(false);

        // Resume: half already on disk, server honours Range.
        let (base, _) = file_server(content.clone(), false);
        let staging = tmp("resume");
        let f = entry(&base, "data/a.pak", &content);
        fs::create_dir_all(staging.join("data")).unwrap();
        fs::write(staging.join("data/a.pak.part"), &content[..100_000]).unwrap();
        let path = download_file(&t, &manifest_for(vec![], false), &f, &staging, NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(&path).unwrap(), content);
        assert!(!staging.join("data/a.pak.part").exists());

        // Server ignores Range and sends everything: the part file is restarted, not appended to.
        let (base, _) = file_server(content.clone(), true);
        let staging = tmp("restart");
        let f = entry(&base, "a.pak", &content);
        fs::write(staging.join("a.pak.part"), &content[..50_000]).unwrap();
        let path = download_file(&t, &manifest_for(vec![], false), &f, &staging, NEVER, &|_| {}).unwrap();
        assert_eq!(fs::read(&path).unwrap(), content);
    }

    #[test]
    fn refuses_oversize_bad_hash_and_executables() {
        let t = Transport::with_https_only(false);
        let m = manifest_for(vec![], false);

        let (base, _) = file_server(b"0123456789".to_vec(), false);
        let staging = tmp("bad");
        let mut small = entry(&base, "a.pak", b"01234");
        small.sha256 = sha(b"01234");
        assert!(matches!(download_file(&t, &m, &small, &staging, NEVER, &|_| {}), Err(DownloadError::Oversize(_))));
        assert!(!staging.join("a.pak.part").exists(), "oversize part removed");

        let mut wrong = entry(&base, "b.pak", b"0123456789");
        wrong.sha256 = sha(b"something else");
        assert!(matches!(download_file(&t, &m, &wrong, &staging, NEVER, &|_| {}), Err(DownloadError::Hash(_))));
        assert!(!staging.join("b.pak").exists() && !staging.join("b.pak.part").exists(), "nothing unverified kept");

        let elf = b"\x7fELF fake binary".to_vec();
        let (base, _) = file_server(elf.clone(), false);
        let disguised = entry(&base, "data/level.dat", &elf);
        assert!(matches!(download_file(&t, &m, &disguised, &staging, NEVER, &|_| {}), Err(DownloadError::Executable(_))));
        let ok = download_file(&t, &manifest_for(vec![], true), &disguised, &staging, NEVER, &|_| {});
        assert!(ok.is_ok(), "the rights holder may ship executables");
    }

    #[test]
    fn redirects_only_to_listed_hosts_and_never_to_http_in_production() {
        let content = b"payload".to_vec();
        let (target, _) = file_server(content.clone(), false);
        let to_listed = target.clone();
        let (base, _) = serve(Arc::new(move |path, _| {
            let dest = if path.contains("evil") { "http://evil.example.com/x".to_string() } else { format!("{to_listed}/real") };
            ("302 Found".into(), vec![format!("Location: {dest}")], vec![])
        }));
        let t = Transport::with_https_only(false);
        let m = manifest_for(vec![], false);
        let staging = tmp("redirect");
        let mut f = entry(&base, "a.pak", &content);
        assert!(download_file(&t, &m, &f, &staging, NEVER, &|_| {}).is_ok(), "redirect to a listed host followed");
        f.url = format!("{base}/evil");
        f.path = "b.pak".into();
        assert!(matches!(download_file(&t, &m, &f, &staging, NEVER, &|_| {}), Err(DownloadError::Refused(_))));

        let production = Transport::new();
        assert!(matches!(production.get(&format!("{target}/x"), None, 0), Err(DownloadError::Refused(_))), "http refused");
    }

    #[test]
    fn plans_only_changed_files_and_downloads_in_parallel() {
        let a = b"same".to_vec();
        let b: Vec<u8> = vec![7; 5000];
        let (base, hits) = serve(Arc::new({
            let (a, b) = (a.clone(), b.clone());
            move |path, _| ("200 OK".into(), vec![], if path.ends_with("a.pak") { a.clone() } else { b.clone() })
        }));
        let install = tmp("install");
        fs::write(install.join("a.pak"), &a).unwrap();
        fs::write(install.join("b.pak"), b"old").unwrap();
        let m = manifest_for(vec![entry(&base, "a.pak", &a), entry(&base, "b.pak", &b), entry(&base, "c.pak", &b)], false);

        let needed = plan(&install, &m).unwrap();
        assert_eq!(needed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["b.pak", "c.pak"]);

        let staging = install.join(".repakhub/staging");
        let t = Transport::with_https_only(false);
        let seen = Mutex::new(Vec::new());
        download_all(&t, &m, &needed, &staging, 2, &AtomicBool::new(false), &|p, _| seen.lock().unwrap().push(p.to_string())).unwrap();
        assert_eq!(fs::read(staging.join("c.pak")).unwrap(), b);
        assert_eq!(hits.load(Ordering::SeqCst), 2, "unchanged file not downloaded");
        assert!(seen.lock().unwrap().iter().any(|p| p == "b.pak"));

        let cancelled = download_all(&t, &m, &needed, &tmp("cancel"), 2, &AtomicBool::new(true), &|_, _| {});
        assert!(matches!(cancelled, Err(DownloadError::Cancelled)));
        assert!(matches!(check_space(&install, u64::MAX / 2), Err(DownloadError::Space { .. })));
    }

    #[test]
    fn fetches_and_verifies_manifests_with_a_size_cap() {
        let env = manifest::tests::envelope(&manifest::tests::manifest(serde_json::json!([manifest::tests::file("a.pak")])));
        let big = "x".repeat(MAX_MANIFEST_BYTES as usize + 10);
        let (base, _) = serve(Arc::new(move |path, _| {
            ("200 OK".into(), vec![], if path == "/big" { big.clone().into_bytes() } else { env.clone().into_bytes() })
        }));
        let t = Transport::with_https_only(false);
        let keys = manifest::tests::trusted(false);
        assert_eq!(fetch_manifest_with(&t, &format!("{base}/m"), &keys).unwrap().version, "1.1.0");
        assert!(matches!(fetch_manifest_with(&t, &format!("{base}/big"), &keys), Err(DownloadError::Network(_))));
        assert!(matches!(fetch_manifest(&t, &format!("{base}/m")), Err(DownloadError::Manifest(_))), "untrusted by default");
    }
}
