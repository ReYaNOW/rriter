use encoding_rs::{
    Encoding, IBM866, ISO_8859_2, ISO_8859_5, KOI8_R, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252,
};
use std::fs;
use std::io;
use std::path::Path;

const LEGACY_MIN_NON_ASCII_BYTES: usize = 4;
const LEGACY_MIN_SCORE: f32 = 72.0;
const LEGACY_MIN_MARGIN: f32 = 4.0;
const LEGACY_MIXED_SCRIPT_WORD_PENALTY: f32 = 12.0;
const LEGACY_MAX_MIXED_SCRIPT_PENALTY: f32 = 36.0;
const LEGACY_INTRAWORD_SYMBOL_PENALTY: f32 = 4.0;
const LEGACY_MAX_INTRAWORD_SYMBOL_PENALTY: f32 = 16.0;
const LEGACY_LATIN_SATURATED_WORD_PENALTY: f32 = 12.0;
const LEGACY_MAX_LATIN_SATURATION_PENALTY: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyEncoding {
    Iso8859_5,
    Windows1251,
    Koi8R,
    Ibm866,
    Windows1252,
    Windows1250,
    Iso8859_2,
}

impl LegacyEncoding {
    const DETECTION_CANDIDATES: [Self; 7] = [
        Self::Iso8859_5,
        Self::Windows1251,
        Self::Koi8R,
        Self::Ibm866,
        Self::Windows1252,
        Self::Windows1250,
        Self::Iso8859_2,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Iso8859_5 => "ISO-8859-5",
            Self::Windows1251 => "Windows-1251",
            Self::Koi8R => "KOI8-R",
            Self::Ibm866 => "IBM866",
            Self::Windows1252 => "Windows-1252",
            Self::Windows1250 => "Windows-1250",
            Self::Iso8859_2 => "ISO-8859-2",
        }
    }

    fn codec(self) -> &'static Encoding {
        match self {
            Self::Iso8859_5 => ISO_8859_5,
            Self::Windows1251 => WINDOWS_1251,
            Self::Koi8R => KOI8_R,
            Self::Ibm866 => IBM866,
            Self::Windows1252 => WINDOWS_1252,
            Self::Windows1250 => WINDOWS_1250,
            Self::Iso8859_2 => ISO_8859_2,
        }
    }

    fn script_family(self) -> ScriptFamily {
        match self {
            Self::Iso8859_5 | Self::Windows1251 | Self::Koi8R | Self::Ibm866 => {
                ScriptFamily::Cyrillic
            }
            Self::Windows1252 | Self::Windows1250 | Self::Iso8859_2 => ScriptFamily::Latin,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Legacy(LegacyEncoding),
}

impl TextEncoding {
    pub const fn status_label(self) -> Option<&'static str> {
        match self {
            Self::Utf8 | Self::Utf8Bom => None,
            Self::Utf16Le => Some("UTF-16 LE"),
            Self::Utf16Be => Some("UTF-16 BE"),
            Self::Legacy(encoding) => Some(encoding.display_name()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFileFormat {
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
}

impl Default for TextFileFormat {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            line_ending: if cfg!(windows) {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedTextFile {
    pub text: String,
    pub format: TextFileFormat,
}

pub fn read_text_file(path: &Path) -> io::Result<DecodedTextFile> {
    let bytes = fs::read(path)?;
    decode_text_bytes(&bytes)
}

pub fn decode_text_bytes(bytes: &[u8]) -> io::Result<DecodedTextFile> {
    let (encoding, raw_text) = if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (
            TextEncoding::Utf8Bom,
            std::str::from_utf8(bytes)
                .map_err(invalid_text_error)?
                .to_string(),
        )
    } else if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (TextEncoding::Utf16Le, decode_utf16(bytes, true)?)
    } else if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (TextEncoding::Utf16Be, decode_utf16(bytes, false)?)
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (TextEncoding::Utf8, text.to_string())
    } else {
        let (encoding, text) = decode_legacy_text(bytes)?;
        (TextEncoding::Legacy(encoding), text)
    };
    let line_ending = detect_line_ending(&raw_text);
    Ok(DecodedTextFile {
        text: normalize_line_endings(&raw_text),
        format: TextFileFormat {
            encoding,
            line_ending,
        },
    })
}

pub fn encode_text(text: &str, format: TextFileFormat) -> io::Result<Vec<u8>> {
    let external_text = match format.line_ending {
        LineEnding::Lf => text.to_string(),
        LineEnding::CrLf => text.replace('\n', "\r\n"),
        LineEnding::Cr => text.replace('\n', "\r"),
    };
    match format.encoding {
        TextEncoding::Utf8 => Ok(external_text.into_bytes()),
        TextEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(external_text.len() + 3);
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
            bytes.extend_from_slice(external_text.as_bytes());
            Ok(bytes)
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let little_endian = format.encoding == TextEncoding::Utf16Le;
            let mut bytes = Vec::with_capacity(external_text.len() * 2 + 2);
            bytes.extend_from_slice(if little_endian {
                &[0xff, 0xfe]
            } else {
                &[0xfe, 0xff]
            });
            for word in external_text.encode_utf16() {
                let encoded = if little_endian {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                };
                bytes.extend_from_slice(&encoded);
            }
            Ok(bytes)
        }
        TextEncoding::Legacy(encoding) => {
            let (bytes, _, had_errors) = encoding.codec().encode(&external_text);
            if had_errors {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "text contains characters that cannot be represented in {}",
                        encoding.display_name()
                    ),
                ));
            }
            Ok(bytes.into_owned())
        }
    }
}

