use crate::app::keyboard::KeyInput;
use std::path::PathBuf;
use winit::keyboard::ModifiersState;

pub(crate) const SIZE_MIN: (u32, u32) = (320, 200);
pub(crate) const SIZE_MAX: (u32, u32) = (8192, 8192);
const SCALE_MIN: f64 = 0.5;
const SCALE_MAX: f64 = 4.0;
const MAX_WAIT_MS: u64 = 60_000;
const DEFAULT_SETTLE_MS: u64 = 500;
const MAX_BENCH_FRAMES: u32 = 100_000;
const MAX_RECORD_FRAMES: u32 = 600;
const MAX_KEY_REPEATS: u32 = 1000;
// Longest piece of user input echoed back inside an error reason.
const MAX_ECHO_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MouseButtonArg {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ClickPhase {
    Both,
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WheelUnit {
    Lines,
    Px,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DialogAnswer {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BenchAction {
    None,
    Wheel { dx: f64, dy: f64 },
    Key { input: KeyInput, mods: ModifiersState },
    Type(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    Open(PathBuf),
    Workspace(PathBuf),
    Resize { w: u32, h: u32 },
    Scale(f64),
    MouseMove { x: f64, y: f64 },
    Click { button: MouseButtonArg, phase: ClickPhase, alt: bool },
    DblClick { button: MouseButtonArg },
    Wheel { dx: f64, dy: f64, unit: WheelUnit },
    Key { input: KeyInput, mods: ModifiersState, combo: String, repeats: u32 },
    Type(String),
    Settle { ms: u64 },
    Wait { ms: u64 },
    /// Strict loop model: one native `about_to_wait` wake-up within `ms`.
    Wake { ms: u64 },
    /// Strict loop model: `wake` repeated for `ms` of input-free time.
    Idle { ms: u64 },
    Screenshot(PathBuf),
    Dump(Option<PathBuf>),
    Dialog(DialogAnswer),
    PickerAnswer(Vec<PathBuf>),
    Bench { frames: u32, csv: Option<PathBuf>, action: BenchAction },
    Record { frames: u32, dir: PathBuf, action: BenchAction },
    Info,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Response {
    Ok(Option<String>),
    Err(String),
}

impl Response {
    /// One protocol output line (without the trailing newline); embedded line breaks become spaces.
    pub(crate) fn line(&self) -> String {
        let (head, body) = match self {
            Response::Ok(None) => return "ok".to_string(),
            Response::Ok(Some(payload)) => ("ok ", payload),
            Response::Err(reason) => ("err ", reason),
        };
        let mut line = String::with_capacity(head.len() + body.len());
        line.push_str(head);
        line.extend(body.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }));
        line
    }
}

/// Whitespace-separated token cursor over the argument part of a command line.
#[derive(Clone, Copy)]
struct Args<'a> {
    rest: &'a str,
}

impl<'a> Args<'a> {
    fn token(&mut self) -> Option<&'a str> {
        let s = self.rest.trim_start();
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        let (token, rest) = s.split_at(end);
        self.rest = rest;
        (!token.is_empty()).then_some(token)
    }

    fn required(&mut self, what: &str) -> Result<&'a str, String> {
        self.token().ok_or_else(|| format!("missing {what}"))
    }

    /// Rest of the line from its first non-whitespace char; trailing whitespace is kept.
    fn remainder(&mut self) -> &'a str {
        let s = self.rest.trim_start();
        self.rest = "";
        s
    }

    fn finish(&mut self) -> Result<(), String> {
        match self.token() {
            None => Ok(()),
            Some(extra) => Err(format!("unexpected argument '{}'", clip(extra))),
        }
    }
}

