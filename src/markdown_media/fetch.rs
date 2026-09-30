//! Acquiring media bytes: local files, http(s) with an on-disk cache, Mermaid source.
//!
//! Everything here blocks, so it is only called from background tasks, never from a frame.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{FetchEnv, FileStamp, MediaError, MediaKind, MediaSource};

/// Bytes fetched for one source, the detected kind and the file stamp (local files only).
type Fetched = (MediaKind, Vec<u8>, Option<FileStamp>);

pub(crate) fn fetch_bytes(source: &MediaSource, env: &FetchEnv) -> Result<Fetched, MediaError> {
    match source {
        MediaSource::File(path) => fetch_file(path, env.max_bytes),
        MediaSource::Url(url) => fetch_url(url, env),
        MediaSource::Mermaid(code) => Ok((MediaKind::Mermaid, code.as_bytes().to_vec(), None)),
    }
}

fn fetch_file(path: &Path, max_bytes: u64) -> Result<Fetched, MediaError> {
    let meta = std::fs::metadata(path).map_err(|_| MediaError::NotFound)?;
    if !meta.is_file() {
        return Err(MediaError::NotFound);
    }
    if meta.len() > max_bytes {
        return Err(MediaError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|_| MediaError::NotFound)?;
    let stamp = FileStamp {
        mtime: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        len: meta.len(),
    };
    let svg_extension = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
    let kind = if svg_extension || looks_like_svg(&bytes) {
        MediaKind::Svg
    } else {
        MediaKind::Raster
    };
    Ok((kind, bytes, Some(stamp)))
}

fn fetch_url(url: &str, env: &FetchEnv) -> Result<Fetched, MediaError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| MediaError::Unsupported)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(MediaError::Unsupported);
    }
    let cache_path = cache_file_path(&env.cache_dir, url);
    if let Some(bytes) = read_cached(&cache_path, env.max_bytes) {
        let kind = if looks_like_svg(&bytes) {
            MediaKind::Svg
        } else {
            MediaKind::Raster
        };
        return Ok((kind, bytes, None));
    }
    let response = env
        .http
        .get(parsed)
        .send()
        .map_err(|_| MediaError::Timeout)?;
    let status = response.status();
    if !status.is_success() {
        return Err(MediaError::Http(status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > env.max_bytes)
    {
        return Err(MediaError::TooLarge);
    }
    let svg_content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim_start().to_ascii_lowercase().starts_with("image/svg+xml"));
    let mut bytes = Vec::new();
    response
        .take(env.max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| MediaError::Timeout)?;
    if bytes.len() as u64 > env.max_bytes {
        return Err(MediaError::TooLarge);
    }
    // A cache write failure must not fail the load.
    let _ = write_cache_atomic(&cache_path, &bytes);
    let kind = if svg_content_type || looks_like_svg(&bytes) {
        MediaKind::Svg
    } else {
        MediaKind::Raster
    };
    Ok((kind, bytes, None))
}

/// `<svg` or `<?xml` after optional BOM and whitespace.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let start = head
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(head.len());
    let head = &head[start..];
    head.starts_with(b"<svg") || head.starts_with(b"<?xml")
}

/// FNV-1a 64. Unlike `DefaultHasher` it is stable across compiler versions, so cache file
/// names survive upgrades.
fn fnv1a_64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn cache_file_path(cache_dir: &Path, url: &str) -> PathBuf {
    cache_dir.join(format!("{:016x}", fnv1a_64(url)))
}

fn read_cached(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max_bytes {
        return None;
    }
    std::fs::read(path).ok()
}