pub fn write_text_file(path: &Path, text: &str, format: TextFileFormat) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && !parent.is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("parent directory does not exist: {}", parent.display()),
        ));
    }
    let bytes = encode_text(text, format)?;
    super::atomic_write(path, &bytes)
}

fn decode_legacy_text(bytes: &[u8]) -> io::Result<(LegacyEncoding, String)> {
    if legacy_bytes_look_binary(bytes) {
        return Err(invalid_text_error("binary-looking data"));
    }
    let non_ascii_bytes = bytes.iter().filter(|&&byte| byte >= 0x80).count();
    if non_ascii_bytes < LEGACY_MIN_NON_ASCII_BYTES {
        return Err(invalid_text_error("ambiguous legacy encoding"));
    }

    let mut best: Option<(LegacyEncoding, String, f32)> = None;
    let mut runner_up = f32::NEG_INFINITY;
    for encoding in LegacyEncoding::DETECTION_CANDIDATES {
        let Some(decoded) = encoding
            .codec()
            .decode_without_bom_handling_and_without_replacement(bytes)
        else {
            continue;
        };
        let text = decoded.into_owned();
        let score = legacy_text_score(&text, encoding);
        match best.as_ref() {
            None => best = Some((encoding, text, score)),
            Some((_, _, best_score)) if score > *best_score => {
                runner_up = runner_up.max(*best_score);
                best = Some((encoding, text, score));
            }
            Some(_) => runner_up = runner_up.max(score),
        }
    }

    let Some((encoding, text, best_score)) = best else {
        return Err(invalid_text_error("unsupported legacy encoding"));
    };
    if best_score < LEGACY_MIN_SCORE || best_score - runner_up < LEGACY_MIN_MARGIN {
        return Err(invalid_text_error("ambiguous legacy encoding"));
    }
    Ok((encoding, text))
}

fn legacy_bytes_look_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    let raw_controls = bytes
        .iter()
        .filter(|&&byte| {
            matches!(byte, 0x01..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f)
        })
        .count();
    raw_controls >= 2 && raw_controls.saturating_mul(20) >= bytes.len().max(1)
}

