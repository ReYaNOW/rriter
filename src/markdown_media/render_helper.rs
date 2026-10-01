//! The media helper process: wire protocol, spawning and the helper-side entry point.
//!
//! All decoding runs in a child process of the editor executable (`--rriter-media-render`),
//! so a crash or abort in a decoder cannot take the editor down. Protocol:
//!
//! * stdin: `<kind> <scale> <max_raster_w> <len>\n`, then exactly `len` bytes of input;
//! * stdout on success: `OK <natural_w> <natural_h> <raster_w> <raster_h>\n`, then
//!   `raster_w * raster_h * 4` bytes of RGBA8 (straight alpha);
//! * stdout on a handled error: `ERR <code> <text up to 200 chars>\n`, exit code 0.
//!
//! Anything else (non-zero exit, signal, truncated or inconsistent output) is `Crashed`,
//! exceeding the deadline is `Timeout`. The reply is untrusted and validated before any
//! pixel buffer is allocated.

use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::decode::{decode_in_process, prepare_mermaid_font_cache};
use super::{MediaError, MediaKind, MediaPixels, RenderCommand};
use crate::platform::{self, ManagedChild};

const HELPER_FLAG: &str = "--rriter-media-render";
/// Largest input the helper accepts; longer input is `TooLarge` without spawning anything.
const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;
/// The decoder keeps the long side of a raster within this many pixels.
const MAX_OUTPUT_SIDE: u32 = 4096;
/// Longest header line accepted in either direction.
const HEADER_LIMIT: u64 = 1024;
const ERR_TEXT_LIMIT: usize = 200;
const TERMINATE_GRACE: Duration = Duration::from_millis(200);
const EXIT_POLL: Duration = Duration::from_millis(10);

impl RenderCommand {
    /// Command that re-runs this executable as the helper. `None` when the executable path
    /// is unknown; the owner then reports `Unsupported` for every load.
    pub(crate) fn for_current_process() -> Option<RenderCommand> {
        // The test binary is not rriter, it cannot act as the helper.
        #[cfg(test)]
        {
            Some(RenderCommand::InProcess)
        }
        #[cfg(not(test))]
        {
            std::env::current_exe().ok().map(|exe| RenderCommand::Process {
                exe,
                prefix_args: Vec::new(),
            })
        }
    }
}

/// Decodes `bytes` through the helper described by `cmd`.
pub(crate) fn render_media(
    cmd: &RenderCommand,
    kind: MediaKind,
    bytes: &[u8],
    scale: f32,
    max_raster_w: u32,
    timeout: Duration,
) -> Result<MediaPixels, MediaError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(MediaError::TooLarge);
    }
    match cmd {
        RenderCommand::InProcess => decode_in_process(kind, bytes, scale, max_raster_w),
        RenderCommand::Process { exe, prefix_args } => {
            let mermaid_cache = (kind == MediaKind::Mermaid)
                .then(|| platform::cache_dir().join("markdown-mermaid"));
            let mut command = helper_command(exe, prefix_args, mermaid_cache.as_deref())?;
            exchange(&mut command, kind, bytes, scale, max_raster_w, timeout)
        }
    }
}

/// Builds the helper command. For Mermaid the font cache of mermaid-rs-renderer is seeded
/// first and the child alone gets `XDG_CACHE_HOME` pointing at it, so the crate never scans
/// system fonts; the editor's own environment is not touched.
fn helper_command(
    exe: &Path,
    prefix_args: &[String],
    mermaid_cache: Option<&Path>,
) -> Result<Command, MediaError> {
    let mut command = Command::new(exe);
    command.args(prefix_args).arg(HELPER_FLAG);
    if let Some(cache_root) = mermaid_cache {
        prepare_mermaid_font_cache(cache_root).map_err(|error| {
            MediaError::Mermaid(format!("кэш шрифтов Mermaid: {}", error.kind()))
        })?;
        command.env("XDG_CACHE_HOME", cache_root);
    }
    Ok(command)
}

