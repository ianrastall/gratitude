//! Local show inspection, source catalogues and cancellable external-tool jobs.
//! Notes are data only; nothing in an imported TXT file is executed.
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const ARTIST: &str = "Grateful Dead";
pub const GENRE: &str = "Psychedelic Rock";
/// Gratitude covers the band's years with Jerry Garcia: its first shows through his death.
const FIRST_DATE: &str = "1965-05-05";
const LAST_DATE: &str = "1995-08-09";
/// Output tags as ffprobe names them, then the container fields it reports for M4A.
/// Any other name on a finished file is refused.
const ALLOWED_TAGS: [&str; 13] = [
    "ALBUM",
    "ARTIST",
    "COMMENT",
    "DATE",
    "GENRE",
    "TITLE",
    "TRACK",
    "COMPATIBLE_BRANDS",
    "HANDLER_NAME",
    "LANGUAGE",
    "MAJOR_BRAND",
    "MINOR_VERSION",
    "VENDOR_ID",
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub set: String,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub codec: String,
    pub bit_depth: u32,
    pub sample_format: String,
    pub channel_layout: String,
    /// BerthaML performance IDs this track holds, from linking; empty when unmatched.
    pub performances: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Draft {
    pub source_summary: String,
    pub credits: String,
    pub reference_info: String,
    pub folder: String,
    pub artist: String,
    pub date: String,
    pub venue: String,
    pub notes_path: String,
    pub notes: String,
    pub warnings: Vec<String>,
    pub tracks: Vec<Track>,
    pub album: String,
    pub comment: String,
    /// BerthaML show and recording IDs from linking, empty when not found.
    pub show_id: String,
    pub recording_id: String,
    /// What linking matched, in a sentence or two.
    pub link: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub venue: String,
    pub city: String,
    pub region: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Recording {
    pub title: String,
    pub source: String,
    pub url: String,
    pub key: String,
    pub detail: String,
    pub source_kind: String,
    pub source_summary: String,
    pub credits: String,
    pub date: String,
    pub venue: String,
    pub show_info: String,
    pub evidence: String,
    pub modifiers: Vec<String>,
    pub inferred: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Job {
    pub state: String,
    pub phase: String,
    pub log: String,
    pub output: String,
}
#[derive(Default)]
pub struct Service {
    status: Arc<Mutex<Job>>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    /// The resume store's folder, where a finished MKA's source evidence is recorded.
    store_dir: Mutex<Option<PathBuf>>,
}
impl Drop for Service {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}
fn tool(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("tools").join(format!("{name}.exe"))))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(format!("{name}.exe")))
}
pub(crate) fn rx(pattern: &str) -> Regex {
    static PATTERNS: std::sync::OnceLock<Mutex<std::collections::HashMap<String, Regex>>> = std::sync::OnceLock::new();
    PATTERNS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(pattern.to_owned())
        .or_insert_with(|| Regex::new(pattern).expect("static regex"))
        .clone()
}
/// Segue notation belongs in the original notes, not song/chapter names.
pub fn clean_title(title: &str) -> String {
    let arrow = r"(?:-+>|=+>|~+>|[>→⇒➔➜⟶↦]+)";
    let title = rx(&format!(r"\s*(?:{arrow}\s*)+$")).replace(title, "").into_owned();
    let title = rx(&format!(r"^(?:\s*{arrow})+\s*")).replace(&title, "").into_owned();
    rx(&format!(r"\s*{arrow}\s*")).replace_all(title.trim(), " / ").trim().to_owned()
}
/// Disc/track codes such as d1t01, s2t03, cd1 t04 or track 7, with an optional show prefix.
const TRACK_CODE: &str = r"(?:gd\d{2,4}[-.]?\d{2}[-.]?\d{2}[a-z]*\.?)?(?:(?:disc|disk|cd|d)\s*\d+\s*[-_.]?\s*(?:track|trk|t)\s*\d+|s(?:et)?\s*\d+\s*[-_.]?\s*t\d+|(?:track|trk|t)\s*\d+)";
/// Removes every leading track label, so 101-d1t01 - Bertha and 01. 01 Bertha both
/// leave Bertha. A later bare number stays when it could belong to a title
/// (03 500 Miles); it goes when it has punctuation or repeats the first number.
fn strip_labels(line: &str) -> &str {
    let code = rx(&format!(r"(?i)^{TRACK_CODE}(?:\s*[-.:)_]+\s*|\s+|$)"));
    let number = rx(r"^(\d{1,3})(\s*[-.:)_]+\s*|\s+|$)");
    let encore = rx(r"(?i)^(?:e\d?|encore(?:\s*\d)?)\s*[:\-]\s*");
    let timing = rx(r"^\d{1,3}:\d{2}");
    let mut rest = line.trim();
    let mut first: Option<&str> = None;
    loop {
        if let Some(m) = code.find(rest).or_else(|| encore.find(rest)) {
            rest = &rest[m.end()..];
            continue;
        }
        let Some(c) = number.captures(rest).filter(|_| !timing.is_match(rest)) else { break };
        let digits = c.get(1).map_or("", |m| m.as_str());
        let punctuated = !c[2].trim().is_empty();
        if first.is_some_and(|f| !punctuated && !f.ends_with(digits.trim_start_matches('0'))) {
            break;
        }
        first.get_or_insert(digits);
        rest = &rest[c.get(0).map_or(0, |m| m.end())..];
    }
    rest
}
/// The song name and nothing else. Track labels, timings, footnote marks, performance
/// remarks and segues are all notes about a song, whichever source they arrive from.
pub fn song_title(raw: &str) -> String {
    let trailing = rx(concat!(
        r"(?i)(?:\s*[\[({]?\s*\d{1,3}:\d{2}(?:[:.]\d+)?\s*[\])}]?",
        r"|\s*[\[(]\s*(?:\d{1,2}|[*#%^+†‡~@]+)\s*[\])]",
        r"|\s*[\[(]\s*(?:w/|(?:cut|partial|incomplete|fades?|faded|fade-in|fade-out|false start|tape|splice|patch|with|feat|featuring|guest|missing)\b)[^\])]*[\])]",
        r"|\s*[*#%^+†‡~@=]+",
        r"|\s*(?:-+>|=+>|~+>|[>→⇒➔➜⟶↦]+)",
        r"|\s+-+)\s*$"
    ));
    let mut title = strip_labels(raw.trim().trim_start_matches(['*', '#', '~', '='])).to_owned();
    loop {
        let shorter = trailing.replace(&title, "").into_owned();
        if shorter == title {
            break;
        }
        title = shorter;
    }
    clean_title(&title).split_whitespace().collect::<Vec<_>>().join(" ")
}
/// A song name from a file name, where _ stands in for characters Windows forbids.
fn title_from_file(stem: &str) -> String {
    song_title(&rx(r"(\p{L})_s\b").replace_all(stem, "$1's").replace('_', " "))
}
fn slug(text: &str) -> String {
    let text = text.to_lowercase().replace('&', " and ").replace(['\'', '’', '.'], "");
    text.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-")
}
/// The show descriptor used as the album: YYMMDD_region-city-venue.
pub fn album_name(date: &str, place: &Place) -> String {
    let stamp = date.get(2..).unwrap_or_default().replace('-', "");
    let parts = [&place.region, &place.city, &place.venue].map(|s| slug(s));
    format!("{stamp}_{}", parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-"))
}
fn place_from_venue(venue: &str) -> Place {
    match rx(r"^(.*?)\s*[-,]\s*([^,-]+?)(?:\s*,\s*|\s+)([A-Za-z]{2})\.?$").captures(venue.trim()) {
        Some(c) => Place { venue: c[1].to_owned(), city: c[2].to_owned(), region: c[3].to_owned() },
        None => Place { venue: venue.trim().to_owned(), ..Default::default() },
    }
}
pub(crate) fn date_from_name(name: &str) -> Option<String> {
    let c = rx(r"(?:^|\D)((?:19)?[6-9]\d)[-._]?(0[1-9]|1[0-2])[-._]?(0[1-9]|[12]\d|3[01])(?:\D|$)").captures(name)?;
    Some(format!("19{}-{}-{}", &c[1][c[1].len() - 2..], &c[2], &c[3]))
}
fn in_era(date: &str) -> bool {
    rx(r"^\d{4}-\d{2}-\d{2}$").is_match(date) && (FIRST_DATE..=LAST_DATE).contains(&date)
}
/// Taper and equipment for the comment tag, from the notes' own labels.
fn recording_comment(notes: &str, known_tapers: &[String]) -> String {
    let lines: Vec<_> = notes.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let field = |pattern: &str| {
        let pattern = rx(pattern);
        lines.iter().enumerate().find_map(|(i, line)| {
            let value = pattern.captures(line)?.get(1).map_or("", |m| m.as_str()).trim();
            let value = if value.is_empty() { lines.get(i + 1).copied().unwrap_or_default() } else { value };
            (!value.is_empty()).then(|| value.to_owned())
        })
    };
    let equipment = field(r"(?i)^(?:source(?:\s*\d+)?(?:\s+info)?|src|recording(?:\s+info)?|equipment|taping equipment|mics?|microphones?|gear)\s*:\s*(.*)$")
        .or_else(|| {
            lines
                .iter()
                .find(|l| {
                    rx(r"(?i)^\(?(?:sbd|aud|fob|mtx|matrix|fm|master|reels?|cass\w*|dat|mics?)\b.*(?:->|>|→)").is_match(l)
                })
                .map(|l| (*l).to_owned())
        })
        .or_else(|| field(r"(?i)^lineage\s*:\s*(.*)$"))
        // A source line can run on into thanks and history; the chain ends at its first sentence.
        .map(|e| rx(r"\.\s*[A-Z][a-z]").find(&e).map_or(e.clone(), |m| e[..m.start()].to_owned()));
    let taper = field(r"(?i)^(?:taper|taped by|recorded by|recording by|recordist)\s*[:\-]?\s*(.+)$")
        .or_else(|| field(r"(?i)\b(?:taped|recorded)\s+by\s*:?\s+([^,;.()]+)"))
        .map(|t| rx(r"[\w.+-]+@[\w.-]+\.[A-Za-z]+").replace_all(&t, "").trim().to_owned())
        .filter(|t| !t.is_empty())
        .or_else(|| (!known_tapers.is_empty()).then(|| known_tapers.join(", ")));
    [taper.map(|t| format!("Taper: {t}")), equipment.map(|e| format!("Equipment: {e}"))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")
}
fn probe_audio(path: &Path) -> Result<Value, String> {
    let output=hidden(&mut Command::new(tool("ffprobe"))).args(["-v","error","-select_streams","a:0","-show_entries","stream=codec_name,sample_rate,channels,sample_fmt,channel_layout,bits_per_sample,bits_per_raw_sample:format=duration:format_tags=title:stream_tags=title","-of","json"]).arg(path).output().map_err(|e|format!("ffprobe could not start: {e}"))?;
    if !output.status.success() {
        return Err(format!("Cannot inspect {}: {}", path.display(), String::from_utf8_lossy(&output.stderr)));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn source_depth(stream: &Value) -> u32 {
    fn number(v: &Value) -> u32 {
        v.as_u64().map(|n| n as u32).or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
    }
    let bits = number(&stream["bits_per_raw_sample"]).max(number(&stream["bits_per_sample"]));
    if bits > 0 {
        bits
    } else {
        match stream["sample_fmt"].as_str().unwrap_or_default() {
            "u8" | "u8p" => 8,
            "s16" | "s16p" => 16,
            "s32" | "s32p" => 32,
            _ => 0,
        }
    }
}
fn validate_audio(tracks: &[Track], copy: bool) -> Result<(), String> {
    let first = tracks.first().ok_or("No tracks to package.")?;
    if first.sample_rate == 0 || first.channels == 0 {
        return Err("The source sample rate or channel count is unknown. Packaging cannot preserve it safely.".into());
    }
    if tracks.iter().any(|t| {
        t.sample_rate != first.sample_rate
            || t.channels != first.channels
            || t.bit_depth != first.bit_depth
            || t.channel_layout != first.channel_layout
            || t.sample_format != first.sample_format
            || t.codec != first.codec
    }) {
        return Err("Tracks must have matching codecs and identical sample rates, bit depths and channel layouts. Show Player never resamples or changes precision to make them match.".into());
    }
    if !copy
        && (!matches!(first.bit_depth, 16 | 24)
            || !tracks.iter().all(|t| {
                matches!(t.codec.as_str(), "flac" | "shorten" | "alac")
                    || t.codec.starts_with("pcm_s16")
                    || t.codec.starts_with("pcm_s24")
            }))
    {
        return Err("FLAC conversion requires lossless 16-bit or 24-bit integer audio. Use stream copy to preserve other source formats.".into());
    }
    Ok(())
}
fn validate_probed(track: &Track, v: &Value) -> Result<(), String> {
    let s = &v["streams"][0];
    let rate = s["sample_rate"].as_str().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    if rate != track.sample_rate
        || source_depth(s) != track.bit_depth
        || s["channels"].as_u64().unwrap_or(0) != u64::from(track.channels)
        || s["codec_name"].as_str().unwrap_or_default() != track.codec
        || s["sample_fmt"].as_str().unwrap_or_default() != track.sample_format
        || s["channel_layout"].as_str().unwrap_or_default() != track.channel_layout
    {
        return Err(format!(
            "Audio format changed or does not match the reviewed import: {}. Import the folder again; no resampling will be performed.",
            track.path
        ));
    }
    Ok(())
}
fn natural_key(s: &str) -> String {
    rx(r"\d+").replace_all(&s.to_lowercase(), |c: &regex::Captures<'_>| format!("{:0>12}", &c[0])).into_owned()
}
fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|s| s.to_str()).is_some_and(|e| {
        ["flac", "shn", "wav", "aiff", "aif", "mp3", "ogg", "opus", "m4a"].contains(&e.to_lowercase().as_str())
    })
}
fn collect(folder: &Path, depth: u32, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 4 {
        return Ok(());
    }
    for e in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let t = e.file_type().map_err(|e| e.to_string())?;
        if t.is_file() {
            files.push(e.path());
        } else if t.is_dir() {
            collect(&e.path(), depth + 1, files)?;
        }
        if files.len() > 10000 {
            return Err("Choose one show folder with fewer than 10,000 files.".into());
        }
    }
    Ok(())
}
pub fn parse_notes(notes: &str) -> (String, String, Vec<(String, String)>) {
    let preamble = rx(r"(?i)^(?:this is (?:a |an )?(?:flac|shn|lossless)|shnid\s*:|encoding notes\s*:)");
    let lines: Vec<_> = notes.lines().map(str::trim).filter(|l| !l.is_empty() && !preamble.is_match(l)).collect();
    let date_re = rx(r"\b((?:19|20)\d{2})[-/.](\d{1,2})[-/.](\d{1,2})\b");
    let us_re = rx(r"\b(\d{1,2})[-/](\d{1,2})[-/]((?:19|20)?\d{2})\b");
    let month_re = rx(r"(?i)\b([a-z]+)\.?\s+(\d{1,2}),?\s+((?:19|20)\d{2})\b");
    let mut date = String::new();
    let mut venue = String::new();
    for (i, line) in lines.iter().take(12).enumerate() {
        if let Some(c) = date_re.captures(line) {
            date = format!("{}-{:0>2}-{:0>2}", &c[1], &c[2], &c[3]);
        } else if let Some(c) = us_re.captures(line) {
            let y: u32 = c[3].parse().unwrap_or(0);
            let y = if y < 100 { if y >= 65 { 1900 + y } else { 2000 + y } } else { y };
            date = format!("{y}-{:0>2}-{:0>2}", &c[1], &c[2]);
        } else if let Some(c) = month_re.captures(line) {
            let m = c[1].to_lowercase();
            let month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
                .iter()
                .position(|p| m.starts_with(p));
            if let Some(month) = month {
                date = format!("{}-{:02}-{:0>2}", &c[3], month + 1, &c[2]);
            } else {
                continue;
            }
        } else {
            continue;
        }
        if i > 1 {
            venue = lines[1..i].join(", ");
        } else if i + 1 < lines.len() {
            venue = lines[i + 1].to_string();
        }
        break;
    }
    let mut set = String::new();
    let mut set_numbered = false;
    let mut tracks: Vec<(String, String)> = Vec::new();
    for line in lines {
        if missing_song(line).is_some() {
            continue;
        }
        // Checksums and tool output may sit above or below the setlist.
        if rx(r"(?i)^(?:;\s*)?(?:shntool|flac fingerprints?|md5|ffp|st5|checksums?)\b").is_match(line) {
            if tracks.is_empty() {
                continue;
            }
            break;
        }
        if rx(r"^\d{1,3}:\d{2}(?:[.:]\d+)?\s").is_match(line)
            || rx(r"(?i)\.(?:shn|flac|wav|mp3|md5|ffp|st5)\b").is_match(line)
            || rx(r"(?i)^\d+\s*(?:bit|khz|hz|discs?|cds?|mins?|minutes?|hours?|tracks?|channels?)\b").is_match(line)
        {
            continue;
        }
        let heading = line.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '=' | ':'));
        let heading = rx(r"(?i)^(?:disc|disk|cd)\s*(?:\d+|one|two|three|i{1,3})\s*[-:,]?\s*").replace(heading, "");
        if heading.is_empty() {
            continue;
        }
        if let Some(c) = rx(
            r"(?i)^(set\s*(?:one|two|three|i{1,3}|[123])|(?:first|second|third|1st|2nd|3rd)\s+set|encore)(?:\s*\(?cont\S*)?$",
        )
        .captures(&heading)
        {
            set = c[1].to_string();
            set_numbered = false;
            continue;
        }
        if rx(r"(?i)^(?:source(?:\s*\d+)?|lineage|notes?|recording|transfer|checksums|comments?|guests?|credits|taper|thanks)\b[^:]*:").is_match(line) {
            // One or two numbered lines before this are notes such as "2 Source Matrix".
            if tracks.len() >= 3 || tracks.iter().any(|t| !t.1.is_empty()) {
                break;
            }
            tracks.clear();
            set.clear();
            set_numbered = true;
            continue;
        }
        if let Some(c) = rx(r"(?i)^(?:e|encore)\s*:\s*(.+)$").captures(line) {
            tracks.push((song_title(&c[1]), "Encore".into()));
            set.clear();
            continue;
        }
        let song = strip_labels(line);
        if song.len() != line.len() {
            set_numbered = true;
            // Dates, hashes and bare timings are not song titles.
            let title = song_title(song);
            if title.chars().any(char::is_alphabetic) && !rx(r"^[0-9a-fA-F]{32,64}(?:\s|$)").is_match(&title) {
                tracks.push((title, set.clone()));
            }
        } else if !set.is_empty() && !set_numbered && !line.starts_with(['*', '#', '-', '_', '=']) {
            tracks.push((song_title(line), set.clone()));
        }
    }
    (date, venue, tracks)
}
fn missing_song(line: &str) -> Option<String> {
    rx(r"(?i)^(?:\(\s*missing\s*\)|\[\s*missing\s*\]|missing\s*[-:])\s*[-:]?\s*(.+)$")
        .captures(line.trim())
        .map(|c| c[1].trim().to_owned())
}
fn embedded_title(value: &Value) -> Option<String> {
    [
        value.get("format").and_then(|f| f.get("tags")),
        value.get("streams").and_then(|s| s.get(0)).and_then(|s| s.get("tags")),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_object)
    .flat_map(|tags| tags.iter())
    .find(|(key, title)| key.eq_ignore_ascii_case("title") && title.as_str().is_some_and(|s| !s.trim().is_empty()))
    .and_then(|(_, v)| v.as_str())
    .map(song_title)
    .filter(|s| !s.is_empty())
}
/// Shorten carries no duration, so its samples are counted by decoding.
fn decoded_seconds(path: &Path, sample_rate: u32, channels: u32) -> Result<f64, String> {
    let mut child = hidden(Command::new(tool("ffmpeg")).args(["-v", "error", "-nostdin", "-i"]).arg(path))
        .args(["-map", "0:a:0", "-c:a", "pcm_s16le", "-f", "s16le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg could not start: {e}"))?;
    let bytes = std::io::copy(&mut child.stdout.take().ok_or("ffmpeg gave no output.")?, &mut std::io::sink())
        .map_err(|e| e.to_string())?;
    if !child.wait().map_err(|e| e.to_string())?.success() || bytes == 0 || sample_rate == 0 || channels == 0 {
        return Err(format!("Cannot decode {}", path.display()));
    }
    Ok(bytes as f64 / 2.0 / f64::from(channels) / f64::from(sample_rate))
}
/// A hash of the decoded samples at full precision, for comparing a copy with its source.
fn audio_hash(path: &Path) -> Result<String, String> {
    let output = hidden(Command::new(tool("ffmpeg")).args(["-v", "error", "-nostdin", "-i"]).arg(path))
        .args(["-map", "0:a:0", "-c:a", "pcm_s32le", "-f", "md5", "-"])
        .output()
        .map_err(|e| format!("ffmpeg could not start: {e}"))?;
    if !output.status.success() {
        return Err(format!("Cannot decode {}: {}", path.display(), String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
/// Every tag name present on a file, at file and stream level.
fn tag_names(path: &Path) -> Result<Vec<String>, String> {
    let output = hidden(Command::new(tool("ffprobe")).args([
        "-v",
        "error",
        "-show_entries",
        "format_tags:stream_tags",
        "-of",
        "json",
    ]))
    .arg(path)
    .output()
    .map_err(|e| format!("ffprobe could not start: {e}"))?;
    let v: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let mut names: Vec<_> = std::iter::once(&v["format"])
        .chain(streams.iter())
        .filter_map(|n| n["tags"].as_object())
        .flat_map(|tags| tags.keys().map(|k| k.to_uppercase()))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}
/// `head` with its FLAC DESCRIPTION field renamed COMMENT, at the same length: the
/// padding block that follows the tags takes up the bytes saved.
fn flac_comment_renamed(head: &[u8]) -> Option<Vec<u8>> {
    const OLD: &[u8] = b"DESCRIPTION=";
    const NEW: &[u8] = b"COMMENT=";
    let saved = OLD.len() - NEW.len();
    let le32 = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?) as usize);
    let mut block = head.starts_with(b"fLaC").then_some(4)?;
    loop {
        let flags = *head.get(block)?;
        let size =
            u32::from_be_bytes([0, *head.get(block + 1)?, *head.get(block + 2)?, *head.get(block + 3)?]) as usize;
        let end = block + 4 + size;
        if flags & 0x7F == 4 {
            let mut entry = block + 8 + le32(block + 4)?;
            let count = le32(entry)?;
            entry += 4;
            for _ in 0..count {
                let length = le32(entry)?;
                if head.get(entry + 4..entry + 4 + OLD.len())?.eq_ignore_ascii_case(OLD) {
                    if head.get(end)? & 0x7F != 1 {
                        return None;
                    }
                    let padding =
                        u32::from_be_bytes([0, *head.get(end + 1)?, *head.get(end + 2)?, *head.get(end + 3)?]);
                    let mut out = head[..block + 1].to_vec();
                    out.extend_from_slice(&((size - saved) as u32).to_be_bytes()[1..]);
                    out.extend_from_slice(&head[block + 4..entry]);
                    out.extend_from_slice(&((length - saved) as u32).to_le_bytes());
                    out.extend_from_slice(NEW);
                    out.extend_from_slice(&head[entry + 4 + OLD.len()..end + 1]);
                    out.extend_from_slice(&(padding + saved as u32).to_be_bytes()[1..]);
                    out.extend_from_slice(&[0; 4][..saved]);
                    out.extend_from_slice(&head[end + 4..]);
                    return Some(out);
                }
                entry += 4 + length;
            }
            return None;
        }
        if flags & 0x80 != 0 {
            return None;
        }
        block = end;
    }
}
/// FFmpeg stores a FLAC comment as DESCRIPTION, which taggers list apart from COMMENT.
fn rename_flac_comment(path: &Path) -> Result<(), String> {
    use std::io::{Read, Seek};
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let mut head = Vec::new();
    (&mut file).take(1 << 20).read_to_end(&mut head).map_err(|e| e.to_string())?;
    let renamed =
        flac_comment_renamed(&head).ok_or_else(|| format!("Cannot set the comment on {}.", path.display()))?;
    file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.write_all(&renamed).map_err(|e| e.to_string())
}
fn file_name_part(title: &str) -> String {
    let name: String = title
        .replace(" / ", " - ")
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect();
    name.trim().trim_end_matches('.').to_owned()
}
/// Source extensions that can hold tags keep their format; the rest become FLAC.
fn tagged_extension(path: &str) -> Option<String> {
    let extension = Path::new(path).extension()?.to_str()?.to_lowercase();
    ["flac", "mp3", "ogg", "opus", "m4a"].contains(&extension.as_str()).then_some(extension)
}
/// The exact tag set a file carries; nothing else is written.
fn tags_for(draft: &Draft, index: usize) -> [(&'static str, String); 7] {
    [
        ("title", song_title(&draft.tracks[index].title)),
        ("album", draft.album.clone()),
        ("date", draft.date.clone()),
        ("track", track_number(index, draft.tracks.len())),
        ("artist", ARTIST.to_owned()),
        ("comment", draft.comment.clone()),
        ("genre", GENRE.to_owned()),
    ]
}
/// One running number for the whole show, padded to the digit count of the last.
fn track_number(index: usize, total: usize) -> String {
    format!("{:0width$}", index + 1, width = total.to_string().len())
}
/// The fields both outputs need before any work starts.
fn validate_tags(draft: &Draft) -> Result<(), String> {
    if draft.tracks.is_empty() {
        return Err("No tracks to package.".into());
    }
    if !in_era(&draft.date) {
        return Err(format!("Enter a date from {FIRST_DATE} through {LAST_DATE}, written as YYYY-MM-DD."));
    }
    if !draft.album.starts_with(&format!("{}_", draft.date[2..].replace('-', ""))) {
        return Err("The album must start with the show date as YYMMDD_.".into());
    }
    if let Some(i) = draft.tracks.iter().position(|t| song_title(&t.title).is_empty()) {
        return Err(format!("Track {} has no song title.", i + 1));
    }
    Ok(())
}
fn stores_in_matroska(tracks: &[Track]) -> bool {
    tracks.iter().all(|t| t.codec != "shorten")
}
/// Lossless FLAC settings that hold a source's own rate and depth.
fn add_flac_format(cmd: &mut Command, source: &Track) {
    cmd.args(["-sample_fmt", if source.bit_depth == 16 { "s16" } else { "s32" }]).args([
        "-ar",
        &source.sample_rate.to_string(),
        "-bits_per_raw_sample",
        &source.bit_depth.to_string(),
    ]);
}
/// canonicalize answers in \\?\ form, which folder pickers refuse.
fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(share) => PathBuf::from(format!(r"\\{share}")),
        None => PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)),
    }
}
pub fn inspect(folder: &Path, notes_override: &str) -> Result<Draft, String> {
    let folder = plain_path(&fs::canonicalize(folder).map_err(|e| e.to_string())?);
    let mut files = Vec::new();
    collect(&folder, 0, &mut files)?;
    files.sort_by_key(|p| natural_key(&p.to_string_lossy()));
    let mut notes: Vec<_> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")))
        .filter(|p| !p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains("ffp"))
        .filter(|p| !p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains("md5"))
        .filter(|p| !p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains("st5"))
        .collect();
    notes.sort_by_key(|p| std::cmp::Reverse(p.metadata().map(|m| m.len()).unwrap_or(0)));
    let note_path = if !notes_override.is_empty() {
        PathBuf::from(notes_override)
    } else {
        notes.first().map(|p| (*p).clone()).unwrap_or_default()
    };
    let bytes = if note_path.is_file() { fs::read(&note_path).map_err(|e| e.to_string())? } else { Vec::new() };
    let text = String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string();
    let (date, venue, titles) = parse_notes(&text);
    let audio: Vec<_> = files.iter().filter(|p| is_audio(p)).cloned().collect();
    if audio.is_empty() {
        return Err("No supported audio files were found in this show folder.".into());
    }
    let date = Some(date)
        .filter(|d| !d.is_empty())
        .or_else(|| date_from_name(&folder.file_name().unwrap_or_default().to_string_lossy()))
        .or_else(|| date_from_name(&note_path.file_name().unwrap_or_default().to_string_lossy()))
        .unwrap_or_default();
    let mut draft = Draft {
        folder: folder.to_string_lossy().into(),
        artist: ARTIST.into(),
        date,
        venue,
        notes_path: note_path.to_string_lossy().into(),
        notes: text,
        ..Default::default()
    };
    for song in draft.notes.lines().filter_map(missing_song) {
        draft.warnings.push(format!("TXT marks {song} as missing; no track was added for it."));
    }
    if titles.len() != audio.len() {
        draft.warnings.push(format!(
            "TXT contains {} song entries; folder contains {} audio files. Embedded song titles are used where available. Review every track title and order.",
            titles.len(),
            audio.len()
        ));
    }
    if notes.len() > 1 && notes_override.is_empty() {
        draft.warnings.push(format!(
            "{} TXT files found; selected {}. Use Choose TXT to select another.",
            notes.len(),
            note_path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    if std::str::from_utf8(&bytes).is_err() {
        draft.warnings.push("Notes are not UTF-8; the original bytes will still be attached unchanged.".into());
    }
    for (i, path) in audio.iter().enumerate() {
        let v = probe_audio(path)?;
        let s = &v["streams"][0];
        let sample_rate = s["sample_rate"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
        let channels = s["channels"].as_u64().unwrap_or(0) as u32;
        let duration = match v["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
        {
            Some(duration) => duration,
            None => decoded_seconds(path, sample_rate, channels)?,
        };
        let title = if titles.len() == audio.len() {
            titles[i].0.clone()
        } else {
            embedded_title(&v)
                .unwrap_or_else(|| title_from_file(&path.file_stem().unwrap_or_default().to_string_lossy()))
        };
        let set = if titles.len() == audio.len() { titles[i].1.clone() } else { String::new() };
        draft.tracks.push(Track {
            path: path.to_string_lossy().into(),
            title,
            set,
            duration,
            sample_rate,
            channels,
            codec: s["codec_name"].as_str().unwrap_or_default().into(),
            bit_depth: source_depth(s),
            sample_format: s["sample_fmt"].as_str().unwrap_or_default().into(),
            channel_layout: s["channel_layout"].as_str().unwrap_or_default().into(),
            performances: Vec::new(),
        });
    }
    if let Err(reason) = validate_audio(&draft.tracks, false) {
        draft.warnings.push(reason);
    }
    if !stores_in_matroska(&draft.tracks) {
        draft.warnings.push(
            "SHN holds no tags and does not fit in MKA, so it is stored as FLAC at its original rate and depth. Samples are unchanged."
                .into(),
        );
    }
    // Archive folder names state the source; notes may only mention one, as with an audience patch.
    let name = folder.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let profile = crate::catalogue::classify(&name, &draft.notes, "", "", "", "");
    draft.comment = recording_comment(&draft.notes, &crate::catalogue::known_tapers(&profile));
    if draft.comment.is_empty() {
        draft.warnings.push("The notes name no taper or equipment. Fill in the comment by hand.".into());
    }
    if !in_era(&draft.date) {
        draft.warnings.push(format!(
            "Gratitude covers shows through {LAST_DATE}. Enter the date of this show as YYYY-MM-DD before creating output."
        ));
    }
    draft.source_summary = profile.summary();
    draft.credits = profile.credits;
    let place = place_from_venue(&draft.venue);
    draft.set_place(&place);
    Ok(draft)
}
impl Draft {
    /// Takes a BerthaML link: IDs, its description and each track's performances.
    pub fn set_link(&mut self, link: &crate::linking::Link) {
        self.show_id = link.show.clone();
        self.recording_id = link.recording.clone();
        self.link = link.detail.clone();
        for (i, track) in self.tracks.iter_mut().enumerate() {
            track.performances = link.performances.get(i).cloned().unwrap_or_default();
        }
    }
    /// Names the show from a place. The offline catalogue's spelling replaces the notes' when it has the date.
    pub fn set_place(&mut self, place: &Place) {
        self.album = album_name(&self.date, place);
    }
}
impl Service {
    /// Records each finished MKA's source folder evidence in the resume store in `dir`.
    pub fn record_sources_in(&self, dir: PathBuf) {
        *self.store_dir.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir);
    }
    pub fn snapshot(&self) -> Job {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn busy(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
    pub fn shutdown(&self) {
        self.cancel();
        let until = std::time::Instant::now() + Duration::from_secs(2);
        while self.busy() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    fn start<F>(&self, phase: &str, output: String, work: F) -> Result<Job, String>
    where
        F: FnOnce(Arc<AtomicBool>, Arc<Mutex<Job>>) -> Result<(), String> + Send + 'static,
    {
        if self.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err("A job is already running. Cancel it or wait for completion.".into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) =
            Job { state: "Running".into(), phase: phase.into(), output, ..Default::default() };
        let initial = self.snapshot();
        let (cancel, status, running) = (self.cancel.clone(), self.status.clone(), self.running.clone());
        std::thread::spawn(move || {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(cancel.clone(), status.clone())))
                    .unwrap_or_else(|_| Err("The job failed unexpectedly.".into()));
            let mut job = status.lock().unwrap_or_else(|e| e.into_inner());
            job.state = if cancel.load(Ordering::Relaxed) {
                "Cancelled"
            } else if outcome.is_ok() {
                "Completed"
            } else {
                "Failed"
            }
            .into();
            if let Err(e) = outcome {
                job.log.push_str(&format!("\n{e}"));
            }
            running.store(false, Ordering::SeqCst);
        });
        Ok(initial)
    }
    pub fn package(&self, draft: Draft, output: PathBuf, copy_audio: bool) -> Result<Job, String> {
        if output.extension().is_none_or(|x| !x.eq_ignore_ascii_case("mka")) {
            return Err("Output must end in .mka.".into());
        }
        if output.exists() {
            return Err("The output already exists. Choose another name.".into());
        }
        validate_tags(&draft)?;
        // Matroska has no Shorten mapping, so FLAC is the lossless way in.
        let copy_audio = copy_audio && stores_in_matroska(&draft.tracks);
        validate_audio(&draft.tracks, copy_audio)?;
        for t in &draft.tracks {
            if !t.duration.is_finite() || t.duration <= 0.0 || !Path::new(&t.path).is_file() {
                return Err("A track is missing or has an invalid duration.".into());
            }
        }
        let output_string = output.to_string_lossy().into_owned();
        // The folder's names are gathered now, while it is certainly there.
        let evidence = serde_json::to_string(&crate::linking::import_evidence(&draft)).map_err(|e| e.to_string())?;
        let store_dir = self.store_dir.lock().unwrap_or_else(|e| e.into_inner()).clone();
        self.start("Creating MKA", output_string, move |cancel, status| {
            for track in &draft.tracks {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                validate_probed(track, &probe_audio(Path::new(&track.path))?)?;
            }
            let parent = output.parent().ok_or("Choose an output folder.")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let temp =
                tempfile::Builder::new().prefix(".show-player-").tempdir_in(parent).map_err(|e| e.to_string())?;
            let list = temp.path().join("tracks.ffconcat");
            let mut concat = String::from("ffconcat version 1.0\n");
            let mut meta = String::from(";FFMETADATA1\n");
            fn escape(s: &str) -> String {
                s.replace('\\', "\\\\")
                    .replace('=', "\\=")
                    .replace(';', "\\;")
                    .replace('#', "\\#")
                    .replace('\r', "")
                    .replace('\n', "\\\n")
            }
            // The segment title names the file; song titles and numbers belong to chapters.
            meta += &format!(
                "title={}\nalbum={}\nartist={}\ndate={}\ncomment={}\ngenre={}\n",
                escape(&draft.album),
                escape(&draft.album),
                escape(ARTIST),
                escape(&draft.date),
                escape(&draft.comment),
                escape(GENRE)
            );
            // Shorten packets carry no timestamps, so joining them directly keeps only the
            // first file. Each one goes in as a checked FLAC instead.
            let convert = !stores_in_matroska(&draft.tracks);
            let copy_audio = copy_audio || convert;
            let mut elapsed = 0.0;
            for (i, t) in draft.tracks.iter().enumerate() {
                if t.path.contains(['\n', '\r']) {
                    return Err("Track paths cannot contain line breaks.".into());
                }
                let mut source = t.path.clone();
                if convert {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Cancelled".into());
                    }
                    status.lock().unwrap_or_else(|e| e.into_inner()).phase =
                        format!("Converting track {} of {} to FLAC", i + 1, draft.tracks.len());
                    let part = temp.path().join(format!("{i}.flac"));
                    let mut cmd = Command::new(tool("ffmpeg"));
                    cmd.args(["-xerror", "-hide_banner", "-nostdin", "-n", "-i"]).arg(&t.path).args([
                        "-map",
                        "0:a:0",
                        "-map_metadata",
                        "-1",
                        "-c:a",
                        "flac",
                    ]);
                    add_flac_format(&mut cmd, t);
                    cmd.arg(&part);
                    run(&mut cmd, &temp.path().join("ffmpeg.log"), &cancel, &status)?;
                    if audio_hash(&part)? != audio_hash(Path::new(&t.path))? {
                        return Err(format!("{} did not match its source. Nothing was saved.", t.path));
                    }
                    source = part.to_string_lossy().into_owned();
                }
                let p = source.trim_start_matches(r"\\?\").replace('\\', "/").replace('\'', "'\\''");
                concat += &format!("file '{p}'\nduration {:.9}\n", t.duration);
                let start = (elapsed * 1000.0_f64).round() as u64;
                elapsed += t.duration;
                meta += &format!(
                    "[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={}\ntitle={}\ntrack={}\n",
                    (elapsed * 1000.0).round() as u64,
                    escape(&song_title(&t.title)),
                    track_number(i, draft.tracks.len())
                );
            }
            fs::write(&list, concat).map_err(|e| e.to_string())?;
            let metadata = temp.path().join("show.ffmeta");
            fs::write(&metadata, meta).map_err(|e| e.to_string())?;
            let partial = temp.path().join("show.mka");
            let mut cmd = Command::new(tool("ffmpeg"));
            cmd.arg("-noauto_conversion_filters").arg("-xerror");
            cmd.args(["-hide_banner", "-nostdin", "-n", "-f", "concat", "-safe", "0", "-i"])
                .arg(&list)
                .args(["-f", "ffmetadata", "-i"])
                .arg(&metadata)
                .args([
                    "-map",
                    "0:a:0",
                    "-map_metadata",
                    "1",
                    "-map_metadata:s:a",
                    "-1",
                    "-map_chapters",
                    "1",
                    "-bitexact",
                    "-flags:a",
                    "+bitexact",
                    "-metadata:s:a:0",
                    "encoder=",
                    "-c:a",
                    if copy_audio { "copy" } else { "flac" },
                ]);
            if !copy_audio {
                add_flac_format(&mut cmd, &draft.tracks[0]);
            }
            if !draft.notes_path.is_empty() {
                cmd.arg("-attach").arg(&draft.notes_path).args([
                    "-metadata:s:t:0",
                    "mimetype=text/plain",
                    "-metadata:s:t:0",
                    "filename=show-notes.txt",
                ]);
            }
            cmd.arg(&partial);
            run(&mut cmd, &temp.path().join("ffmpeg.log"), &cancel, &status)?;
            let output_probe = probe_audio(&partial)?;
            let output_stream = &output_probe["streams"][0];
            let original = &draft.tracks[0];
            if output_stream["sample_rate"].as_str().and_then(|s| s.parse::<u32>().ok()) != Some(original.sample_rate)
                || source_depth(output_stream) != original.bit_depth
                || output_stream["channels"].as_u64() != Some(u64::from(original.channels))
            {
                return Err(
                    "Output sample rate, bit depth or channels differ from the source. The MKA was not saved.".into()
                );
            }
            // A join that drops a file still produces a valid MKA, only a shorter one.
            let length = output_probe["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok());
            if length.is_none_or(|length| (length - elapsed).abs() > 1.0) {
                return Err("The MKA is not as long as its tracks. It was not saved.".into());
            }
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            // persist_noclobber prevents replacing a file created while the job was running.
            let mut target = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            std::io::copy(&mut fs::File::open(&partial).map_err(|e| e.to_string())?, target.as_file_mut())
                .map_err(|e| e.to_string())?;
            target.as_file_mut().flush().map_err(|e| e.to_string())?;
            target.persist_noclobber(&output).map_err(|e| e.to_string())?;
            if let Some(dir) = store_dir {
                let size = fs::metadata(&output).map(|m| m.len() as i64).unwrap_or(0);
                if let Err(e) = crate::store::record_source(&dir, &output.to_string_lossy(), size, &evidence) {
                    status.lock().unwrap_or_else(|e| e.into_inner()).log +=
                        &format!("\nThe MKA was saved, but its source folder could not be recorded for linking: {e}");
                }
            }
            Ok(())
        })
    }
    /// Writes a new folder of tagged files named after the album. Sources stay as they
    /// are; the folder appears only once every file has passed its checks.
    pub fn tag_files(&self, draft: Draft, parent: PathBuf) -> Result<Job, String> {
        validate_tags(&draft)?;
        let folder = parent.join(file_name_part(&draft.album));
        if folder.exists() {
            return Err(format!("{} already exists. Choose another location.", folder.display()));
        }
        if let Some(t) = draft.tracks.iter().find(|t| !Path::new(&t.path).is_file()) {
            return Err(format!("{} is missing.", t.path));
        }
        for t in draft.tracks.iter().filter(|t| tagged_extension(&t.path).is_none()) {
            validate_audio(std::slice::from_ref(t), false)?;
        }
        let output = folder.to_string_lossy().into_owned();
        self.start("Writing tagged files", output, move |cancel, status| {
            let temp =
                tempfile::Builder::new().prefix(".show-player-").tempdir_in(&parent).map_err(|e| e.to_string())?;
            let work = temp.path().join("tagged");
            fs::create_dir(&work).map_err(|e| e.to_string())?;
            for (i, track) in draft.tracks.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let tags = tags_for(&draft, i);
                status.lock().unwrap_or_else(|e| e.into_inner()).phase =
                    format!("Writing track {} of {}: {}", i + 1, draft.tracks.len(), tags[0].1);
                let kept = tagged_extension(&track.path);
                let extension = kept.as_deref().unwrap_or("flac");
                let name = format!("{} {}.{extension}", tags[3].1, file_name_part(&tags[0].1));
                let path = work.join(name);
                let mut cmd = Command::new(tool("ffmpeg"));
                if track.codec != "shorten" {
                    cmd.arg("-noauto_conversion_filters");
                }
                cmd.args(["-xerror", "-hide_banner", "-nostdin", "-n", "-i"])
                    .arg(&track.path)
                    .args(["-map", "0:a:0", "-map_metadata", "-1", "-map_metadata:s:a", "-1", "-map_chapters", "-1"])
                    .args(["-bitexact", "-flags:a", "+bitexact", "-metadata:s:a:0", "encoder="])
                    .args(["-c:a", if kept.is_some() { "copy" } else { "flac" }]);
                if kept.is_none() {
                    add_flac_format(&mut cmd, track);
                }
                if extension == "mp3" {
                    cmd.args(["-id3v2_version", "4", "-write_id3v1", "0", "-write_apetag", "0"]);
                }
                for (key, value) in &tags {
                    cmd.arg("-metadata").arg(format!("{key}={value}"));
                }
                cmd.arg(&path);
                run(&mut cmd, &temp.path().join("ffmpeg.log"), &cancel, &status)?;
                if extension == "flac" {
                    rename_flac_comment(&path)?;
                }
                if audio_hash(&path)? != audio_hash(Path::new(&track.path))? {
                    return Err(format!("{} did not match its source. Nothing was saved.", track.path));
                }
                // ffprobe reports the MP3 Xing header's encoder string, which is not an ID3 frame.
                let names = tag_names(&path)?;
                let xing = |t: &&String| extension == "mp3" && t.as_str() == "ENCODER";
                if let Some(extra) = names.iter().filter(|t| !xing(t)).find(|t| !ALLOWED_TAGS.contains(&t.as_str())) {
                    return Err(format!("{extra} was left on {}. Nothing was saved.", path.display()));
                }
            }
            if let Some(name) = Path::new(&draft.notes_path).file_name() {
                fs::copy(&draft.notes_path, work.join(name)).map_err(|e| e.to_string())?;
            }
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            fs::rename(&work, &folder).map_err(|e| e.to_string())
        })
    }
}
fn run(cmd: &mut Command, log_path: &Path, cancel: &AtomicBool, status: &Mutex<Job>) -> Result<(), String> {
    let log = fs::File::create(log_path).map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null()).stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log);
    let mut child = hidden(cmd).spawn().map_err(|e| format!("Cannot start the bundled tool: {e}"))?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled; originals and download state are retained.".into());
        }
        let text = fs::read(log_path).unwrap_or_default();
        let tail = &text[text.len().saturating_sub(16000)..];
        status.lock().unwrap_or_else(|e| e.into_inner()).log = String::from_utf8_lossy(tail).into();
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            return if exit.success() { Ok(()) } else { Err(format!("Tool exited with {exit}. See the log above.")) };
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_segue_markers_without_removing_song_punctuation() {
        for title in ["Bertha >", "Bertha->", "Bertha -->", "Bertha >> ", "Bertha →", "Bertha =>"] {
            assert_eq!(clean_title(title), "Bertha");
        }
        assert_eq!(clean_title("Scarlet > Fire"), "Scarlet / Fire");
        assert_eq!(clean_title("> He's Gone ->"), "He's Gone");
        assert_eq!(clean_title("Brown-Eyed Women"), "Brown-Eyed Women");
    }
    #[test]
    fn refuses_rate_or_precision_changes() {
        let source = Track {
            sample_rate: 96000,
            bit_depth: 24,
            channels: 2,
            channel_layout: "stereo".into(),
            sample_format: "s32".into(),
            codec: "flac".into(),
            ..Default::default()
        };
        for copy in [true, false] {
            assert!(validate_audio(&[source.clone(), source.clone()], copy).is_ok());
            let mut changed = source.clone();
            changed.sample_rate = 48000;
            assert!(validate_audio(&[source.clone(), changed], copy).is_err());
            let mut changed = source.clone();
            changed.bit_depth = 16;
            assert!(validate_audio(&[source.clone(), changed], copy).is_err());
            let mut changed = source.clone();
            changed.channel_layout = "dual mono".into();
            assert!(validate_audio(&[source.clone(), changed], copy).is_err());
        }
        let floating = Track { codec: "pcm_f32le".into(), sample_format: "flt".into(), bit_depth: 32, ..source };
        assert!(validate_audio(std::slice::from_ref(&floating), false).is_err());
        assert!(validate_audio(&[floating], true).is_ok());
    }
    #[test]
    fn parses_winterland_labels_decorated_sets_and_missing_encore() {
        let (date, venue, tracks) = parse_notes(include_str!("../../../tests/fixtures/gd1977-12-27.24069.txt"));
        assert_eq!((date.as_str(), venue.as_str()), ("1977-12-27", "Winterland Arena"));
        let expected = [
            "Bertha",
            "Good Lovin'",
            "Brown-Eyed Women",
            "Cassidy",
            "Peggy-O",
            "Looks Like Rain",
            "Dire Wolf",
            "Passenger",
            "Candyman",
            "El Paso",
            "Friend of the Devil",
            "The Music Never Stopped",
            "Cold Rain & Snow",
            "Lazy Lightnin'",
            "Supplication",
            "Scarlet Begonias",
            "Fire on the Mountain",
            "Estimated Prophet",
            "He's Gone",
            "Truckin'",
            "Wharf Rat",
            "Around & Around",
        ];
        assert_eq!(tracks.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), expected);
        assert!(tracks[..12].iter().all(|t| t.1 == "Set 1"));
        assert!(tracks[12..].iter().all(|t| t.1 == "Set 2"));
        assert_eq!(missing_song("(missing) - Samson & Delilah").as_deref(), Some("Samson & Delilah"));
    }
    #[test]
    fn reads_embedded_titles_case_insensitively() {
        assert_eq!(
            embedded_title(&serde_json::json!({"format":{"tags":{"TITLE":"Bertha>"}}})).as_deref(),
            Some("Bertha")
        );
        assert_eq!(
            embedded_title(&serde_json::json!({"streams":[{"tags":{"title":"Sugaree"}}]})).as_deref(),
            Some("Sugaree")
        );
        assert!(embedded_title(&serde_json::json!({"format":{"tags":{"title":" "}}})).is_none());
    }
    #[test]
    fn cancels_jobs_and_refuses_overlapping_work() {
        let service = Service::default();
        service
            .start("Test", String::new(), |cancel, _| {
                while !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err("cancelled".into())
            })
            .unwrap();
        assert!(service.start("Overlap", String::new(), |_, _| Ok(())).is_err());
        service.shutdown();
        assert_eq!(service.snapshot().state, "Cancelled");
        assert!(!service.busy());
    }
    #[test]
    fn parses_real_archive_notes_with_roman_sets_and_venue_before_date() {
        let (date, venue, tracks) = parse_notes(include_str!("../../../tests/fixtures/gd1993-09-22.140821.txt"));
        assert_eq!(date, "1993-09-22");
        assert_eq!(venue, "Madison Square Garden, New York , NY");
        assert_eq!(tracks.len(), 20);
        assert_eq!(tracks[8].1, "Set II");
        assert_eq!(tracks[11].0, "Estimated Prophet");
        assert_eq!(tracks[16].0, "Throwing Stones");
        assert_eq!(tracks[18].0, "encore break");
        assert_eq!(tracks[19].0, "I Fought The Law");
        assert_eq!(tracks[19].1, "encore");
    }
    #[test]
    fn parses_unnumbered_sets_and_named_months() {
        let (date, venue, tracks) = parse_notes(
            "Grateful Dead\nSeptember 22, 1993\nMadison Square Garden\nSet 1:\nHelp On the Way >\nSlipknot >\nSet 2:\nDark Star\nE: I Fought the Law\nSource 1: SBD\nEquipment details",
        );
        assert_eq!(date, "1993-09-22");
        assert_eq!(venue, "Madison Square Garden");
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[3].1, "Encore");
    }
    #[test]
    fn parses_sets_and_disc_track_notation() {
        let (date, venue, tracks) = parse_notes(
            "Grateful Dead\n1977-05-08\nBarton Hall\nSet 1:\n01. Minglewood Blues\n02 Loser\nSet 2\nd2t01 Scarlet Begonias >\nd2t02 Fire on the Mountain\nSource: SBD\n",
        );
        assert_eq!((date.as_str(), venue.as_str()), ("1977-05-08", "Barton Hall"));
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[2], ("Scarlet Begonias".into(), "Set 2".into()));
    }
    #[test]
    fn parses_old_dates_and_ignores_hashes() {
        let (date, _, tracks) =
            parse_notes("Jerry Garcia Band\n12/31/75\nKeystone\n01 - Sugaree\n02 0123456789abcdef0123456789abcdef\n");
        assert_eq!(date, "1975-12-31");
        assert_eq!(tracks.len(), 1);
    }
    #[test]
    fn keeps_only_the_song_title() {
        for raw in [
            "Bertha",
            "01. Bertha",
            "01 01 Bertha",
            "01. 01 Bertha",
            "01. 01. Bertha",
            "1 - 01 Bertha",
            "101-d1t01 - Bertha ->",
            "101. 01 Bertha",
            "d1t01. 01. Bertha",
            "d1t01 - 01 - Bertha",
            "gd77-12-27d1t01 Bertha",
            "s1t01 Bertha",
            "Track 01: Bertha",
            "t01 Bertha >",
            "Bertha (1)",
            "Bertha (1) (2) ->",
            "Bertha > (1)",
            "Bertha *",
            "Bertha **#",
            "Bertha [7:32]",
            "Bertha 7:32",
            "01. Bertha (7:32) >",
            "Bertha (cut)",
            "Bertha [incomplete]",
            "Bertha (w/ Branford Marsalis)",
            "Bertha (with David Murray) ->",
            "~~Bertha~~",
            "E: Bertha",
            "Encore - Bertha",
        ] {
            assert_eq!(song_title(raw), "Bertha", "{raw}");
        }
        for title in [
            "Mannish Boy (I'm A Man)",
            "(I'm A) Road Runner",
            "Riot In Cell Block #9",
            "Rainy Day Women #12 and 35",
            "How Sweet It Is (To Be Loved By You)",
            "Weather Report Suite Part 1",
            "Peggy-O",
            "Slipknot!",
            "U.S. Blues",
            "Goin' Down The Road Feeling Bad",
        ] {
            assert_eq!(song_title(title), title);
        }
        assert_eq!(song_title("03 500 Miles"), "500 Miles");
        assert_eq!(song_title("China Cat Sunflower > I Know You Rider"), "China Cat Sunflower / I Know You Rider");
    }
    #[test]
    fn names_songs_from_tags_and_files_by_the_same_rule() {
        assert_eq!(title_from_file("101. Bertha_"), "Bertha");
        assert_eq!(title_from_file("204. Scarlet Begonias -_"), "Scarlet Begonias");
        assert_eq!(title_from_file("207. He_s Gone"), "He's Gone");
        assert_eq!(title_from_file("gd77-12-27d1t01"), "");
        assert_eq!(
            embedded_title(&serde_json::json!({"format":{"tags":{"TITLE":"01 01 Sugaree ->"}}})).as_deref(),
            Some("Sugaree")
        );
    }
    #[test]
    fn ignores_tool_output_and_notes_around_the_setlist() {
        let notes = "Grateful Dead\n1977-05-08\nBarton Hall\n16 bit\n2 discs\n\
            ; shntool md5 fingerprint file\ndc900cceb82220fefbd01cc87b1a9370  [shntool]  gd77-05-08d1t01.shn\n\
            Disc 1\nSet 1:\n1. Minglewood Blues\n2. 02 Loser\nDisc 2 - Set 2\n3. Scarlet Begonias ->\n\
            Notes:\n1) Jerry's guitar drops out\n2) Taped by a friend\n";
        let (_, _, tracks) = parse_notes(notes);
        let titles: Vec<_> = tracks.iter().map(|t| t.0.as_str()).collect();
        assert_eq!(titles, ["Minglewood Blues", "Loser", "Scarlet Begonias"]);
        assert_eq!(tracks[2].1, "Set 2");
    }
    #[test]
    fn renames_the_flac_comment_at_the_same_length() {
        let entry = |text: &str| [&(text.len() as u32).to_le_bytes()[..], text.as_bytes()].concat();
        let comments = [
            &6u32.to_le_bytes()[..],
            b"ffmpeg",
            &2u32.to_le_bytes()[..],
            &entry("title=Bertha"),
            &entry("DESCRIPTION=Equipment: SBD"),
        ]
        .concat();
        let header = |kind: u8, size: usize| [&[kind][..], &(size as u32).to_be_bytes()[1..]].concat();
        let mut flac = b"fLaC".to_vec();
        flac.extend(header(0, 34));
        flac.extend([0; 34]);
        flac.extend(header(4, comments.len()));
        flac.extend(&comments);
        flac.extend(header(0x81, 8));
        flac.extend([0; 8]);
        flac.extend(b"frames");
        let renamed = flac_comment_renamed(&flac).unwrap();
        assert_eq!(renamed.len(), flac.len());
        assert!(renamed.ends_with(b"frames"));
        let text = String::from_utf8_lossy(&renamed);
        assert!(text.contains("title=Bertha") && text.contains("COMMENT=Equipment: SBD"));
        assert!(!text.contains("DESCRIPTION"));
        // The tag block is four bytes shorter and the padding four longer.
        assert_eq!(renamed[4 + 4 + 34 + 3], flac[4 + 4 + 34 + 3] - 4);
        let padding = 4 + 4 + 34 + 4 + comments.len() - 4;
        assert_eq!((renamed[padding], renamed[padding + 3]), (0x81, 12));
        assert!(flac_comment_renamed(b"not flac").is_none());
    }
    #[test]
    fn finds_the_setlist_after_numbered_notes_in_a_matrix_txt() {
        let (date, _, tracks) = parse_notes(include_str!("../../../tests/fixtures/gd77-10-30.mtx.dusborne.flac16.txt"));
        assert_eq!(date, "1977-10-30");
        assert_eq!(tracks.len(), 21);
        assert_eq!(tracks[0], ("Tuning".into(), "Set 1".into()));
        assert_eq!(tracks[3].0, "Mama Tried");
        assert_eq!(tracks[19], ("Playing in the Band".into(), "Set 2".into()));
        assert_eq!(tracks[20], ("Johnny B. Goode".into(), "Encore".into()));
    }
    #[test]
    fn ends_the_equipment_at_its_first_sentence() {
        assert_eq!(
            recording_comment("Source: Reel > DAT (DAT-1).Original DATs circulated in 1996. Thanks to all\n", &[]),
            "Equipment: Reel > DAT (DAT-1)"
        );
        assert_eq!(
            recording_comment("Source: Nak 300 > Sony D-5 v.2 > DAT\n", &[]),
            "Equipment: Nak 300 > Sony D-5 v.2 > DAT"
        );
    }
    #[test]
    fn trusts_the_folder_name_over_a_passing_mention_in_the_notes() {
        let profile = crate::catalogue::classify(
            "gd1977-10-01.140578.sbd.betty.patched.smith.miller.clugston.flac16",
            "7 inch reel. Audience patch for one song.",
            "",
            "",
            "",
            "",
        );
        assert_eq!(profile.kind, "SBD");
        assert_eq!(crate::catalogue::known_tapers(&profile), ["Betty Cantor-Jackson"]);
    }
    #[test]
    fn shows_paths_in_their_ordinary_form() {
        assert_eq!(plain_path(Path::new(r"\\?\D:\music\show")), PathBuf::from(r"D:\music\show"));
        assert_eq!(plain_path(Path::new(r"\\?\UNC\nas\music")), PathBuf::from(r"\\nas\music"));
        assert_eq!(plain_path(Path::new(r"D:\music")), PathBuf::from(r"D:\music"));
    }
    #[test]
    fn numbers_tracks_as_one_padded_sequence() {
        assert_eq!(track_number(0, 9), "1");
        assert_eq!(track_number(0, 21), "01");
        assert_eq!(track_number(20, 21), "21");
        assert_eq!(track_number(6, 104), "007");
    }
    #[test]
    fn describes_a_show_as_date_region_city_venue() {
        let place = Place { venue: "Civic Center Arena".into(), city: "Springfield".into(), region: "MA".into() };
        assert_eq!(album_name("1977-04-23", &place), "770423_ma-springfield-civic-center-arena");
        let place = Place { venue: "O'Keefe Centre".into(), city: "St. Louis".into(), region: "Canada".into() };
        assert_eq!(album_name("1967-07-31", &place), "670731_canada-st-louis-okeefe-centre");
        assert_eq!(place_from_venue("Civic Center Arena - Springfield, MA").city, "Springfield");
        assert_eq!(place_from_venue("Madison Square Garden, New York , NY").region, "NY");
        assert_eq!(place_from_venue("Winterland Arena").venue, "Winterland Arena");
    }
    #[test]
    fn takes_dates_from_the_folder_and_keeps_to_the_garcia_years() {
        assert_eq!(date_from_name("gd1977-12-27.24069").as_deref(), Some("1977-12-27"));
        assert_eq!(date_from_name("gd77-12-27.sbd.ashley").as_deref(), Some("1977-12-27"));
        assert_eq!(date_from_name("770423-ma-springfield").as_deref(), Some("1977-04-23"));
        assert_eq!(date_from_name("Winterland"), None);
        assert!(in_era("1995-07-09"));
        assert!(in_era("1995-08-09"));
        assert!(!in_era("1995-08-10"));
        assert!(!in_era("2015-07-05"));
        assert!(!in_era("07/09/1995"));
    }
    #[test]
    fn writes_taper_and_equipment_for_the_comment() {
        assert_eq!(
            recording_comment(include_str!("../../../tests/fixtures/gd1993-09-22.140821.txt"), &[]),
            "Equipment: (FOB) Schoeps MK41 > Panasonic SV-255 > dat(48k)"
        );
        assert_eq!(
            recording_comment(include_str!("../../../tests/fixtures/gd1977-12-27.24069.txt"), &[]),
            "Equipment: SBD>>MR>>SS>>DAT>>CDA>>WAV(EAC)>>DAW(Bertha)>>CDA/SHN - Sound A"
        );
        assert_eq!(
            recording_comment("Recording Info:\nSBD -> Master Reel -> Dat -> CD\nNotes:\n", &[]),
            "Equipment: SBD -> Master Reel -> Dat -> CD"
        );
        assert_eq!(
            recording_comment("Taper: Bob Wagner\nSource: AKG 451 > Sony D5\n", &[]),
            "Taper: Bob Wagner; Equipment: AKG 451 > Sony D5"
        );
        assert_eq!(
            recording_comment("Source: SBD > Reel\n", &["Dan Healy".into()]),
            "Taper: Dan Healy; Equipment: SBD > Reel"
        );
    }
    #[test]
    fn refuses_output_without_valid_tags() {
        let track = Track { title: "Bertha".into(), ..Default::default() };
        let mut draft =
            Draft { date: "1977-12-27".into(), album: "771227_ca".into(), tracks: vec![track], ..Default::default() };
        assert!(validate_tags(&draft).is_ok());
        draft.album = "770101_ca".into();
        assert!(validate_tags(&draft).is_err());
        draft.date = "2015-07-05".into();
        draft.album = "150705_il".into();
        assert!(validate_tags(&draft).is_err());
    }
    #[test]
    fn natural_order_is_numeric() {
        assert!(natural_key("track2.flac") < natural_key("track10.flac"));
    }
    #[test]
    fn rejects_bad_jobs_before_starting_tools() {
        let service = Service::default();
        assert!(service.package(Draft::default(), PathBuf::from("x.mka"), true).is_err());
    }
}