fn legacy_text_score(text: &str, encoding: LegacyEncoding) -> f32 {
    let family = encoding.script_family();
    let mut total = 0usize;
    let mut printable = 0usize;
    let mut bad = 0usize;
    let mut non_ascii_printable = 0usize;
    let mut alphabetic = 0usize;
    let mut ascii_alphabetic = 0usize;
    let mut non_ascii_alphabetic = 0usize;
    let mut matching_alphabetic = 0usize;
    let mut mismatching_alphabetic = 0usize;

    for ch in text.chars() {
        total += 1;
        if is_bad_decoded_char(ch) {
            bad += 1;
            continue;
        }
        printable += 1;
        if !ch.is_ascii() && !ch.is_whitespace() {
            non_ascii_printable += 1;
        }
        if !ch.is_alphabetic() {
            continue;
        }
        alphabetic += 1;
        if ch.is_ascii() {
            ascii_alphabetic += 1;
            continue;
        }
        non_ascii_alphabetic += 1;
        let script = script_kind(ch);
        if script_matches_family(script, family) {
            matching_alphabetic += 1;
        } else {
            mismatching_alphabetic += 1;
        }
    }

    let total = total.max(1) as f32;
    let mut score = 40.0 * printable as f32 / total - 80.0 * bad as f32 / total;
    if non_ascii_alphabetic > 0 {
        score += 28.0 * (matching_alphabetic as f32 - mismatching_alphabetic as f32)
            / non_ascii_alphabetic as f32;
    }
    if non_ascii_printable > 0 {
        score += 24.0 * matching_alphabetic as f32 / non_ascii_printable as f32;
    }
    if family == ScriptFamily::Latin && alphabetic >= 8 {
        let ascii_ratio = ascii_alphabetic as f32 / alphabetic as f32;
        if ascii_ratio < 0.22 {
            score -= 28.0 * (0.22 - ascii_ratio) / 0.22;
        }
    }
    score += case_coherence_score(text, family);
    score -= mixed_script_word_penalty(text, family);
    score -= unusual_intraword_symbol_penalty(text, family);
    score -= latin_diacritic_saturation_penalty(text, family);
    score
}

fn case_coherence_score(text: &str, family: ScriptFamily) -> f32 {
    if !matches!(family, ScriptFamily::Latin | ScriptFamily::Cyrillic) {
        return 0.0;
    }
    let mut words = 0usize;
    let mut plausible = 0usize;
    let mut length = 0usize;
    let mut upper = 0usize;
    let mut lower = 0usize;
    let mut first_upper = false;
    let mut has_non_ascii = false;
    let mut valid = true;

    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphabetic() {
            let script = script_kind(ch);
            valid &= script_matches_family(script, family);
            has_non_ascii |= !ch.is_ascii();
            if length == 0 {
                first_upper = ch.is_uppercase();
            }
            length += 1;
            upper += if ch.is_uppercase() { 1 } else { 0 };
            lower += if ch.is_lowercase() { 1 } else { 0 };
            continue;
        }
        if valid && has_non_ascii && length >= 2 && upper + lower == length {
            words += 1;
            if lower == length || upper == length || (first_upper && upper == 1) {
                plausible += 1;
            }
        }
        length = 0;
        upper = 0;
        lower = 0;
        first_upper = false;
        has_non_ascii = false;
        valid = true;
    }

    if words == 0 {
        0.0
    } else {
        12.0 * (2.0 * plausible as f32 / words as f32 - 1.0)
    }
}

fn mixed_script_word_penalty(text: &str, family: ScriptFamily) -> f32 {
    let mut mixed_words = 0usize;
    let mut has_expected = false;
    let mut has_other = false;
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphabetic() {
            if script_matches_family(script_kind(ch), family) {
                has_expected = true;
            } else {
                has_other = true;
            }
            continue;
        }
        if has_expected && has_other {
            mixed_words += 1;
        }
        has_expected = false;
        has_other = false;
    }
    (mixed_words as f32 * LEGACY_MIXED_SCRIPT_WORD_PENALTY)
        .min(LEGACY_MAX_MIXED_SCRIPT_PENALTY)
}