/// How the wait on the child ended.
enum Wait {
    Exited(std::process::ExitStatus),
    TimedOut,
    BadOutput,
    Failed,
}

/// Runs `command` as a helper: the request goes to stdin from its own thread while another
/// thread reads the reply from stdout (input up to 20 MB, output up to 64 MB: doing both on
/// one thread would deadlock on full pipes). The main thread owns the deadline.
fn exchange(
    command: &mut Command,
    kind: MediaKind,
    bytes: &[u8],
    scale: f32,
    max_raster_w: u32,
    timeout: Duration,
) -> Result<MediaPixels, MediaError> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = ManagedChild::spawn(command).map_err(|_| MediaError::Crashed)?;
    let (Some(stdin), Some(stdout)) = (child.take_stdin(), child.take_stdout()) else {
        let _ = child.terminate(TERMINATE_GRACE);
        return Err(MediaError::Crashed);
    };
    let header = format!("{} {} {} {}\n", kind_name(kind), scale, max_raster_w, bytes.len());
    let bad_output = AtomicBool::new(false);
    thread::scope(|scope| {
        let writer = thread::Builder::new()
            .name("rriter-media-stdin".to_string())
            .spawn_scoped(scope, move || {
                let mut stdin = stdin;
                // EPIPE is not an error by itself: the helper may finish before it has read
                // all of its input. The exit status and the reply decide the outcome.
                let _ = stdin
                    .write_all(header.as_bytes())
                    .and_then(|()| stdin.write_all(bytes));
            });
        let bad_output = &bad_output;
        let reader = thread::Builder::new()
            .name("rriter-media-stdout".to_string())
            .spawn_scoped(scope, move || {
                let mut reader = BufReader::with_capacity(HEADER_LIMIT as usize, stdout);
                let reply = read_reply(&mut reader);
                if matches!(reply, Err(MediaError::Crashed)) {
                    bad_output.store(true, Ordering::Release);
                }
                reply
            });
        let (Ok(writer), Ok(reader)) = (writer, reader) else {
            // The closures own the pipe ends; terminating the child unblocks the other one.
            let _ = child.terminate(TERMINATE_GRACE);
            return Err(MediaError::Crashed);
        };
        let deadline = Instant::now() + timeout;
        let waited = loop {
            if bad_output.load(Ordering::Acquire) {
                break Wait::BadOutput;
            }
            let slice = EXIT_POLL.min(deadline.saturating_duration_since(Instant::now()));
            match child.wait_timeout(slice) {
                Ok(Some(status)) => break Wait::Exited(status),
                Ok(None) if Instant::now() >= deadline => break Wait::TimedOut,
                Ok(None) => {}
                Err(_) => break Wait::Failed,
            }
        };
        if !matches!(waited, Wait::Exited(_)) {
            // Kills the whole process group, which also closes the pipes both threads use.
            let _ = child.terminate(TERMINATE_GRACE);
        }
        let reply = reader.join();
        let _ = writer.join();
        match waited {
            Wait::TimedOut => Err(MediaError::Timeout),
            Wait::BadOutput | Wait::Failed => Err(MediaError::Crashed),
            Wait::Exited(status) if !status.success() => Err(MediaError::Crashed),
            Wait::Exited(_) => reply.unwrap_or(Err(MediaError::Crashed)),
        }
    })
}

fn kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Raster => "raster",
        MediaKind::Svg => "svg",
        MediaKind::Mermaid => "mermaid",
    }
}

fn parse_kind(name: &str) -> Option<MediaKind> {
    match name {
        "raster" => Some(MediaKind::Raster),
        "svg" => Some(MediaKind::Svg),
        "mermaid" => Some(MediaKind::Mermaid),
        _ => None,
    }
}

/// One header line without its newline; fails when the line is longer than `HEADER_LIMIT`,
/// not terminated, or not UTF-8.
fn read_line_limited(reader: &mut impl BufRead) -> io::Result<String> {
    let mut line = Vec::new();
    reader.by_ref().take(HEADER_LIMIT).read_until(b'\n', &mut line)?;
    if line.pop() != Some(b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unterminated header"));
    }
    String::from_utf8(line)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "header is not UTF-8"))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// A request as the helper sees it.