/// Parses one protocol line; `Ok(None)` for blank and comment lines.
pub(crate) fn parse_line(line: &[u8]) -> Result<Option<Command>, String> {
    let line = std::str::from_utf8(line).map_err(|_| "invalid utf-8".to_string())?;
    let mut args = Args { rest: line.trim_end_matches(['\r', '\n']) };
    let Some(name) = args.token() else {
        return Ok(None);
    };
    if name.starts_with('#') {
        return Ok(None);
    }
    let command = match name {
        "open" => Command::Open(path_rest(&mut args)?),
        "workspace" => Command::Workspace(path_rest(&mut args)?),
        "screenshot" => Command::Screenshot(path_rest(&mut args)?),
        "dump" => {
            let rest = args.remainder().trim_end();
            Command::Dump((!rest.is_empty()).then(|| PathBuf::from(rest)))
        }
        "type" => Command::Type(unescape_text(args.remainder())),
        "picker_answer" => {
            let paths = std::iter::from_fn(|| args.token()).map(PathBuf::from).collect();
            Command::PickerAnswer(paths)
        }
        "resize" => {
            let (w, h) = parse_size(args.required("size")?)?;
            args.finish()?;
            Command::Resize { w, h }
        }
        "scale" => {
            let scale = parse_scale(args.required("scale")?)?;
            args.finish()?;
            Command::Scale(scale)
        }
        "mouse_move" => {
            let x = parse_f64(args.required("x")?)?;
            let y = parse_f64(args.required("y")?)?;
            args.finish()?;
            Command::MouseMove { x, y }
        }
        "click" => parse_click(&mut args)?,
        "dblclick" => {
            let button = match args.token() {
                None => MouseButtonArg::Left,
                Some(t) => button_arg(t).ok_or_else(|| format!("invalid button '{}'", clip(t)))?,
            };
            args.finish()?;
            Command::DblClick { button }
        }
        "wheel" => {
            let dx = parse_f64(args.required("dx")?)?;
            let dy = parse_f64(args.required("dy")?)?;
            let unit = match args.token() {
                None | Some("lines") => WheelUnit::Lines,
                Some("px") => WheelUnit::Px,
                Some(t) => return Err(format!("invalid wheel unit '{}'", clip(t))),
            };
            args.finish()?;
            Command::Wheel { dx, dy, unit }
        }
        "key" => {
            let first = args.required("key combo or --repeat")?;
            let (combo, repeats) = if first == "--repeat" {
                let repeats = parse_uint::<u32>(args.required("repeat count")?)?;
                if repeats > MAX_KEY_REPEATS {
                    return Err(format!("repeat count exceeds {MAX_KEY_REPEATS}"));
                }
                (args.required("key combo")?, repeats)
            } else {
                (first, 0)
            };
            let (input, mods) = KeyInput::parse_combo(combo)?;
            args.finish()?;
            Command::Key { input, mods, combo: combo.to_string(), repeats }
        }
        "settle" => {
            let ms = match args.token() {
                None => DEFAULT_SETTLE_MS,
                Some(t) => parse_ms(t)?,
            };
            args.finish()?;
            Command::Settle { ms }
        }
        "wait" => {
            let ms = parse_ms(args.required("milliseconds")?)?;
            args.finish()?;
            Command::Wait { ms }
        }
        "wake" | "idle" => {
            let ms = parse_ms(args.required("milliseconds")?)?;
            args.finish()?;
            if name == "wake" { Command::Wake { ms } } else { Command::Idle { ms } }
        }
        "dialog" => {
            let answer = match args.required("dialog answer")? {
                "save" => DialogAnswer::Save,
                "discard" => DialogAnswer::Discard,
                "cancel" => DialogAnswer::Cancel,
                t => return Err(format!("invalid dialog answer '{}'", clip(t))),
            };
            args.finish()?;
            Command::Dialog(answer)
        }
        "bench" => {
            let frames = parse_frames(args.required("frame count")?, MAX_BENCH_FRAMES)?;
            let mut probe = args;
            let mut csv = None;
            if let Some(path) = probe.token().and_then(|t| t.strip_prefix("csv=")) {
                if path.is_empty() {
                    return Err("missing csv path".to_string());
                }
                csv = Some(PathBuf::from(path));
                args = probe;
            }
            let action = parse_action(&mut args)?;
            Command::Bench { frames, csv, action }
        }
        "record" => {
            let frames = parse_frames(args.required("frame count")?, MAX_RECORD_FRAMES)?;
            let dir = PathBuf::from(args.required("directory")?);
            let action = parse_action(&mut args)?;
            Command::Record { frames, dir, action }
        }
        "info" => {
            args.finish()?;
            Command::Info
        }
        "quit" => {
            args.finish()?;
            Command::Quit
        }
        _ => return Err(format!("unknown command '{}'", clip(name))),
    };
    Ok(Some(command))
}