fn write_cache_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp_path = dir.join(temp_name);
    let result = std::fs::write(&temp_path, bytes).and_then(|()| std::fs::rename(&temp_path, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Deletes the oldest files (by mtime) until the directory is within `limit_bytes`.
/// Errors are ignored: the cache is best effort.
pub(crate) fn trim_disk_cache(dir: &Path, limit_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        total = total.saturating_add(meta.len());
        files.push((mtime, meta.len(), entry.path()));
    }
    if total <= limit_bytes {
        return;
    }
    files.sort_by_key(|(mtime, _, _)| *mtime);
    for (_, len, path) in files {
        if total <= limit_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown_media::RenderCommand;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nnot a real image";

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rriter-md-fetch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    fn test_env(cache_dir: PathBuf, max_bytes: u64, timeout: Duration) -> FetchEnv {
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .expect("http client");
        FetchEnv {
            cache_dir,
            http,
            max_bytes,
            render: Some(RenderCommand::InProcess),
        }
    }

    fn http_response(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
        let mut head = format!("HTTP/1.1 {status}\r\n");
        for header in headers {
            head.push_str(header);
            head.push_str("\r\n");
        }
        head.push_str("Connection: close\r\n\r\n");
        let mut response = head.into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn read_request_head(stream: &mut TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut seen = Vec::new();
        let mut chunk = [0u8; 512];
        while !seen.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => seen.extend_from_slice(&chunk[..read]),
            }
        }
    }

    /// Serves one connection on a loopback port, then drops the listener.
    fn serve_once(response: Vec<u8>, delay: Duration) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                read_request_head(&mut stream);
                thread::sleep(delay);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}/image"), handle)
    }

    fn url_source(url: &str) -> MediaSource {
        MediaSource::Url(url.to_string())
    }

    #[test]
    fn file_png_bytes_are_raster_with_stamp() {
        let dir = test_dir("file-png");
        let path = dir.join("a.png");
        std::fs::write(&path, PNG_BYTES).expect("write");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        let (kind, bytes, stamp) = fetch_bytes(&MediaSource::File(path), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Raster);
        assert_eq!(bytes, PNG_BYTES);
        let stamp = stamp.expect("stamp");
        assert_eq!(stamp.len, PNG_BYTES.len() as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_svg_by_extension_and_by_signature() {
        let dir = test_dir("file-svg");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        let by_extension = dir.join("a.svg");
        std::fs::write(&by_extension, b"whatever").expect("write");
        let (kind, _, _) = fetch_bytes(&MediaSource::File(by_extension), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Svg);
        let by_signature = dir.join("noext");
        std::fs::write(&by_signature, b"\xEF\xBB\xBF  \n<svg xmlns=\"x\"></svg>").expect("write");
        let (kind, _, _) = fetch_bytes(&MediaSource::File(by_signature), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Svg);
        let xml_prolog = dir.join("prolog");
        std::fs::write(&xml_prolog, b"<?xml version=\"1.0\"?><svg/>").expect("write");
        let (kind, _, _) = fetch_bytes(&MediaSource::File(xml_prolog), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Svg);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mermaid_source_is_passed_through() {
        let dir = test_dir("mermaid");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        let (kind, bytes, stamp) =
            fetch_bytes(&MediaSource::Mermaid("graph TD; A-->B".to_string()), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Mermaid);
        assert_eq!(bytes, b"graph TD; A-->B");
        assert!(stamp.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = test_dir("file-missing");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        let result = fetch_bytes(&MediaSource::File(dir.join("nope.png")), &env);
        assert_eq!(result.err(), Some(MediaError::NotFound));
        let directory = fetch_bytes(&MediaSource::File(dir.clone()), &env);
        assert_eq!(directory.err(), Some(MediaError::NotFound));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_over_limit_is_too_large() {
        let dir = test_dir("file-large");
        let path = dir.join("big.png");
        std::fs::write(&path, vec![7u8; 1025]).expect("write");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        let result = fetch_bytes(&MediaSource::File(path.clone()), &env);
        assert_eq!(result.err(), Some(MediaError::TooLarge));
        std::fs::write(&path, vec![7u8; 1024]).expect("write");
        assert!(fetch_bytes(&MediaSource::File(path), &env).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_ok_is_cached_and_served_after_server_stops() {
        let dir = test_dir("http-ok");
        let cache_dir = dir.join("cache");
        let env = test_env(cache_dir.clone(), 1024, Duration::from_secs(2));
        let response = http_response(
            "200 OK",
            &[&format!("Content-Length: {}", PNG_BYTES.len()), "Content-Type: image/png"],
            PNG_BYTES,
        );
        let (url, server) = serve_once(response, Duration::ZERO);
        let (kind, bytes, stamp) = fetch_bytes(&url_source(&url), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Raster);
        assert_eq!(bytes, PNG_BYTES);
        assert!(stamp.is_none());
        server.join().expect("server thread");
        let cached = cache_file_path(&cache_dir, &url);
        assert_eq!(std::fs::read(&cached).expect("cache file"), PNG_BYTES);
        let entries: Vec<_> = std::fs::read_dir(&cache_dir).expect("read_dir").flatten().collect();
        assert_eq!(entries.len(), 1, "no temporary files are left behind");
        // The listener is gone: a second fetch can only succeed from the disk cache.
        let (_, again, _) = fetch_bytes(&url_source(&url), &env).expect("cached fetch");
        assert_eq!(again, PNG_BYTES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_svg_content_type_is_svg() {
        let dir = test_dir("http-svg");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(2));
        let body = b"opaque bytes";
        let response = http_response(
            "200 OK",
            &[
                &format!("Content-Length: {}", body.len()),
                "Content-Type: image/svg+xml; charset=utf-8",
            ],
            body,
        );
        let (url, server) = serve_once(response, Duration::ZERO);
        let (kind, _, _) = fetch_bytes(&url_source(&url), &env).expect("fetch");
        assert_eq!(kind, MediaKind::Svg);
        server.join().expect("server thread");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_404_is_http_error() {
        let dir = test_dir("http-404");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(2));
        let response = http_response("404 Not Found", &["Content-Length: 0"], b"");
        let (url, server) = serve_once(response, Duration::ZERO);
        let result = fetch_bytes(&url_source(&url), &env);
        assert_eq!(result.err(), Some(MediaError::Http(404)));
        server.join().expect("server thread");
        assert!(
            !cache_file_path(&dir.join("cache"), &url).exists(),
            "errors are not cached"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_content_length_over_limit_is_too_large() {
        let dir = test_dir("http-length");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(2));
        let response = http_response("200 OK", &["Content-Length: 5000"], b"");
        let (url, server) = serve_once(response, Duration::ZERO);
        let result = fetch_bytes(&url_source(&url), &env);
        assert_eq!(result.err(), Some(MediaError::TooLarge));
        server.join().expect("server thread");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_body_over_limit_without_length_is_too_large() {
        let dir = test_dir("http-stream");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(2));
        let response = http_response("200 OK", &[], &vec![9u8; 4000]);
        let (url, server) = serve_once(response, Duration::ZERO);
        let result = fetch_bytes(&url_source(&url), &env);
        assert_eq!(result.err(), Some(MediaError::TooLarge));
        server.join().expect("server thread");
        assert!(
            !cache_file_path(&dir.join("cache"), &url).exists(),
            "oversized bodies are not cached"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_silent_server_is_timeout() {
        let dir = test_dir("http-timeout");
        let env = test_env(dir.join("cache"), 1024, Duration::from_millis(300));
        let response = http_response("200 OK", &["Content-Length: 0"], b"");
        let (url, _server) = serve_once(response, Duration::from_millis(1200));
        let result = fetch_bytes(&url_source(&url), &env);
        assert_eq!(result.err(), Some(MediaError::Timeout));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_http_and_garbage_urls_are_unsupported() {
        let dir = test_dir("unsupported");
        let env = test_env(dir.join("cache"), 1024, Duration::from_secs(1));
        for url in [
            "data:image/png;base64,AAAA",
            "file:///etc/passwd",
            "ftp://example.com/a.png",
            "not a url at all",
            "",
        ] {
            let result = fetch_bytes(&url_source(url), &env);
            assert_eq!(result.err(), Some(MediaError::Unsupported), "url {url:?}");
        }
        assert!(!env.cache_dir.exists(), "rejected URLs touch nothing on disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_file_name_is_stable_16_hex() {
        assert_eq!(fnv1a_64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64("a"), 0xaf63_dc4c_8601_ec8c);
        let dir = Path::new("/cache");
        let first = cache_file_path(dir, "https://example.com/a.png");
        let second = cache_file_path(dir, "https://example.com/a.png");
        let other = cache_file_path(dir, "https://example.com/b.png");
        assert_eq!(first, second);
        assert_ne!(first, other);
        let name = first.file_name().and_then(|name| name.to_str()).expect("name");
        assert_eq!(name.len(), 16);
        assert!(name.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    }

    #[test]
    fn trim_disk_cache_keeps_newest_files_within_limit() {
        let dir = test_dir("trim");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        for (index, name) in ["old", "mid", "new"].iter().enumerate() {
            let path = dir.join(name);
            std::fs::write(&path, vec![1u8; 100]).expect("write");
            let file = std::fs::OpenOptions::new().write(true).open(&path).expect("open");
            file.set_modified(base + Duration::from_secs(index as u64 * 10))
                .expect("set mtime");
        }
        trim_disk_cache(&dir, 250);
        assert!(!dir.join("old").exists());
        assert!(dir.join("mid").exists());
        assert!(dir.join("new").exists());
        trim_disk_cache(&dir, 1000);
        assert!(dir.join("mid").exists() && dir.join("new").exists());
        trim_disk_cache(&dir, 0);
        assert!(!dir.join("mid").exists() && !dir.join("new").exists());
        trim_disk_cache(&dir.join("missing"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn error_labels_are_russian_texts() {
        let cases = [
            (MediaError::NotFound, "не найдено"),
            (MediaError::Http(404), "HTTP 404"),
            (MediaError::Timeout, "таймаут"),
            (MediaError::TooLarge, "больше 20 МБ"),
            (MediaError::TooManyPixels, "слишком большая"),
            (MediaError::Decode, "не удалось декодировать"),
            (MediaError::Unsupported, "не поддерживается"),
            (MediaError::Mermaid("parse error".to_string()), "parse error"),
            (MediaError::Crashed, "рендер упал"),
        ];
        for (error, label) in cases {
            assert_eq!(error.label(), label);
        }
    }
}