#[derive(Debug, PartialEq)]
enum HelperRequest {
    Decode {
        kind: MediaKind,
        scale: f32,
        max_raster_w: u32,
        bytes: Vec<u8>,
    },
    /// The declared length is over the limit; the body is not read.
    TooLarge,
}

fn read_request(input: &mut impl BufRead) -> io::Result<HelperRequest> {
    let line = read_line_limited(input)?;
    let mut parts = line.split(' ');
    let (Some(kind), Some(scale), Some(max_raster_w), Some(len), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(invalid_data("bad request header"));
    };
    let kind = parse_kind(kind).ok_or_else(|| invalid_data("unknown kind"))?;
    let scale = scale.parse::<f32>().map_err(|_| invalid_data("bad scale"))?;
    let max_raster_w = max_raster_w
        .parse::<u32>()
        .map_err(|_| invalid_data("bad width"))?;
    let len = len.parse::<usize>().map_err(|_| invalid_data("bad length"))?;
    if len > MAX_INPUT_BYTES {
        return Ok(HelperRequest::TooLarge);
    }
    let mut bytes = Vec::with_capacity(len);
    input.by_ref().take(len as u64).read_to_end(&mut bytes)?;
    if bytes.len() != len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated input"));
    }
    Ok(HelperRequest::Decode {
        kind,
        scale,
        max_raster_w,
        bytes,
    })
}

/// `MediaError` as the `<code>` and text of an `ERR` line.
fn error_parts(error: &MediaError) -> (&'static str, String) {
    match error {
        MediaError::NotFound => ("NotFound", String::new()),
        MediaError::Http(code) => ("Http", code.to_string()),
        MediaError::Timeout => ("Timeout", String::new()),
        MediaError::TooLarge => ("TooLarge", String::new()),
        MediaError::TooManyPixels => ("TooManyPixels", String::new()),
        MediaError::Decode => ("Decode", String::new()),
        MediaError::Unsupported => ("Unsupported", String::new()),
        MediaError::Mermaid(text) => ("Mermaid", text.clone()),
        MediaError::Crashed => ("Crashed", String::new()),
    }
}

fn error_from_parts(code: &str, text: &str) -> MediaError {
    match code {
        "NotFound" => MediaError::NotFound,
        "Http" => text
            .trim()
            .parse::<u16>()
            .map_or(MediaError::Crashed, MediaError::Http),
        "Timeout" => MediaError::Timeout,
        "TooLarge" => MediaError::TooLarge,
        "TooManyPixels" => MediaError::TooManyPixels,
        "Decode" => MediaError::Decode,
        "Unsupported" => MediaError::Unsupported,
        "Mermaid" => MediaError::Mermaid(text.to_string()),
        // An unknown code means the output is not ours.
        _ => MediaError::Crashed,
    }
}

fn write_reply(output: &mut impl Write, result: &Result<MediaPixels, MediaError>) -> io::Result<()> {
    match result {
        Ok(pixels)
            if pixels.rgba.len() as u64
                == u64::from(pixels.raster_w) * u64::from(pixels.raster_h) * 4 =>
        {
            writeln!(
                output,
                "OK {} {} {} {}",
                pixels.natural_w, pixels.natural_h, pixels.raster_w, pixels.raster_h
            )?;
            output.write_all(&pixels.rgba)
        }
        // A decoder that returns an inconsistent buffer is a decode failure, not a reply.
        Ok(_) => writeln!(output, "ERR Decode"),
        Err(error) => {
            let (code, text) = error_parts(error);
            let text: String = text
                .chars()
                .map(|ch| if ch.is_control() { ' ' } else { ch })
                .take(ERR_TEXT_LIMIT)
                .collect();
            let text = text.trim();
            if text.is_empty() {
                writeln!(output, "ERR {code}")
            } else {
                writeln!(output, "ERR {code} {text}")
            }
        }
    }
}