/// Parses `WxH` within `SIZE_MIN..=SIZE_MAX`; shared with the `--size` CLI option.
pub(crate) fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let invalid = || format!("invalid size '{}', expected WxH", clip(s));
    let (w, h) = s.split_once('x').ok_or_else(invalid)?;
    let w: u32 = w.parse().map_err(|_| invalid())?;
    let h: u32 = h.parse().map_err(|_| invalid())?;
    if !(SIZE_MIN.0..=SIZE_MAX.0).contains(&w) || !(SIZE_MIN.1..=SIZE_MAX.1).contains(&h) {
        return Err(format!(
            "size {w}x{h} out of range {}x{}..{}x{}",
            SIZE_MIN.0, SIZE_MIN.1, SIZE_MAX.0, SIZE_MAX.1
        ));
    }
    Ok((w, h))
}

fn clip(s: &str) -> &str {
    match s.char_indices().nth(MAX_ECHO_CHARS) {
        Some((end, _)) => &s[..end],
        None => s,
    }
}

fn path_rest(args: &mut Args) -> Result<PathBuf, String> {
    let rest = args.remainder().trim_end();
    if rest.is_empty() {
        return Err("missing path".to_string());
    }
    Ok(PathBuf::from(rest))
}

pub(crate) fn parse_f64(t: &str) -> Result<f64, String> {
    let value: f64 = t.parse().map_err(|_| format!("invalid number '{}'", clip(t)))?;
    if !value.is_finite() {
        return Err("non-finite number".to_string());
    }
    Ok(value)
}

/// Parses a finite UI scale within `SCALE_MIN..=SCALE_MAX`; shared with the `--scale` CLI option.
pub(crate) fn parse_scale(t: &str) -> Result<f64, String> {
    let scale = parse_f64(t)?;
    if !(SCALE_MIN..=SCALE_MAX).contains(&scale) {
        return Err(format!("scale must be between {SCALE_MIN} and {SCALE_MAX}"));
    }
    Ok(scale)
}

fn parse_uint<T: std::str::FromStr>(t: &str) -> Result<T, String> {
    t.parse().map_err(|_| format!("invalid integer '{}'", clip(t)))
}

fn parse_ms(t: &str) -> Result<u64, String> {
    let ms: u64 = parse_uint(t)?;
    if ms > MAX_WAIT_MS {
        return Err(format!("milliseconds must be at most {MAX_WAIT_MS}"));
    }
    Ok(ms)
}

fn parse_frames(t: &str, max: u32) -> Result<u32, String> {
    let frames: u32 = parse_uint(t)?;
    if !(1..=max).contains(&frames) {
        return Err(format!("frame count must be between 1 and {max}"));
    }
    Ok(frames)
}

fn button_arg(t: &str) -> Option<MouseButtonArg> {
    match t {
        "left" => Some(MouseButtonArg::Left),
        "right" => Some(MouseButtonArg::Right),
        "middle" => Some(MouseButtonArg::Middle),
        _ => None,
    }
}

fn parse_click(args: &mut Args) -> Result<Command, String> {
    let mut button = None;
    let mut phase = None;
    let mut alt = false;
    while let Some(t) = args.token() {
        let duplicate = if t == "alt" {
            std::mem::replace(&mut alt, true)
        } else if let Some(b) = button_arg(t) {
            button.replace(b).is_some()
        } else {
            let p = match t {
                "down" => ClickPhase::Down,
                "up" => ClickPhase::Up,
                _ => return Err(format!("invalid click argument '{}'", clip(t))),
            };
            phase.replace(p).is_some()
        };
        if duplicate {
            return Err(format!("duplicate click argument '{t}'"));
        }
    }
    Ok(Command::Click {
        button: button.unwrap_or(MouseButtonArg::Left),
        phase: phase.unwrap_or(ClickPhase::Both),
        alt,
    })
}