fn latin_diacritic_saturation_penalty(text: &str, family: ScriptFamily) -> f32 {
    if family != ScriptFamily::Latin {
        return 0.0;
    }
    let mut saturated_words = 0usize;
    let mut letters = 0usize;
    let mut non_ascii_letters = 0usize;
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphabetic() && script_kind(ch) == ScriptKind::Latin {
            letters += 1;
            if !ch.is_ascii() {
                non_ascii_letters += 1;
            }
            continue;
        }
        if letters >= 4 && non_ascii_letters.saturating_mul(5) >= letters.saturating_mul(4) {
            saturated_words += 1;
        }
        letters = 0;
        non_ascii_letters = 0;
    }
    (saturated_words as f32 * LEGACY_LATIN_SATURATED_WORD_PENALTY)
        .min(LEGACY_MAX_LATIN_SATURATION_PENALTY)
}

fn unusual_intraword_symbol_penalty(text: &str, family: ScriptFamily) -> f32 {
    let mut unusual = 0usize;
    let mut previous = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let next = chars.peek().copied();
        if !ch.is_alphanumeric()
            && !ch.is_whitespace()
            && !is_common_text_punctuation(ch)
            && previous.is_some_and(|previous| is_family_letter(previous, family))
            && next.is_some_and(|next| is_family_letter(next, family))
        {
            unusual += 1;
        }
        previous = Some(ch);
    }
    (unusual as f32 * LEGACY_INTRAWORD_SYMBOL_PENALTY)
        .min(LEGACY_MAX_INTRAWORD_SYMBOL_PENALTY)
}

fn is_bad_decoded_char(ch: char) -> bool {
    let code = ch as u32;
    (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        || (0xe000..=0xf8ff).contains(&code)
        || (0xf0000..=0xffffd).contains(&code)
        || (0x100000..=0x10fffd).contains(&code)
        || (0xfdd0..=0xfdef).contains(&code)
        || matches!(code & 0xffff, 0xfffe | 0xffff)
}

fn is_common_text_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation()
        || matches!(
            ch,
            '“' | '”' | '‘' | '’' | '«' | '»' | '…' | '—' | '–' | '‐' | '‑'
        )
}

fn is_family_letter(ch: char, family: ScriptFamily) -> bool {
    ch.is_alphabetic() && script_matches_family(script_kind(ch), family)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptFamily {
    Latin,
    Cyrillic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptKind {
    Latin,
    Cyrillic,
    Other,
}

fn script_kind(ch: char) -> ScriptKind {
    let code = ch as u32;
    if (0x0400..=0x052f).contains(&code) {
        ScriptKind::Cyrillic
    } else if (0x0370..=0x03ff).contains(&code)
        || (0x0590..=0x05ff).contains(&code)
        || (0x0600..=0x06ff).contains(&code)
        || (0x0750..=0x077f).contains(&code)
        || (0x08a0..=0x08ff).contains(&code)
    {
        ScriptKind::Other
    } else if ch.is_alphabetic() {
        ScriptKind::Latin
    } else {
        ScriptKind::Other
    }
}

fn script_matches_family(script: ScriptKind, family: ScriptFamily) -> bool {
    match family {
        ScriptFamily::Latin => script == ScriptKind::Latin,
        ScriptFamily::Cyrillic => script == ScriptKind::Cyrillic,
    }
}

fn invalid_text_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported or invalid text encoding: {error}"),
    )
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> io::Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(invalid_text_error("odd UTF-16 byte length"));
    }
    let words = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    std::char::decode_utf16(words)
        .map(|item| item.map_err(invalid_text_error))
        .collect()
}

fn detect_line_ending(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\r' if bytes.get(idx + 1) == Some(&b'\n') => {
                crlf += 1;
                idx += 2;
            }
            b'\r' => {
                cr += 1;
                idx += 1;
            }
            b'\n' => {
                lf += 1;
                idx += 1;
            }
            _ => idx += 1,
        }
    }
    if crlf >= lf && crlf >= cr && crlf > 0 {
        LineEnding::CrLf
    } else if lf >= cr && lf > 0 {
        LineEnding::Lf
    } else if cr > 0 {
        LineEnding::Cr
    } else {
        TextFileFormat::default().line_ending
    }
}

fn normalize_line_endings(text: &str) -> String {
    if !text.as_bytes().contains(&b'\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}
