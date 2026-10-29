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
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Draft {
    pub folder: String,
    pub artist: String,
    pub date: String,
    pub venue: String,
    pub notes_path: String,
    pub notes: String,
    pub warnings: Vec<String>,
    pub tracks: Vec<Track>,
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
fn get(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(35))).build().new_agent();
    agent
        .get(url)
        .header("User-Agent", "JupiterShowPlayer/0.1")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())
}
fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(35))).build().new_agent();
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}
fn rx(pattern: &str) -> Regex {
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
pub fn parse_notes(notes: &str) -> (String, String, String, Vec<(String, String)>) {
    let preamble = rx(r"(?i)^(?:this is (?:a |an )?(?:flac|shn|lossless)|shnid\s*:|encoding notes\s*:)");
    let lines: Vec<_> = notes.lines().map(str::trim).filter(|l| !l.is_empty() && !preamble.is_match(l)).collect();
    let artist = lines.first().copied().unwrap_or_default().to_string();
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
    let numbered = rx(r"(?i)^(?:(?:d\d+\s*t|t)\s*)?\d{1,3}(?:\s*[.):-]\s*|\s+)(.+)$");
    let explicit = rx(r"(?i)^d\d+t\d+\s*[-.:)]?\s*(.+)$");
    let compound = rx(r"(?i)^\d{1,3}\s*[-.:)]\s*d\d+t\d+\s*[-.:)]\s*(.+)$");
    let mut set = String::new();
    let mut set_numbered = false;
    let mut tracks = Vec::new();
    for line in lines {
        if missing_song(line).is_some() {
            continue;
        }
        if rx(r"(?i)^(?:shntool output|flac fingerprints|md5 checksums|ffp checksums)\s*:").is_match(line) {
            break;
        }
        if rx(r"^\d{1,3}:\d{2}(?:[.:]\d+)?\s").is_match(line) {
            continue;
        }
        let heading = line.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '=' | ':'));
        if rx(r"(?i)^(?:set\s*(?:one|two|three|i{1,3}|[123])|(?:first|second|third)\s+set|encore)$").is_match(heading) {
            set = heading.to_string();
            set_numbered = false;
            continue;
        }
        if rx(r"(?i)^(?:source(?:\s+\d+)?|lineage|notes|recording|transfer|checksums)\s*:").is_match(line) {
            set.clear();
            set_numbered = true;
            continue;
        }
        if let Some(song) = line.strip_prefix("E:").or_else(|| line.strip_prefix("Encore:")) {
            tracks.push((song.trim().into(), "Encore".into()));
            set.clear();
            continue;
        }
        if let Some(c) = compound.captures(line).or_else(|| explicit.captures(line)).or_else(|| numbered.captures(line))
        {
            set_numbered = true;
            let title = c[1].trim();
            // Dates, hashes and bare timings are not song titles.
            if title.chars().any(char::is_alphabetic) && !rx(r"^[0-9a-fA-F]{32,64}(?:\s|$)").is_match(title) {
                tracks.push((title.to_string(), set.clone()));
            }
        } else if !set.is_empty() && !set_numbered && !line.starts_with(['*', '#', '-', '_', '=']) {
            tracks.push((line.into(), set.clone()));
        }
    }
    (artist, date, venue, tracks.into_iter().map(|(title, set)| (clean_title(&title), set)).collect())
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
    .map(clean_title)
}
pub fn inspect(folder: &Path, notes_override: &str) -> Result<Draft, String> {
    let folder = fs::canonicalize(folder).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    collect(&folder, 0, &mut files)?;
    files.sort_by_key(|p| natural_key(&p.to_string_lossy()));
    let mut notes: Vec<_> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")))
        .filter(|p| !p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains("ffp"))
        .filter(|p| !p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains("md5"))
        .collect();
    notes.sort_by_key(|p| std::cmp::Reverse(p.metadata().map(|m| m.len()).unwrap_or(0)));
    let note_path = if !notes_override.is_empty() {
        PathBuf::from(notes_override)
    } else {
        notes.first().map(|p| (*p).clone()).unwrap_or_default()
    };
    let bytes = if note_path.is_file() { fs::read(&note_path).map_err(|e| e.to_string())? } else { Vec::new() };
    let text = String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string();
    let (artist, date, venue, titles) = parse_notes(&text);
    let audio: Vec<_> = files.iter().filter(|p| is_audio(p)).cloned().collect();
    if audio.is_empty() {
        return Err("No supported audio files were found in this show folder.".into());
    }
    let mut draft = Draft {
        folder: folder.to_string_lossy().into(),
        artist,
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
        let duration = v["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
            .ok_or_else(|| format!("Unknown duration for {}", path.display()))?;
        let title = clean_title(&if titles.len() == audio.len() {
            titles[i].0.clone()
        } else {
            embedded_title(&v).unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into())
        });
        let set = if titles.len() == audio.len() { titles[i].1.clone() } else { String::new() };
        draft.tracks.push(Track {
            path: path.to_string_lossy().into(),
            title,
            set,
            duration,
            sample_rate: s["sample_rate"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
            channels: s["channels"].as_u64().unwrap_or(0) as u32,
            codec: s["codec_name"].as_str().unwrap_or_default().into(),
            bit_depth: source_depth(s),
            sample_format: s["sample_fmt"].as_str().unwrap_or_default().into(),
            channel_layout: s["channel_layout"].as_str().unwrap_or_default().into(),
        });
    }
    if let Err(reason) = validate_audio(&draft.tracks, false) {
        draft.warnings.push(reason);
    }
    Ok(draft)
}
pub fn catalogue(data: &Path, query: &str, source: &str) -> Result<Vec<Recording>, String> {
    let tokens: Vec<_> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let short_date = rx(r"(?i)gd(\d{2})-(\d{2})-(\d{2})");
    let matches = |s: &str| {
        let expanded =
            short_date.replace_all(s, |c: &regex::Captures<'_>| format!("gd19{}-{}-{}", &c[1], &c[2], &c[3]));
        let s = format!("{s} {expanded}").to_lowercase();
        tokens.iter().all(|t| s.contains(t))
    };
    let mut rows = Vec::new();
    if source != "Archive.org" {
        let items: Vec<Value> =
            serde_json::from_slice(&fs::read(data.join("gd_torrents.json")).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        for r in items {
            let title = r["title"].as_str().unwrap_or_default();
            if matches(title) {
                let id = r["id"].as_u64().unwrap_or(0);
                rows.push(Recording {
                    title: title.into(),
                    source: "etree".into(),
                    url: format!("https://bt.etree.org/details.php?torrentId={id}"),
                    key: format!("etree-{id}"),
                    detail: format!("Saved listing; {} seeds at capture (not live)", r["seeds"].as_u64().unwrap_or(0)),
                    ..Default::default()
                });
            }
        }
    }
    if source != "etree" {
        let urls = fs::read_to_string(data.join("grateful_dead_torrent_urls.txt")).map_err(|e| e.to_string())?;
        let mut seen = std::collections::HashSet::new();
        for line in urls.lines() {
            if let Some(id) = line.split("/items/").nth(1).and_then(|s| s.split('/').next())
                && matches(id)
                && seen.insert(id.to_string())
            {
                rows.push(Recording {
                    title: id.into(),
                    source: "Archive.org".into(),
                    url: format!("https://archive.org/details/{id}"),
                    key: id.into(),
                    detail: "Saved listing; download availability will be checked".into(),
                    ..Default::default()
                });
            }
        }
    }
    rows.truncate(500);
    Ok(rows)
}
pub fn torrent_url(recording: &Recording) -> Result<(String, Vec<String>), String> {
    if recording.source == "Archive.org" {
        if !rx(r"^[A-Za-z0-9_.-]+$").is_match(&recording.key) {
            return Err("Invalid Archive identifier.".into());
        }
        let v: Value = serde_json::from_str(&get(&format!("https://archive.org/metadata/{}", recording.key))?)
            .map_err(|e| e.to_string())?;
        let metadata = &v["metadata"];
        if metadata["collection"].to_string().contains("stream_only")
            || metadata["access-restricted-item"].as_str() == Some("true")
            || metadata["no-download"].as_str() == Some("true")
        {
            return Err("Archive.org marks this recording as restricted or stream-only.".into());
        }
        let files = v["files"].as_array().ok_or("Archive returned no files.")?;
        let downloadable = files.iter().any(|f| {
            f["source"].as_str() == Some("original")
                && f["private"].as_str() != Some("true")
                && f["private"].as_bool() != Some(true)
                && f["name"].as_str().is_some_and(|n| is_audio(Path::new(n)))
        });
        if !downloadable {
            return Err("No downloadable original audio is exposed for this recording.".into());
        }
        let name = files
            .iter()
            .find(|f| f["format"].as_str() == Some("Archive BitTorrent"))
            .and_then(|f| f["name"].as_str())
            .ok_or("No current Archive torrent is available.")?;
        if name.contains('/') || name.contains('\\') || name.contains(' ') {
            return Err("Unexpected torrent filename.".into());
        }
        let wanted = files
            .iter()
            .filter(|f| {
                f["source"].as_str() == Some("original")
                    && f["private"].as_str() != Some("true")
                    && f["private"].as_bool() != Some(true)
            })
            .filter_map(|f| f["name"].as_str())
            .map(str::to_owned)
            .collect();
        Ok((format!("https://archive.org/download/{}/{name}", recording.key), wanted))
    } else if recording.source == "etree" {
        let id = recording
            .key
            .strip_prefix("etree-")
            .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
            .ok_or("Invalid etree ID.")?;
        let html = get(&format!("https://bt.etree.org/details.php?torrentId={id}"))?;
        let links = rx(r#"(?i)href\s*=\s*["']([^"']+)["']"#);
        for c in links.captures_iter(&html) {
            let href = c[1].replace("&amp;", "&");
            if href.contains("download.php") || href.ends_with(".torrent") {
                if href.starts_with("https://bt.etree.org/") {
                    return Ok((href, Vec::new()));
                }
                if !href.contains("://") && !href.starts_with("//") {
                    return Ok((format!("https://bt.etree.org/{}", href.trim_start_matches('/')), Vec::new()));
                }
            }
        }
        Err("No torrent link found on the etree detail page. The saved listing may be stale.".into())
    } else {
        Err("Unknown recording source.".into())
    }
}
fn select_indices(listing: &str, wanted: &[String]) -> Vec<String> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let line = rx(r"(?m)^\s*(\d+)\|(.+)\r?$");
    line.captures_iter(listing)
        .filter(|c| {
            let path = c[2].trim().replace('\\', "/");
            wanted.iter().any(|w| path == *w || path.ends_with(&format!("/{w}")))
        })
        .map(|c| c[1].to_owned())
        .collect()
}
impl Service {
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
    pub fn download(&self, r: Recording, destination: PathBuf, seed_minutes: u32) -> Result<Job, String> {
        if seed_minutes > 1440 {
            return Err("Seeding time must be at most 1440 minutes.".into());
        }
        if !rx(r"^[A-Za-z0-9_.-]+$").is_match(&r.key) {
            return Err("Invalid recording identifier.".into());
        }
        let dest = destination.join(&r.key);
        let output = dest.to_string_lossy().to_string();
        self.start("Checking recording availability", output, move |cancel, status| {
            let (url, wanted) = torrent_url(&r)?;
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            let torrent = dest.join("recording.torrent");
            fs::write(&torrent, get_bytes(&url)?).map_err(|e| e.to_string())?;
            let listing = hidden(&mut Command::new(tool("aria2c")))
                .args(["--no-conf=true", "--show-files=true"])
                .arg(&torrent)
                .output()
                .map_err(|e| e.to_string())?;
            if !listing.status.success() {
                return Err("The source did not return a readable torrent.".into());
            }
            let listing = String::from_utf8_lossy(&listing.stdout);
            let indices = select_indices(&listing, &wanted);
            if !wanted.is_empty() && indices.is_empty() {
                return Err(
                    "The current torrent does not contain the advertised original files. Try refreshing later.".into(),
                );
            }
            status.lock().unwrap_or_else(|e| e.into_inner()).phase = "Downloading; seeding follows if requested".into();
            let log = dest.join("show-player-download.log");
            let mut cmd = Command::new(tool("aria2c"));
            cmd.arg(format!("--dht-file-path={}", dest.join(".show-player-dht.dat").display()))
                .arg(format!("--dht-file-path6={}", dest.join(".show-player-dht6.dat").display()))
                .arg("--enable-color=false");
            if !indices.is_empty() {
                cmd.arg(format!("--select-file={}", indices.join(",")));
            }
            cmd.args([
                "--no-conf=true",
                "--check-integrity=true",
                "--auto-file-renaming=false",
                "--allow-overwrite=false",
                "--auto-save-interval=1",
                "--summary-interval=1",
                "--console-log-level=notice",
                "--follow-torrent=true",
                "--bt-save-metadata=true",
                "--max-upload-limit=256K",
                "--max-connection-per-server=2",
                "--bt-stop-timeout=120",
            ])
            .arg(format!("--seed-time={seed_minutes}"))
            .arg(format!("--dir={}", dest.display()))
            .arg(torrent);
            run(&mut cmd, &log, &cancel, &status)
        })
    }
    pub fn package(&self, draft: Draft, output: PathBuf, copy_audio: bool) -> Result<Job, String> {
        if output.extension().is_none_or(|x| !x.eq_ignore_ascii_case("mka")) {
            return Err("Output must end in .mka.".into());
        }
        if output.exists() {
            return Err("The output already exists. Choose another name.".into());
        }
        if draft.tracks.is_empty() {
            return Err("No tracks to package.".into());
        }
        validate_audio(&draft.tracks, copy_audio)?;
        for t in &draft.tracks {
            if !t.duration.is_finite() || t.duration <= 0.0 || !Path::new(&t.path).is_file() {
                return Err("A track is missing or has an invalid duration.".into());
            }
        }
        let output_string = output.to_string_lossy().into_owned();
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
            meta += &format!(
                "title={}\nartist={}\ndate={}\ncomment={}\n",
                escape(&format!("{} — {} — {}", draft.artist, draft.date, draft.venue)),
                escape(&draft.artist),
                escape(&draft.date),
                escape(&draft.notes)
            );
            let mut elapsed = 0.0;
            for t in &draft.tracks {
                if t.path.contains(['\n', '\r']) {
                    return Err("Track paths cannot contain line breaks.".into());
                }
                let p = t.path.trim_start_matches(r"\\?\").replace('\\', "/").replace('\'', "'\\''");
                concat += &format!("file '{p}'\nduration {:.9}\n", t.duration);
                let start = (elapsed * 1000.0_f64).round() as u64;
                elapsed += t.duration;
                let song = clean_title(&t.title);
                let title = if t.set.is_empty() { song } else { format!("{} / {}", t.set, song) };
                meta += &format!(
                    "[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={}\ntitle={}\n",
                    (elapsed * 1000.0).round() as u64,
                    escape(&title)
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
                    "-map_chapters",
                    "1",
                    "-c:a",
                    if copy_audio { "copy" } else { "flac" },
                ]);
            if !copy_audio {
                let source = &draft.tracks[0];
                cmd.args(["-sample_fmt", if source.bit_depth == 16 { "s16" } else { "s32" }]).args([
                    "-ar",
                    &source.sample_rate.to_string(),
                    "-bits_per_raw_sample",
                    &source.bit_depth.to_string(),
                ]);
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
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            // persist_noclobber prevents replacing a file created while the job was running.
            let mut target = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            std::io::copy(&mut fs::File::open(&partial).map_err(|e| e.to_string())?, target.as_file_mut())
                .map_err(|e| e.to_string())?;
            target.as_file_mut().flush().map_err(|e| e.to_string())?;
            target.persist_noclobber(&output).map_err(|e| e.to_string())?;
            Ok(())
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
        let (artist, date, venue, tracks) = parse_notes(include_str!("../../../tests/fixtures/gd1977-12-27.24069.txt"));
        assert_eq!(
            (artist.as_str(), date.as_str(), venue.as_str()),
            ("Grateful Dead", "1977-12-27", "Winterland Arena")
        );
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
        let (artist, date, venue, tracks) =
            parse_notes(include_str!("../../../tests/fixtures/gd1993-09-22.140821.txt"));
        assert_eq!(artist, "Grateful Dead");
        assert_eq!(date, "1993-09-22");
        assert_eq!(venue, "Madison Square Garden, New York , NY");
        assert_eq!(tracks.len(), 20);
        assert_eq!(tracks[8].1, "Set II");
        assert_eq!(tracks[19].0, "I Fought The Law");
        assert_eq!(tracks[19].1, "encore");
    }
    #[test]
    fn selects_original_files_from_torrent_listing() {
        let wanted = vec!["song.flac".into(), "show.txt".into()];
        assert_eq!(
            select_indices("  1|./show/song.flac\n  2|./show/song.mp3\n  3|./show/show.txt\n", &wanted),
            ["1", "3"]
        );
    }
    #[test]
    fn parses_unnumbered_sets_and_named_months() {
        let (_, date, venue, tracks) = parse_notes(
            "Grateful Dead\nSeptember 22, 1993\nMadison Square Garden\nSet 1:\nHelp On the Way >\nSlipknot >\nSet 2:\nDark Star\nE: I Fought the Law\nSource 1: SBD\nEquipment details",
        );
        assert_eq!(date, "1993-09-22");
        assert_eq!(venue, "Madison Square Garden");
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[3].1, "Encore");
    }
    #[test]
    fn parses_sets_and_disc_track_notation() {
        let (artist, date, venue, tracks) = parse_notes(
            "Grateful Dead\n1977-05-08\nBarton Hall\nSet 1:\n01. Minglewood Blues\n02 Loser\nSet 2\nd2t01 Scarlet Begonias >\nd2t02 Fire on the Mountain\nSource: SBD\n",
        );
        assert_eq!((artist.as_str(), date.as_str(), venue.as_str()), ("Grateful Dead", "1977-05-08", "Barton Hall"));
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[2], ("Scarlet Begonias".into(), "Set 2".into()));
    }
    #[test]
    fn parses_old_dates_and_ignores_hashes() {
        let (_, date, _, tracks) =
            parse_notes("Jerry Garcia Band\n12/31/75\nKeystone\n01 - Sugaree\n02 0123456789abcdef0123456789abcdef\n");
        assert_eq!(date, "1975-12-31");
        assert_eq!(tracks.len(), 1);
    }
    #[test]
    fn natural_order_is_numeric() {
        assert!(natural_key("track2.flac") < natural_key("track10.flac"));
    }
    #[test]
    fn rejects_bad_jobs_before_starting_tools() {
        let service = Service::default();
        assert!(service.package(Draft::default(), PathBuf::from("x.mka"), true).is_err());
        assert!(
            service
                .download(Recording { key: "../escape".into(), ..Default::default() }, PathBuf::from("x"), 0)
                .is_err()
        );
    }
}