fn parse_action(args: &mut Args) -> Result<BenchAction, String> {
    let action = match args.token() {
        None | Some("none") => BenchAction::None,
        Some("wheel") => {
            let dx = parse_f64(args.required("dx")?)?;
            let dy = parse_f64(args.required("dy")?)?;
            BenchAction::Wheel { dx, dy }
        }
        Some("key") => {
            let (input, mods) = KeyInput::parse_combo(args.required("key combo")?)?;
            BenchAction::Key { input, mods }
        }
        Some("type") => return Ok(BenchAction::Type(unescape_text(args.remainder()))),
        Some(t) => return Err(format!("unknown action '{}'", clip(t))),
    };
    args.finish()?;
    Ok(action)
}

/// Applies `\n`, `\t`, `\\` left to right; any other `\x` and a trailing `\` stay verbatim.
fn unescape_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Option<Command>, String> {
        parse_line(line.as_bytes())
    }

    fn cmd(line: &str) -> Command {
        parse(line).unwrap().unwrap_or_else(|| panic!("no command for {line:?}"))
    }

    fn err(line: &str) -> String {
        parse(line).unwrap_err()
    }

    fn combo(text: &str) -> (KeyInput, ModifiersState) {
        KeyInput::parse_combo(text).unwrap()
    }

    #[test]
    fn headless_protocol_skips_blank_and_comment_lines() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("   "), Ok(None));
        assert_eq!(parse(" \t \r\n"), Ok(None));
        assert_eq!(parse("# comment"), Ok(None));
        assert_eq!(parse("   #open /x"), Ok(None));
    }

    #[test]
    fn headless_protocol_rejects_invalid_utf8_and_unknown_commands() {
        assert_eq!(parse_line(b"\xff\xfe"), Err("invalid utf-8".to_string()));
        assert_eq!(parse_line(b"type \xff"), Err("invalid utf-8".to_string()));
        assert_eq!(err("frobnicate 1 2"), "unknown command 'frobnicate'");
        let huge = "x".repeat(1 << 20);
        let reason = err(&huge);
        assert!(reason.starts_with("unknown command '"), "{reason}");
        assert!(reason.len() < 100, "{}", reason.len());
    }

    #[test]
    fn headless_protocol_parses_rest_of_line_paths() {
        assert_eq!(cmd("open  /a b/c.rs"), Command::Open(PathBuf::from("/a b/c.rs")));
        assert_eq!(cmd("open /x/y.rs  \r\n"), Command::Open(PathBuf::from("/x/y.rs")));
        assert_eq!(cmd("workspace /w s"), Command::Workspace(PathBuf::from("/w s")));
        assert_eq!(cmd("screenshot /tmp/a b.png"), Command::Screenshot(PathBuf::from("/tmp/a b.png")));
        assert_eq!(cmd("dump"), Command::Dump(None));
        assert_eq!(cmd("dump   "), Command::Dump(None));
        assert_eq!(cmd("dump /tmp/d.json"), Command::Dump(Some(PathBuf::from("/tmp/d.json"))));
        assert_eq!(err("open"), "missing path");
        assert_eq!(err("workspace   "), "missing path");
        assert_eq!(err("screenshot"), "missing path");
    }

    #[test]
    fn headless_protocol_parses_picker_answers() {
        assert_eq!(cmd("picker_answer"), Command::PickerAnswer(Vec::new()));
        assert_eq!(
            cmd("picker_answer /tmp/one /tmp/two"),
            Command::PickerAnswer(vec![PathBuf::from("/tmp/one"), PathBuf::from("/tmp/two")])
        );
    }

    #[test]
    fn headless_protocol_parses_resize_and_scale() {
        assert_eq!(cmd("resize 1920x1080"), Command::Resize { w: 1920, h: 1080 });
        assert_eq!(cmd("scale 1.5"), Command::Scale(1.5));
        assert!(parse("resize 10x10").is_err());
        assert!(parse("resize 9000x600").is_err());
        assert!(parse("resize 800").is_err());
        assert!(parse("resize").is_err());
        assert!(parse("resize 800x600 1").is_err());
        assert!(parse("scale 0.25").is_err());
        assert!(parse("scale 4.5").is_err());
        assert_eq!(err("scale inf"), "non-finite number");
    }

    #[test]
    fn headless_protocol_parse_size_bounds() {
        assert_eq!(parse_size("320x200"), Ok((320, 200)));
        assert_eq!(parse_size("8192x8192"), Ok((8192, 8192)));
        for bad in ["319x200", "320x199", "8193x1080", "1920x8193", "x", "", "axb", "-1x200", "1920x1080x1"] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn headless_protocol_parses_mouse_commands() {
        assert_eq!(cmd("mouse_move 10.5 -3"), Command::MouseMove { x: 10.5, y: -3.0 });
        let click = |button, phase| Command::Click { button, phase, alt: false };
        assert_eq!(cmd("click"), click(MouseButtonArg::Left, ClickPhase::Both));
        assert_eq!(cmd("click right"), click(MouseButtonArg::Right, ClickPhase::Both));
        assert_eq!(cmd("click down"), click(MouseButtonArg::Left, ClickPhase::Down));
        assert_eq!(cmd("click up right"), click(MouseButtonArg::Right, ClickPhase::Up));
        assert_eq!(cmd("click middle down"), click(MouseButtonArg::Middle, ClickPhase::Down));
        assert_eq!(cmd("click alt"), Command::Click { button: MouseButtonArg::Left, phase: ClickPhase::Both, alt: true });
        assert_eq!(cmd("dblclick"), Command::DblClick { button: MouseButtonArg::Left });
        assert_eq!(cmd("dblclick middle"), Command::DblClick { button: MouseButtonArg::Middle });
        assert_eq!(cmd("wheel 0 -3"), Command::Wheel { dx: 0.0, dy: -3.0, unit: WheelUnit::Lines });
        assert_eq!(cmd("wheel 0 -3 lines"), Command::Wheel { dx: 0.0, dy: -3.0, unit: WheelUnit::Lines });
        assert_eq!(cmd("wheel 1 2.5 px"), Command::Wheel { dx: 1.0, dy: 2.5, unit: WheelUnit::Px });
    }

    #[test]
    fn headless_protocol_rejects_bad_mouse_commands() {
        assert!(parse("click banana").is_err());
        assert!(parse("click left left").is_err());
        assert!(parse("click up down").is_err());
        assert!(parse("click left up right").is_err());
        assert!(parse("mouse_move 10").is_err());
        assert!(parse("mouse_move a b").is_err());
        assert_eq!(err("mouse_move nan 5"), "non-finite number");
        assert_eq!(err("mouse_move 5 -inf"), "non-finite number");
        assert!(parse("mouse_move 1 2 3").is_err());
        assert!(parse("wheel 0 -3 furlongs").is_err());
        assert!(parse("wheel 0").is_err());
        assert!(parse("wheel 0 1 px 2").is_err());
        assert!(parse("dblclick up").is_err());
        assert!(parse("dblclick left right").is_err());
    }

    #[test]
    fn headless_protocol_parses_key_and_type() {
        let (input, mods) = combo("ctrl+shift+p");
        assert_eq!(cmd("key ctrl+shift+p"), Command::Key { input, mods, combo: "ctrl+shift+p".to_string(), repeats: 0 });
        assert_eq!(cmd("key --repeat 2 ctrl+z"), Command::Key { input: combo("ctrl+z").0, mods: combo("ctrl+z").1, combo: "ctrl+z".to_string(), repeats: 2 });
        assert!(parse("key --repeat 1001 ctrl+z").is_err());
        assert_eq!(err("key ctrl+"), KeyInput::parse_combo("ctrl+").unwrap_err());
        assert_eq!(err("key ctrl+foo"), "unknown key token 'foo'");
        assert!(parse("key").is_err());
        assert!(parse("key a b").is_err());
        assert_eq!(cmd("type hello world  "), Command::Type("hello world  ".to_string()));
        assert_eq!(cmd("type"), Command::Type(String::new()));
        assert_eq!(cmd("type   "), Command::Type(String::new()));
        assert_eq!(cmd(r"type \tx\\n"), Command::Type("\tx\\n".to_string()));
        assert_eq!(cmd(r"type a\nb"), Command::Type("a\nb".to_string()));
        assert_eq!(cmd(r"type a\qb\"), Command::Type(r"a\qb\".to_string()));
    }

    #[test]
    fn headless_protocol_parses_timing_commands() {
        assert_eq!(cmd("settle"), Command::Settle { ms: 500 });
        assert_eq!(cmd("settle 20"), Command::Settle { ms: 20 });
        assert_eq!(cmd("wait 100"), Command::Wait { ms: 100 });
        assert_eq!(cmd("wait 60000"), Command::Wait { ms: 60000 });
        assert!(parse("wait").is_err());
        assert_eq!(cmd("wake 200"), Command::Wake { ms: 200 });
        assert_eq!(cmd("idle 0"), Command::Idle { ms: 0 });
        assert!(parse("idle").is_err());
        assert!(parse("wake 60001").is_err());
        assert!(parse("wait 60001").is_err());
        assert!(parse("settle 70000").is_err());
        assert!(parse("settle -1").is_err());
        assert!(parse("settle 1.5").is_err());
        assert!(parse("wait abc").is_err());
        assert!(parse("wait 10 20").is_err());
    }

    #[test]
    fn headless_protocol_parses_dialog_info_quit() {
        assert_eq!(cmd("dialog save"), Command::Dialog(DialogAnswer::Save));
        assert_eq!(cmd("dialog discard"), Command::Dialog(DialogAnswer::Discard));
        assert_eq!(cmd("dialog cancel"), Command::Dialog(DialogAnswer::Cancel));
        assert_eq!(cmd("info"), Command::Info);
        assert_eq!(cmd("quit\r\n"), Command::Quit);
        assert!(parse("dialog").is_err());
        assert!(parse("dialog maybe").is_err());
        assert!(parse("dialog save now").is_err());
        assert!(parse("info x").is_err());
        assert!(parse("quit now").is_err());
    }

    #[test]
    fn headless_protocol_parses_bench() {
        assert_eq!(cmd("bench 10"), Command::Bench { frames: 10, csv: None, action: BenchAction::None });
        assert_eq!(
            cmd("bench 10 csv=/tmp/a.csv wheel 0 -3"),
            Command::Bench {
                frames: 10,
                csv: Some(PathBuf::from("/tmp/a.csv")),
                action: BenchAction::Wheel { dx: 0.0, dy: -3.0 },
            }
        );
        assert_eq!(cmd("bench 5 none"), Command::Bench { frames: 5, csv: None, action: BenchAction::None });
        let (input, mods) = combo("ctrl+z");
        assert_eq!(cmd("bench 5 key ctrl+z"), Command::Bench { frames: 5, csv: None, action: BenchAction::Key { input, mods } });
        assert_eq!(
            cmd(r"bench 5 type ab c\t "),
            Command::Bench { frames: 5, csv: None, action: BenchAction::Type("ab c\t ".to_string()) }
        );
        for bad in ["bench", "bench 0", "bench 100001", "bench -1", "bench 5 wheel 1", "bench 5 fly", "bench 5 none x", "bench 5 csv=", "bench 5 key ctrl+foo"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn headless_protocol_parses_record() {
        assert_eq!(cmd("record 60 /tmp/x"), Command::Record { frames: 60, dir: PathBuf::from("/tmp/x"), action: BenchAction::None });
        assert_eq!(
            cmd("record 600 /tmp/x wheel 0 3"),
            Command::Record { frames: 600, dir: PathBuf::from("/tmp/x"), action: BenchAction::Wheel { dx: 0.0, dy: 3.0 } }
        );
        assert_eq!(err("record 5"), "missing directory");
        assert!(parse("record 601 /tmp/x").is_err());
        assert!(parse("record 0 /tmp/x").is_err());
        assert!(parse("record").is_err());
    }

    #[test]
    fn headless_protocol_response_line() {
        assert_eq!(Response::Ok(None).line(), "ok");
        assert_eq!(Response::Ok(Some("a\nb".to_string())).line(), "ok a b");
        assert_eq!(Response::Ok(Some("x\r\ny".to_string())).line(), "ok x  y");
        assert_eq!(Response::Err("bad\rthing".to_string()).line(), "err bad thing");
    }
}