/// Parses an untrusted reply. Any malformed reply is `Crashed`; the pixel buffer is
/// allocated only after the declared size passed validation and is exactly that size.
fn read_reply(reader: &mut impl BufRead) -> Result<MediaPixels, MediaError> {
    let line = read_line_limited(reader).map_err(|_| MediaError::Crashed)?;
    let (tag, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
    match tag {
        "ERR" => {
            let (code, text) = rest.split_once(' ').unwrap_or((rest, ""));
            Err(error_from_parts(code, text))
        }
        "OK" => read_pixels(reader, rest),
        _ => Err(MediaError::Crashed),
    }
}

fn read_pixels(reader: &mut impl BufRead, fields: &str) -> Result<MediaPixels, MediaError> {
    let mut parts = fields.split(' ');
    let (Some(natural_w), Some(natural_h), Some(raster_w), Some(raster_h), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(MediaError::Crashed);
    };
    let natural_w = natural_w.parse::<f32>().map_err(|_| MediaError::Crashed)?;
    let natural_h = natural_h.parse::<f32>().map_err(|_| MediaError::Crashed)?;
    let raster_w = raster_w.parse::<u32>().map_err(|_| MediaError::Crashed)?;
    let raster_h = raster_h.parse::<u32>().map_err(|_| MediaError::Crashed)?;
    let sane = natural_w.is_finite()
        && natural_h.is_finite()
        && natural_w > 0.0
        && natural_h > 0.0
        && (1..=MAX_OUTPUT_SIDE).contains(&raster_w)
        && (1..=MAX_OUTPUT_SIDE).contains(&raster_h);
    if !sane {
        return Err(MediaError::Crashed);
    }
    let len = raster_w as usize * raster_h as usize * 4;
    let mut rgba = Vec::with_capacity(len);
    reader
        .by_ref()
        .take(len as u64)
        .read_to_end(&mut rgba)
        .map_err(|_| MediaError::Crashed)?;
    if rgba.len() != len {
        return Err(MediaError::Crashed);
    }
    // Nothing may follow the pixels.
    let mut extra = [0u8; 1];
    match reader.read(&mut extra) {
        Ok(0) => {}
        _ => return Err(MediaError::Crashed),
    }
    Ok(MediaPixels {
        natural_w,
        natural_h,
        raster_w,
        raster_h,
        rgba,
        stamp: None,
    })
}

/// Helper side: one request from `input`, one reply to `output`.
fn serve(input: &mut impl BufRead, output: &mut impl Write) -> io::Result<()> {
    let result = match read_request(input)? {
        HelperRequest::TooLarge => Err(MediaError::TooLarge),
        HelperRequest::Decode {
            kind,
            scale,
            max_raster_w,
            bytes,
        } => decode_in_process(kind, &bytes, scale, max_raster_w),
    };
    write_reply(output, &result)?;
    output.flush()
}

/// Entry point for `main`: when started as the media helper, serves one request and returns
/// the exit code (`0` for any reply including `ERR`, `2` when the request cannot be read or
/// the reply cannot be written); `None` for a normal start.
pub(crate) fn run_media_helper_if_requested() -> Option<i32> {
    if std::env::args_os().nth(1).as_deref() != Some(OsStr::new(HELPER_FLAG)) {
        return None;
    }
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = io::stdout().lock();
    Some(match serve(&mut input, &mut output) {
        Ok(()) => 0,
        Err(_) => 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown_media::{FetchEnv, MediaKey, MediaRequest, MediaSource, fetch};
    use std::io::Cursor;
    use std::path::PathBuf;

    fn png_bytes() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        let mut out = Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("encode png");
        out.into_inner()
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rriter-md-helper-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    fn pixels(raster_w: u32, raster_h: u32) -> MediaPixels {
        MediaPixels {
            natural_w: 7.5,
            natural_h: 3.0,
            raster_w,
            raster_h,
            rgba: (0..raster_w * raster_h * 4).map(|value| value as u8).collect(),
            stamp: None,
        }
    }

    fn reply_of(bytes: &[u8]) -> Result<MediaPixels, MediaError> {
        read_reply(&mut Cursor::new(bytes.to_vec()))
    }

    #[test]
    fn request_round_trips() {
        let mut wire = Vec::new();
        let header = format!("{} {} {} {}\n", kind_name(MediaKind::Svg), 1.5f32, 640u32, 5usize);
        wire.extend_from_slice(header.as_bytes());
        wire.extend_from_slice(b"<svg>");
        let request = read_request(&mut Cursor::new(wire)).expect("request");
        assert_eq!(
            request,
            HelperRequest::Decode {
                kind: MediaKind::Svg,
                scale: 1.5,
                max_raster_w: 640,
                bytes: b"<svg>".to_vec(),
            }
        );
        for kind in [MediaKind::Raster, MediaKind::Svg, MediaKind::Mermaid] {
            assert_eq!(parse_kind(kind_name(kind)), Some(kind));
        }
    }

    #[test]
    fn request_rejects_bad_headers_and_truncated_bodies() {
        let cases: [&[u8]; 8] = [
            b"",
            b"raster 1 100 5",
            b"raster 1 100\n",
            b"raster 1 100 5 6\n12345",
            b"png 1 100 5\n12345",
            b"raster x 100 5\n12345",
            b"raster 1 -1 5\n12345",
            b"raster 1 100 5\n123",
        ];
        for case in cases {
            assert!(read_request(&mut Cursor::new(case.to_vec())).is_err(), "{case:?}");
        }
        let long = vec![b'a'; 2000];
        assert!(read_request(&mut Cursor::new(long)).is_err());
    }

    #[test]
    fn request_over_limit_is_too_large_without_reading_the_body() {
        let header = format!("raster 1 100 {}\n", MAX_INPUT_BYTES + 1);
        let request = read_request(&mut Cursor::new(header.into_bytes())).expect("request");
        assert_eq!(request, HelperRequest::TooLarge);
    }

    #[test]
    fn reply_round_trips_pixels_and_every_error() {
        let mut wire = Vec::new();
        write_reply(&mut wire, &Ok(pixels(3, 2))).expect("write");
        let back = reply_of(&wire).expect("reply");
        assert_eq!((back.natural_w, back.natural_h), (7.5, 3.0));
        assert_eq!((back.raster_w, back.raster_h), (3, 2));
        assert_eq!(back.rgba, pixels(3, 2).rgba);
        assert!(back.stamp.is_none());
        let errors = [
            MediaError::NotFound,
            MediaError::Http(404),
            MediaError::Timeout,
            MediaError::TooLarge,
            MediaError::TooManyPixels,
            MediaError::Decode,
            MediaError::Unsupported,
            MediaError::Mermaid("parse error".to_string()),
            MediaError::Crashed,
        ];
        for error in errors {
            let mut wire = Vec::new();
            write_reply(&mut wire, &Err(error.clone())).expect("write");
            assert_eq!(reply_of(&wire).err(), Some(error));
        }
    }

    #[test]
    fn error_text_is_one_line_of_at_most_200_chars() {
        let text = format!("a\nb\r\tc{}", "я".repeat(500));
        let mut wire = Vec::new();
        write_reply(&mut wire, &Err(MediaError::Mermaid(text))).expect("write");
        assert_eq!(wire.iter().filter(|byte| **byte == b'\n').count(), 1);
        let Err(MediaError::Mermaid(back)) = reply_of(&wire) else {
            panic!("expected a Mermaid error");
        };
        assert_eq!(back.chars().count(), ERR_TEXT_LIMIT);
        assert!(back.starts_with("a b  c"));
    }

    #[test]
    fn inconsistent_pixel_buffer_is_written_as_decode_error() {
        let mut broken = pixels(2, 2);
        broken.rgba.pop();
        let mut wire = Vec::new();
        write_reply(&mut wire, &Ok(broken)).expect("write");
        assert_eq!(reply_of(&wire).err(), Some(MediaError::Decode));
    }

    #[test]
    fn garbage_truncated_and_oversized_replies_are_crashed() {
        let huge_header = vec![b'a'; 4000];
        let mut trailing = b"OK 1 1 1 1\n".to_vec();
        trailing.extend_from_slice(&[1, 2, 3, 4, 5]);
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"hello world".to_vec(),
            b"hello\n".to_vec(),
            huge_header,
            b"OK\n".to_vec(),
            b"OK 1 1 1\n".to_vec(),
            b"OK 1 1 1 1 1\n".to_vec(),
            b"OK x 1 1 1\n\x01\x02\x03\x04".to_vec(),
            b"OK NaN 1 1 1\n\x01\x02\x03\x04".to_vec(),
            b"OK inf 1 1 1\n\x01\x02\x03\x04".to_vec(),
            b"OK 0 1 1 1\n\x01\x02\x03\x04".to_vec(),
            b"OK 1 1 0 1\n".to_vec(),
            b"OK 1 1 4097 1\n".to_vec(),
            b"OK 1 1 5000 5000\n".to_vec(),
            b"OK 1 1 4294967295 4294967295\n".to_vec(),
            b"OK 1 1 -1 1\n\x01\x02\x03\x04".to_vec(),
            b"OK 1 1 1 1\n".to_vec(),
            b"OK 1 1 2 2\n\x01".to_vec(),
            trailing,
            b"ERR Bogus text\n".to_vec(),
            b"ERR Http abc\n".to_vec(),
            b"ERR\n".to_vec(),
            b"ERR Decode no newline".to_vec(),
        ];
        for case in cases {
            assert_eq!(reply_of(&case).err(), Some(MediaError::Crashed), "{case:?}");
        }
    }

    #[test]
    fn serve_decodes_a_png_and_reports_bad_input() {
        let mut request = format!("raster 1 100 {}\n", png_bytes().len()).into_bytes();
        request.extend_from_slice(&png_bytes());
        let mut output = Vec::new();
        serve(&mut Cursor::new(request), &mut output).expect("serve");
        let back = reply_of(&output).expect("pixels");
        assert_eq!(back.rgba.len(), back.raster_w as usize * back.raster_h as usize * 4);
        assert_eq!(&back.rgba[..4], &[10, 20, 30, 255]);

        let mut output = Vec::new();
        serve(&mut Cursor::new(b"raster 1 100 4\nabcd".to_vec()), &mut output).expect("serve");
        assert_eq!(reply_of(&output).err(), Some(MediaError::Decode));

        let header = format!("raster 1 100 {}\n", MAX_INPUT_BYTES + 1);
        let mut output = Vec::new();
        serve(&mut Cursor::new(header.into_bytes()), &mut output).expect("serve");
        assert_eq!(reply_of(&output).err(), Some(MediaError::TooLarge));

        let mut output = Vec::new();
        assert!(serve(&mut Cursor::new(b"garbage\n".to_vec()), &mut output).is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn helper_entry_is_inactive_without_the_flag() {
        assert_eq!(run_media_helper_if_requested(), None);
        assert!(matches!(
            RenderCommand::for_current_process(),
            Some(RenderCommand::InProcess)
        ));
    }

    #[test]
    fn oversized_input_is_too_large_for_every_command() {
        let bytes = vec![0u8; MAX_INPUT_BYTES + 1];
        let process = RenderCommand::Process {
            exe: PathBuf::from("/nonexistent/rriter-helper"),
            prefix_args: Vec::new(),
        };
        for cmd in [RenderCommand::InProcess, process] {
            let result = render_media(&cmd, MediaKind::Raster, &bytes, 1.0, 100, Duration::from_secs(1));
            assert_eq!(result.err(), Some(MediaError::TooLarge));
        }
    }

    #[test]
    fn in_process_command_decodes_directly() {
        let result = render_media(
            &RenderCommand::InProcess,
            MediaKind::Raster,
            &png_bytes(),
            1.0,
            100,
            Duration::from_secs(1),
        );
        assert_eq!(&result.expect("pixels").rgba[..4], &[10, 20, 30, 255]);
    }

    fn env_with(render: Option<RenderCommand>) -> FetchEnv {
        FetchEnv {
            cache_dir: test_dir("cache"),
            http: crate::markdown_media::HttpSource::ready(
                reqwest::blocking::Client::builder()
                    .no_proxy()
                    .build()
                    .expect("http client"),
            ),
            max_bytes: 20 * 1024 * 1024,
            render,
        }
    }

    fn file_request(path: PathBuf) -> MediaRequest {
        MediaRequest {
            key: MediaKey::Url("unused".to_string()),
            source: MediaSource::File(path),
            max_raster_w: 100,
            scale: 1.0,
        }
    }

    #[test]
    fn load_media_without_render_command_is_unsupported() {
        let request = file_request(PathBuf::from("/nonexistent/image.png"));
        let result = fetch::load_media(&request, &env_with(None));
        assert_eq!(result.err(), Some(MediaError::Unsupported));
    }

    #[test]
    fn load_media_decodes_a_file_and_keeps_the_stamp() {
        let dir = test_dir("load");
        let path = dir.join("pic.png");
        let png = png_bytes();
        std::fs::write(&path, &png).expect("write png");
        let env = env_with(Some(RenderCommand::InProcess));
        let pixels = fetch::load_media(&file_request(path.clone()), &env).expect("pixels");
        assert_eq!(&pixels.rgba[..4], &[10, 20, 30, 255]);
        assert_eq!((pixels.natural_w, pixels.natural_h), (3.0, 2.0));
        let stamp = pixels.stamp.expect("stamp");
        assert_eq!(stamp.len, png.len() as u64);
        let missing = fetch::load_media(&file_request(dir.join("none.png")), &env);
        assert_eq!(missing.err(), Some(MediaError::NotFound));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    mod process {
        use super::*;

        fn sh(script: &str) -> RenderCommand {
            RenderCommand::Process {
                exe: PathBuf::from("sh"),
                prefix_args: vec!["-c".to_string(), script.to_string(), "sh".to_string()],
            }
        }

        fn run(script: &str, bytes: &[u8], timeout: Duration) -> Result<MediaPixels, MediaError> {
            render_media(&sh(script), MediaKind::Raster, bytes, 1.0, 100, timeout)
        }

        fn run_small(script: &str) -> Result<MediaPixels, MediaError> {
            run(script, b"x", Duration::from_secs(10))
        }

        #[test]
        fn non_zero_exit_is_crashed_even_with_a_large_unread_input() {
            let big = vec![7u8; 3 * 1024 * 1024];
            let result = run("exit 101", &big, Duration::from_secs(10));
            assert_eq!(result.err(), Some(MediaError::Crashed));
        }

        #[test]
        fn killed_helper_is_crashed() {
            assert_eq!(run_small("kill -9 $$").err(), Some(MediaError::Crashed));
        }

        #[test]
        fn missing_executable_is_crashed() {
            let cmd = RenderCommand::Process {
                exe: PathBuf::from("/nonexistent/rriter-helper"),
                prefix_args: Vec::new(),
            };
            let result = render_media(&cmd, MediaKind::Raster, b"x", 1.0, 100, Duration::from_secs(1));
            assert_eq!(result.err(), Some(MediaError::Crashed));
        }

        #[test]
        fn slow_helper_times_out_promptly() {
            let started = Instant::now();
            let result = run("sleep 5", b"x", Duration::from_millis(300));
            assert_eq!(result.err(), Some(MediaError::Timeout));
            assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        }

        #[test]
        fn timeout_with_a_blocked_large_input_does_not_hang() {
            let big = vec![7u8; 8 * 1024 * 1024];
            let started = Instant::now();
            let result = run("sleep 5", &big, Duration::from_millis(300));
            assert_eq!(result.err(), Some(MediaError::Timeout));
            assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        }

        #[test]
        fn timeout_kills_the_whole_process_tree() {
            let dir = test_dir("tree");
            let marker = dir.join("survivor");
            let script = format!("(sleep 1; echo alive > {}) & sleep 5", marker.display());
            let result = run(&script, b"x", Duration::from_millis(300));
            assert_eq!(result.err(), Some(MediaError::Timeout));
            std::thread::sleep(Duration::from_millis(1500));
            assert!(!marker.exists(), "a descendant outlived the helper");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn handled_error_reply_keeps_its_code() {
            assert_eq!(
                run_small("printf 'ERR Decode bad\\n'").err(),
                Some(MediaError::Decode)
            );
            assert_eq!(
                run_small("printf 'ERR Mermaid line one\\n'").err(),
                Some(MediaError::Mermaid("line one".to_string()))
            );
        }

        #[test]
        fn ok_reply_with_pixels_is_returned() {
            let pixels = run_small("printf 'OK 2 3 1 1\\n\\001\\002\\003\\004'").expect("pixels");
            assert_eq!((pixels.natural_w, pixels.natural_h), (2.0, 3.0));
            assert_eq!((pixels.raster_w, pixels.raster_h), (1, 1));
            assert_eq!(pixels.rgba, vec![1, 2, 3, 4]);
        }

        #[test]
        fn malformed_helper_output_is_crashed() {
            let scripts = [
                "printf 'OK 1 1 1 1\\n'",
                "printf 'OK 1 1 2 2\\n\\001'",
                "printf 'OK 1 1 1 1\\n\\001\\002\\003\\004\\005'",
                "printf 'OK 1 1 5000 5000\\n'",
                "printf 'hello world'",
                "head -c 4000 /dev/zero | tr '\\0' a",
                "head -c 70000000 /dev/zero",
                "printf 'ERR Bogus\\n'",
                "printf ''",
            ];
            for script in scripts {
                assert_eq!(run_small(script).err(), Some(MediaError::Crashed), "{script}");
            }
        }

        #[test]
        fn garbage_output_does_not_wait_for_the_timeout() {
            let started = Instant::now();
            let result = run("printf 'hello world\\n'; sleep 5", b"x", Duration::from_secs(10));
            assert_eq!(result.err(), Some(MediaError::Crashed));
            assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        }

        #[test]
        fn valid_output_with_non_zero_exit_is_crashed() {
            let result = run_small("printf 'OK 1 1 1 1\\n\\001\\002\\003\\004'; exit 3");
            assert_eq!(result.err(), Some(MediaError::Crashed));
        }

        #[test]
        fn helper_receives_the_request_on_stdin() {
            // The child answers only when the first 12 bytes of its stdin are the header prefix.
            let result = run_small("head -c 12 | grep -q '^raster 1 100' && printf 'ERR Decode ok\\n'");
            assert_eq!(result.err(), Some(MediaError::Decode));
        }

        #[test]
        fn mermaid_helper_gets_a_seeded_cache_through_its_own_environment() {
            let root = test_dir("mermaid-env");
            let script = "test \"$XDG_CACHE_HOME\" = \"$1\" && test \"$2\" = --rriter-media-render \
                          && printf 'ERR Decode env\\n' || exit 101";
            let prefix = vec![
                "-c".to_string(),
                script.to_string(),
                "sh".to_string(),
                root.display().to_string(),
            ];
            let before = std::env::var_os("XDG_CACHE_HOME");
            let mut command =
                helper_command(Path::new("sh"), &prefix, Some(&root)).expect("command");
            let result = exchange(
                &mut command,
                MediaKind::Mermaid,
                b"graph TD; A-->B",
                1.0,
                100,
                Duration::from_secs(10),
            );
            assert_eq!(result.err(), Some(MediaError::Decode));
            assert_eq!(std::env::var_os("XDG_CACHE_HOME"), before);
            let cache = root.join("mmdr").join("font-cache");
            assert!(std::fs::read_dir(&cache).expect("font cache").count() >= 2);
            let _ = std::fs::remove_dir_all(&root);
        }

        #[test]
        fn non_mermaid_helper_gets_no_cache_override() {
            let command = helper_command(Path::new("sh"), &[], None).expect("command");
            assert!(command.get_envs().next().is_none());
        }
    }
}
