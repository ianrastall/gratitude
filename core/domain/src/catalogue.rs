//! Attributed, offline show facts and personal recording priorities.
use crate::shows::Recording;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Priorities {
    pub sources: Vec<String>,
    pub people: Vec<String>,
    pub modifiers: Vec<String>,
    pub people_first: bool,
}
impl Default for Priorities {
    fn default() -> Self {
        Self {
            sources: ["SBD", "MTX", "AUD", "FM", "UNKNOWN"].map(str::to_owned).into(),
            people: vec!["Betty".into(), "Miller".into()],
            modifiers: Vec::new(),
            people_first: false,
        }
    }
}
impl Priorities {
    pub fn validate(mut self) -> Result<Self, String> {
        if self.sources.is_empty() {
            self.sources = Self::default().sources;
        }
        fn unique(items: Vec<String>, limit: usize) -> Result<Vec<String>, String> {
            let mut out = Vec::<String>::new();
            for s in items {
                let s = s.trim();
                if s.is_empty() {
                    continue;
                }
                if s.len() > 120 || out.len() >= limit {
                    return Err(format!("Use at most {limit} short priority entries."));
                }
                if out.iter().any(|x| x.eq_ignore_ascii_case(s)) {
                    return Err(format!("Duplicate priority: {s}"));
                }
                out.push(s.into());
            }
            Ok(out)
        }
        self.sources = unique(self.sources, 5)?.into_iter().map(|s| s.to_uppercase()).collect();
        for s in &self.sources {
            if !["SBD", "MTX", "AUD", "FM", "UNKNOWN"].contains(&s.as_str()) {
                return Err(format!("Unknown source type {s}. Use SBD, MTX, AUD, FM or UNKNOWN."));
            }
        }
        for s in Self::default().sources {
            if !self.sources.contains(&s) {
                self.sources.push(s);
            }
        }
        self.people = unique(self.people, 24)?;
        self.modifiers = unique(self.modifiers, 12)?.into_iter().map(|s| s.to_uppercase()).collect();
        for s in &self.modifiers {
            if !["FOB", "FM", "PRE-FM", "DSBD", "ULTRAMATRIX", "DTS"].contains(&s.as_str()) {
                return Err(format!("Unknown modifier {s}. Use FOB, FM, PRE-FM, DSBD, ULTRAMATRIX or DTS."));
            }
        }
        Ok(self)
    }
    pub fn load(directory: &Path) -> Self {
        fs::read(directory.join("recording-priorities.json"))
            .ok()
            .and_then(|s| serde_json::from_slice::<Self>(&s).ok())
            .and_then(|s| s.validate().ok())
            .unwrap_or_default()
    }
    pub fn save(self, directory: &Path) -> Result<Self, String> {
        use std::io::Write;
        let validated = self.validate()?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
        temp.write_all(&serde_json::to_vec_pretty(&validated).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(directory.join("recording-priorities.json")).map_err(|e| e.to_string())?;
        Ok(validated)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub kind: String,
    pub modifiers: Vec<String>,
    pub people: Vec<String>,
    pub credits: String,
    pub evidence: String,
    pub inferred: bool,
}
fn re(p: &str) -> Regex {
    static PATTERNS: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    PATTERNS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(p.to_owned())
        .or_insert_with(|| Regex::new(p).expect("static regex"))
        .clone()
}
fn has(text: &str, pattern: &str) -> bool {
    re(pattern).is_match(text)
}
fn kind(text: &str) -> Option<&'static str> {
    let normalized = text.replace('_', " ");
    let text = normalized.as_str();
    if has(text, r"(?i)\b(?:mtx|matrix|ultra[- ]?matrix|ultramtx)\b|\b(?:sbd|soundboard)\s*\+\s*(?:aud|audience)\b") {
        Some("MTX")
    } else if has(text, r"(?i)\b(?:sbd|dsbd|soundboard|sound board)\b") {
        Some("SBD")
    } else if has(text, r"(?i)\b(?:aud|audience|fob)\b") {
        Some("AUD")
    } else if has(text, r"(?i)\b(?:fm|pre[- ]?fm|broadcast)\b") {
        Some("FM")
    } else {
        None
    }
}
fn clean(text: &str) -> String {
    re(r"[\w.+-]+@[\w.-]+\.[A-Za-z]+").replace_all(text, "").split_whitespace().collect::<Vec<_>>().join(" ")
}
const ALIASES: &[(&str, &str)] = &[
    (
        "Betty Cantor-Jackson",
        r"(?i)\b(?:betty[- ._]+cantor(?:[- ._]+jackson)?|cantor[- ]jackson|betty[- ._]+boards)\b|[._-]betty(?:[._-]|$)|^betty$",
    ),
    ("Charlie Miller", r"(?i)\bcharlie[- ._]*miller\b|[._-]miller(?:[._-]|$)|^miller$"),
    ("Dan Healy", r"(?i)\b(?:dan[- ._]*healy|healy)\b"),
    ("Don Pearson", r"(?i)\b(?:don[- ._]*pearson|pearson)\b"),
    ("Hunter Seamons", r"(?i)\b(?:hunter[- ._]*seamons|seamons|hunter.?s trix)\b"),
    ("Dave Usborne", r"(?i)\b(?:dave[- ._]*usborne|dusborne|usborne)\b"),
    ("Rob Eaton", r"(?i)\b(?:rob[- ._]*eaton|eaton)\b"),
    ("Rango Keshavan", r"(?i)\b(?:rango[- ._]*keshavan|keshavan)\b"),
    ("GEMS", r"(?i)\bgems\b"),
];
pub fn classify(source: &str, identifier: &str, taper: &str, transfer: &str, lineage: &str, hints: &str) -> Profile {
    let hint = re(r"(?i)^\s*(?:(?:better|cool|great|main)\s+)*(SBD|AUD|matrix|ultramatrix|FM)\b")
        .captures(hints.trim())
        .and_then(|c| kind(&c[1]));
    let explicit = kind(source).or(hint);
    let primary = explicit.or_else(|| kind(identifier)).unwrap_or("UNKNOWN");
    let all = format!("{source} {identifier} {taper} {transfer} {lineage} {hints}");
    let mut modifiers = Vec::new();
    for (label, pattern) in [
        ("FOB", r"(?i)\bfob\b"),
        ("PRE-FM", r"(?i)\bpre[- ]?fm\b"),
        ("DSBD", r"(?i)\bdsbd\b"),
        ("ULTRAMATRIX", r"(?i)\bultra[- ]?matrix\b"),
        ("DTS", r"(?i)\bdts\b"),
    ] {
        if [source, identifier, taper, transfer, lineage, hints].iter().any(|text| has(text.trim(), pattern)) {
            modifiers.push(label.into());
        }
    }
    if has(&all, r"(?i)\bfm\b") && !modifiers.iter().any(|s| s == "PRE-FM") {
        modifiers.push("FM".into());
    }
    let mut people = Vec::new();
    let mut credits = Vec::new();
    for (name, pattern) in ALIASES {
        if [source, identifier, taper, transfer, lineage, hints].iter().any(|text| has(text.trim(), pattern)) {
            people.push((*name).into());
            let role = if has(taper, pattern) {
                "recording"
            } else if has(transfer, pattern) {
                "transfer/mastering"
            } else {
                "role unspecified; name inferred"
            };
            credits.push(format!("{name} ({role})"));
        }
    }
    if !taper.trim().is_empty() {
        credits.push(format!("Recording credit: {}", clean(taper)));
    }
    if !transfer.trim().is_empty() {
        credits.push(format!("Transfer/mastering credit: {}", clean(transfer)));
    }
    Profile {
        kind: primary.into(),
        modifiers,
        people,
        credits: credits.join(" | "),
        evidence: clean(&all),
        inferred: explicit.is_none() && primary != "UNKNOWN",
    }
}
impl Profile {
    pub fn summary(&self) -> String {
        std::iter::once(self.kind.as_str())
            .chain(self.modifiers.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
fn person_match(needle: &str, profile: &Profile) -> bool {
    let n = needle.to_lowercase();
    if n == "betty" {
        return profile.people.iter().any(|p| p == "Betty Cantor-Jackson");
    }
    if n == "miller" {
        return profile.people.iter().any(|p| p == "Charlie Miller");
    }
    profile.people.iter().any(|p| p.to_lowercase().contains(&n)) || profile.evidence.to_lowercase().contains(&n)
}
pub fn compare(a: &Profile, b: &Profile, p: &Priorities) -> Ordering {
    rank(a, p).cmp(&rank(b, p))
}
pub fn rank(x: &Profile, p: &Priorities) -> (usize, usize, usize) {
    let source = |x: &Profile| p.sources.iter().position(|s| s == &x.kind).unwrap_or(p.sources.len());
    let person = |x: &Profile| p.people.iter().position(|n| person_match(n, x)).unwrap_or(p.people.len());
    let modifier = |x: &Profile| p.modifiers.iter().position(|n| x.modifiers.contains(n)).unwrap_or(p.modifiers.len());
    if p.people_first { (person(x), source(x), modifier(x)) } else { (source(x), person(x), modifier(x)) }
}
#[derive(Default)]
struct Catalogue {
    records: Vec<Recording>,
    shows: HashMap<String, String>,
}
impl Catalogue {
    fn parse(text: &str) -> Result<Self, String> {
        let doc = roxmltree::Document::parse(text).map_err(|e| format!("Invalid show catalogue XML: {e}"))?;
        let root = doc.root_element();
        if root.tag_name().name() != "gratefulDeadCatalogue" || root.attribute("schemaVersion") != Some("1") {
            return Err("Unsupported show catalogue schema.".into());
        }
        let mut result = Self::default();
        for show in root.descendants().filter(|n| n.has_tag_name("show")) {
            let date = show.attribute("date").unwrap_or_default();
            let venue = show.attribute("venue").unwrap_or_default();
            let mut detail = format!(
                "{date}\n{venue}\n{}, {} {}",
                show.attribute("city").unwrap_or_default(),
                show.attribute("state").unwrap_or_default(),
                show.attribute("country").unwrap_or_default()
            );
            let performances: Vec<_> = show.children().filter(|n| n.has_tag_name("performance")).collect();
            if show.attribute("basis") == Some("recording-metadata") {
                detail
                    .push_str("\nDate present in recording metadata; no performance entry in the show-data snapshot.");
            }
            if performances.len() > 1 {
                detail.push_str("\nMultiple performance/setlist variants on this date; recordings are not assigned to a variant without evidence.");
            }
            for (i, p) in performances.iter().enumerate() {
                if performances.len() > 1 {
                    detail += &format!("\n\nPerformance {}: {}", i + 1, p.attribute("venue").unwrap_or_default());
                }
                for set in p.children().filter(|n| n.has_tag_name("set")) {
                    detail += &format!("\n\nSet {}", set.attribute("number").unwrap_or_default());
                    for (i, song) in set.children().filter(|n| n.has_tag_name("song")).enumerate() {
                        detail += &format!("\n{}. {}", i + 1, song.attribute("name").unwrap_or_default());
                    }
                }
            }
            if performances.is_empty() {
                detail.push_str("\n\nNo setlist in this snapshot.");
            }
            let refs = show
                .children()
                .filter(|n| n.has_tag_name("reference"))
                .filter_map(|n| n.attribute("url"))
                .collect::<Vec<_>>();
            if !refs.is_empty() {
                detail += &format!("\n\nSources:\n{}", refs.join("\n"));
            }
            result.shows.insert(date.into(), detail);
            for r in show.children().filter(|n| n.has_tag_name("recording")) {
                let key = r.attribute("key").unwrap_or_default();
                let provider = r.attribute("provider").unwrap_or_default();
                if key.is_empty() || !["Archive.org", "etree"].contains(&provider) {
                    return Err("Catalogue recording has an invalid provider/key.".into());
                }
                let get = |name: &str| {
                    r.children().find(|n| n.has_tag_name(name)).and_then(|n| n.text()).unwrap_or_default().to_owned()
                };
                let modifiers = r
                    .attribute("modifiers")
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let source_kind = r.attribute("type").unwrap_or("UNKNOWN").to_owned();
                let summary = std::iter::once(source_kind.as_str())
                    .chain(modifiers.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" | ");
                result.records.push(Recording {
                    title: r.attribute("title").unwrap_or(key).into(),
                    source: provider.into(),
                    url: if provider == "etree" {
                        format!("https://bt.etree.org/details.php?torrentId={}", key.trim_start_matches("etree-"))
                    } else {
                        format!("https://archive.org/details/{key}")
                    },
                    key: key.into(),
                    detail: get("detail"),
                    source_kind,
                    source_summary: summary,
                    credits: get("credits"),
                    date: date.into(),
                    venue: venue.into(),
                    show_info: String::new(),
                    evidence: get("evidence"),
                    modifiers,
                    inferred: r.attribute("inferredType") == Some("true"),
                });
            }
        }
        Ok(result)
    }
}
type Cache = HashMap<PathBuf, (std::time::SystemTime, Arc<Catalogue>)>;
fn load(path: &Path) -> Result<Arc<Catalogue>, String> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let metadata = fs::metadata(path).map_err(|e| format!("Show catalogue is unavailable: {e}"))?;
    if metadata.len() > 128 * 1024 * 1024 {
        return Err("Show catalogue exceeds 128 MiB.".into());
    }
    let modified = metadata.modified().map_err(|e| e.to_string())?;
    if let Some((old, data)) = cache.get(path)
        && *old == modified
    {
        return Ok(data.clone());
    }
    let parsed = Arc::new(Catalogue::parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)?);
    cache.insert(path.into(), (modified, parsed.clone()));
    Ok(parsed)
}
pub fn profile(r: &Recording) -> Profile {
    Profile {
        kind: r.source_kind.clone(),
        modifiers: r.modifiers.clone(),
        people: ALIASES.iter().filter(|(name, _)| r.credits.contains(name)).map(|(name, _)| (*name).into()).collect(),
        credits: r.credits.clone(),
        evidence: r.evidence.clone(),
        inferred: r.inferred,
    }
}
pub fn search(path: &Path, query: &str, provider: &str, priorities: &Priorities) -> Result<Vec<Recording>, String> {
    let data = load(path)?;
    let tokens = query.to_lowercase().split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let mut rows = data
        .records
        .iter()
        .filter(|r| provider == "All sources" || provider.is_empty() || r.source == provider)
        .filter(|r| {
            let text = format!(
                "{} {} {} {} {} {} {}",
                r.date,
                r.title,
                r.venue,
                r.source_summary,
                r.credits,
                r.evidence,
                data.shows.get(&r.date).map(String::as_str).unwrap_or_default()
            )
            .to_lowercase();
            tokens.iter().all(|q| text.contains(q))
        })
        .cloned()
        .collect::<Vec<_>>();
    rows.sort_by_cached_key(|r| {
        (rank(&profile(r), priorities), std::cmp::Reverse(r.date.clone()), r.title.clone(), r.key.clone())
    });
    rows.truncate(500);
    for r in &mut rows {
        r.show_info = data.shows.get(&r.date).cloned().unwrap_or_default();
    }
    Ok(rows)
}
fn xml(value: &str) -> String {
    value
        .chars()
        .filter(|&c| c == '\n' || c == '\r' || c == '\t' || c >= ' ')
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}
pub fn compile(facts: &Path, output: &Path) -> Result<(), String> {
    use std::fmt::Write as _;
    let facts: Value =
        serde_json::from_slice(&fs::read(facts).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gratefulDeadCatalogue schemaVersion=\"1\" artist=\"Grateful Dead\" startYear=\"1967\" endYear=\"1995\" capturedDate=\"2026-10-29\">\n",
    );
    out.push_str("  <coverage>Source-attributed snapshot, not a claim of complete or definitive historical knowledge. Unknown dates are retained separately. Set numbering follows the source; encore roles are not inferred. Recording availability is checked before download.</coverage>\n  <sources>\n");
    for s in facts["sources"].as_array().ok_or("Missing fact sources")? {
        let _ = writeln!(
            out,
            "    <source id=\"{}\" url=\"{}\" sha256=\"{}\" scope=\"{}\" />",
            xml(&text(s, "id")),
            xml(&text(s, "url")),
            xml(&text(s, "sha256")),
            xml(&text(s, "scope"))
        );
    }
    out.push_str("  </sources>\n  <recordingTaxonomy>\n    <type code=\"SBD\" meaning=\"Soundboard\"/><type code=\"MTX\" meaning=\"Matrix of sources\"/><type code=\"AUD\" meaning=\"Audience recording\"/><type code=\"FM\" meaning=\"Broadcast source without a more specific known primary type\"/><type code=\"UNKNOWN\" meaning=\"Not established by available evidence\"/>\n    <modifier code=\"FOB\" meaning=\"Front of board microphone location\"/><modifier code=\"FM\" meaning=\"FM broadcast\"/><modifier code=\"PRE-FM\" meaning=\"Pre-broadcast source\"/><modifier code=\"DSBD\" meaning=\"Digital soundboard\"/><modifier code=\"ULTRAMATRIX\" meaning=\"Ultramatrix source lineage\"/>\n    <rankingPolicy>Personal priorities are separate from facts. Supplied default: SBD, MTX, AUD, FM, UNKNOWN; then Betty, Miller.</rankingPolicy>\n  </recordingTaxonomy>\n  <shows>\n");
    for s in facts["shows"].as_array().ok_or("Missing shows")? {
        let date = text(s, "date");
        if date.get(..4).and_then(|s| s.parse::<u32>().ok()).is_none_or(|y| !(1967..=1995).contains(&y)) {
            return Err("A show lies outside 1967–1995.".into());
        }
        let _ = writeln!(
            out,
            "    <show date=\"{}\" venue=\"{}\" city=\"{}\" state=\"{}\" country=\"{}\" basis=\"{}\">",
            xml(&date),
            xml(&text(s, "venue")),
            xml(&text(s, "city")),
            xml(&text(s, "state")),
            xml(&text(s, "country")),
            if s["performances"].as_array().is_some_and(|p| !p.is_empty()) {
                "show-database"
            } else {
                "recording-metadata"
            }
        );
        for p in s["performances"].as_array().ok_or("Missing performances")? {
            let _ = writeln!(
                out,
                "      <performance id=\"{}\" venue=\"{}\" city=\"{}\" state=\"{}\" country=\"{}\" sourceRef=\"gdshowsdb\">",
                xml(&text(p, "id")),
                xml(&text(p, "venue")),
                xml(&text(p, "city")),
                xml(&text(p, "state")),
                xml(&text(p, "country"))
            );
            for set in p["sets"].as_array().ok_or("Missing sets")? {
                let _ = writeln!(out, "        <set number=\"{}\" role=\"unspecified\">", set["number"]);
                for song in set["songs"].as_array().ok_or("Missing songs")? {
                    let _ = writeln!(
                        out,
                        "          <song name=\"{}\" segueToNext=\"{}\" />",
                        xml(&crate::shows::clean_title(&text(song, "name"))),
                        song["segue"].as_bool().unwrap_or(false)
                    );
                }
                out.push_str("        </set>\n");
            }
            out.push_str("      </performance>\n");
        }
        for r in s["recordings"].as_array().ok_or("Missing recordings")? {
            let p = classify(
                &text(r, "source_text"),
                &if text(r, "source") == "etree" { text(r, "title") } else { text(r, "key") },
                &text(r, "taper"),
                &text(r, "transferer"),
                &text(r, "lineage"),
                &text(r, "hints"),
            );
            let _ = writeln!(
                out,
                "      <recording key=\"{}\" provider=\"{}\" title=\"{}\" type=\"{}\" modifiers=\"{}\" inferredType=\"{}\" sourceRefs=\"{}\">",
                xml(&text(r, "key")),
                xml(&text(r, "source")),
                xml(&text(r, "title")),
                p.kind,
                p.modifiers.join(" "),
                p.inferred,
                r["provenance"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            for (name, pattern) in ALIASES.iter().filter(|(name, _)| p.people.iter().any(|p| p == name)) {
                let role = if has(&text(r, "taper"), pattern) {
                    "recording"
                } else if has(&text(r, "transferer"), pattern) {
                    "transfer-mastering"
                } else {
                    "unspecified"
                };
                let _ = writeln!(out, "        <person name=\"{}\" role=\"{role}\" />", xml(name));
            }
            for (role, field) in [("recording", "taper"), ("transfer-mastering", "transferer")] {
                let value = clean(&text(r, field));
                if !value.is_empty() {
                    let _ = writeln!(out, "        <creditText role=\"{role}\">{}</creditText>", xml(&value));
                }
            }
            for (name, value) in [
                ("credits", p.credits),
                (
                    "evidence",
                    format!(
                        "{} {} {} {} {}",
                        text(r, "title"),
                        text(r, "source_text"),
                        clean(&text(r, "taper")),
                        clean(&text(r, "transferer")),
                        text(r, "lineage")
                    ),
                ),
                ("sourceChain", text(r, "source_text")),
                ("lineage", text(r, "lineage")),
                ("detail", text(r, "detail")),
            ] {
                let _ = writeln!(out, "        <{name}>{}</{name}>", xml(&value));
            }
            for url in
                r["references"].as_array().ok_or("Missing recording references")?.iter().filter_map(Value::as_str)
            {
                let _ = writeln!(out, "        <reference url=\"{}\" />", xml(url));
            }
            out.push_str("      </recording>\n");
        }
        for url in s["references"].as_array().ok_or("Missing show references")?.iter().filter_map(Value::as_str) {
            let _ = writeln!(out, "      <reference url=\"{}\" />", xml(url));
        }
        out.push_str("    </show>\n");
    }
    out.push_str("  </shows>\n  <unplacedRecords>\n");
    for r in facts["unplaced"].as_array().ok_or("Missing unplaced records")? {
        let _ = writeln!(
            out,
            "    <record sourceRef=\"{}\" dateText=\"{}\" reason=\"{}\" />",
            xml(&text(r, "source")),
            xml(&text(r, "date_text")),
            xml(&text(r, "reason"))
        );
    }
    out.push_str("  </unplacedRecords>\n</gratefulDeadCatalogue>\n");
    Catalogue::parse(&out)?;
    fs::write(output, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separates_matrix_broadcast_and_location() {
        let p = classify("Ultramatrix SBD > cassette", "gd1988.fob.fm", "", "Charlie Miller", "", "");
        assert_eq!(p.summary(), "MTX | FOB | ULTRAMATRIX | FM");
        assert!(p.credits.contains("transfer/mastering"));
    }
    #[test]
    fn does_not_turn_david_cantor_into_betty() {
        let p = classify("AUD", "gd.cantor", "David Cantor", "", "", "");
        assert!(!p.people.iter().any(|p| p.contains("Betty")));
        assert!(classify("AUD", "1971 Miller Auditorium", "", "", "", "").people.is_empty());
        assert_eq!(classify("SBD+AUD", "gd.aud", "", "", "", "").kind, "MTX");
        assert_eq!(classify("SBD", "gd.aud", "", "", "", "").kind, "SBD");
    }
    #[test]
    fn rank_is_personal_and_configurable() {
        let a = classify("SBD", "betty", "", "", "", "");
        let b = classify("AUD", "miller", "", "", "", "");
        let mut p = Priorities::default();
        assert_eq!(compare(&a, &b, &p), Ordering::Less);
        p.sources = vec!["AUD".into(), "SBD".into()];
        assert_eq!(compare(&a, &b, &p), Ordering::Greater);
        p.people_first = true;
        assert_eq!(compare(&a, &b, &p), Ordering::Less);
    }
    #[test]
    fn priorities_persist_and_reject_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = Priorities { sources: vec!["AUD".into(), "SBD".into()], ..Default::default() };
        let saved = p.save(dir.path()).unwrap();
        assert_eq!(Priorities::load(dir.path()), saved);
        assert_eq!(saved.sources.len(), 5);
        assert!(Priorities { sources: vec!["BANANA".into()], ..Default::default() }.validate().is_err());
    }
    #[test]
    fn xml_disables_dtd_and_preserves_unknowns() {
        assert!(Catalogue::parse("<!DOCTYPE x [<!ENTITY ext SYSTEM 'file:///x'>]><gratefulDeadCatalogue schemaVersion='1'>&ext;</gratefulDeadCatalogue>").is_err());
        let p = classify("", "gd1977-12-27", "", "", "", "");
        assert_eq!(p.kind, "UNKNOWN");
    }
}
