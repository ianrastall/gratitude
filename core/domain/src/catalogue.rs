//! Offline show facts from the compiled BerthaML database, and personal recording priorities.
use crate::shows::{Place, Recording};
use regex::Regex;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
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
        // Migrate free-form preferences to the two supported source choices.
        let position = |kind: &str| self.sources.iter().position(|s| s == kind).unwrap_or(usize::MAX);
        let matrix_first = position("MTX") < position("SBD");
        self.sources =
            if matrix_first { ["MTX", "SBD", "AUD", "FM", "UNKNOWN"] } else { ["SBD", "MTX", "AUD", "FM", "UNKNOWN"] }
                .map(str::to_owned)
                .into();
        self.people = Self::default().people;
        self.modifiers.clear();
        self.people_first = false;
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
    ("Owsley Stanley", r"(?i)\bowsley\b|\bbear[- ._]+stanley\b|[._-]bear(?:[._-]|$)|^bear$"),
    ("Dick Latvala", r"(?i)\b(?:dick[- ._]+latvala|latvala)\b"),
    ("David Lemieux", r"(?i)\b(?:david[- ._]+lemieux|lemieux)\b"),
    ("Jeffrey Norman", r"(?i)\bjeff(?:rey)?[- ._]+norman\b|[._-]norman(?:[._-]|$)|^norman$"),
    ("Paul Stubblebine", r"(?i)\b(?:paul[- ._]+stubblebine|stubblebine)\b"),
    ("Kidd Candelario", r"(?i)\b(?:kidd[- ._]+candelario|candelario)\b|[._-]kidd(?:[._-]|$)|^kidd$"),
    ("Rex Jackson", r"(?i)\brex[- ._]+jackson\b|[._-]rex(?:[._-]|$)|^rex$"),
    ("Bob Wagner", r"(?i)\bbob[- ._]+wagner\b|[._-]wagner(?:[._-]|$)|^wagner$"),
    ("Rob Bertrando", r"(?i)\b(?:rob[- ._]+bertrando|bertrando)\b"),
    ("Jerry Moore", r"(?i)\bjerry[- ._]+moore\b|[._-]moore(?:[._-]|$)|^moore$"),
    ("Barry Glassberg", r"(?i)\b(?:barry[- ._]+glassberg|glassberg)\b"),
    ("Oade Brothers", r"(?i)\boade(?:[- ._]+(?:brothers|bros))?\b"),
    ("Jim Wise", r"(?i)\bjim[- ._]+wise\b|[._-]wise(?:[._-]|$)|^wise$"),
    ("Joani Walker", r"(?i)\bjoani[- ._]+walker\b|[._-]walker(?:[._-]|$)|^walker$"),
    ("Paul Scotton", r"(?i)\b(?:paul[- ._]+scotton|scotton)\b"),
    ("Steve Rolfe", r"(?i)\b(?:steve[- ._]+rolfe|rolfe)\b"),
    ("Reinhart Holwein", r"(?i)\b(?:reinhart[- ._]+holwein|holwein)\b"),
    ("Marty Weinberg", r"(?i)\b(?:marty[- ._]+weinberg|weinberg)\b"),
    ("Bob Menke", r"(?i)\b(?:bob[- ._]+menke|menke)\b"),
    ("Louis Falanga", r"(?i)\b(?:louis[- ._]+falanga|falanga)\b"),
    ("Ted Mattes", r"(?i)\b(?:ted[- ._]+mattes|mattes)\b"),
    ("Baker/Bruns", r"(?i)\bbaker\s*/\s*bruns\b|[._-](?:baker|bruns)(?:[._-]|$)|^(?:baker|bruns)$"),
    ("Ken and Judy Lee", r"(?i)\b(?:ken(?:\s*(?:and|&)\s*judy)?|judy)[- ._]+lee\b"),
    ("Chris Chappell", r"(?i)\b(?:chris[- ._]+chappell|chappell|sick[- ._]*bits)\b"),
    ("Kevin Tobin", r"(?i)\b(?:kevin[- ._]+tobin|tobin)\b"),
    ("SIRMick", r"(?i)\bsirmick\b"),
    ("Hucky", r"(?i)\bhucky\b"),
];

// User-supplied house policy, not historical facts or a universal quality claim.
// Members of a tier tie; alphabetical/identifier ordering supplies stable display.
const SBD_TIERS: &[&[&str]] = &[
    &["Betty Cantor-Jackson", "Owsley Stanley", "Dan Healy"],
    &[
        "Charlie Miller",
        "Dick Latvala",
        "David Lemieux",
        "Jeffrey Norman",
        "Paul Stubblebine",
        "Kidd Candelario",
        "Rex Jackson",
    ],
];
const AUD_TIERS: &[&[&str]] = &[
    &["Bob Wagner", "Rob Bertrando", "Jerry Moore", "Barry Glassberg", "Rango Keshavan"],
    &["Oade Brothers", "Jim Wise", "Joani Walker", "Paul Scotton", "Steve Rolfe", "Reinhart Holwein"],
    &["Marty Weinberg", "Bob Menke", "Louis Falanga", "Ted Mattes", "Baker/Bruns", "Ken and Judy Lee"],
];
const MTX_TIERS: &[&[&str]] =
    &[&["Hunter Seamons", "Dave Usborne"], &["Chris Chappell", "Kevin Tobin", "SIRMick", "Hucky"]];
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
/// People in a profile known for recording rather than transfer: audience tapers and the
/// sound engineers who ran the board.
pub fn known_tapers(profile: &Profile) -> Vec<String> {
    let recorded = |name: &str| SBD_TIERS[0].contains(&name) || AUD_TIERS.iter().any(|tier| tier.contains(&name));
    profile.people.iter().filter(|p| recorded(p)).cloned().collect()
}
impl Profile {
    pub fn summary(&self) -> String {
        std::iter::once(self.kind.as_str())
            .chain(self.modifiers.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
pub fn compare(a: &Profile, b: &Profile, p: &Priorities) -> Ordering {
    rank(a, p).cmp(&rank(b, p))
}
pub fn rank(x: &Profile, p: &Priorities) -> (usize, usize, usize) {
    let p = p.clone().validate().unwrap_or_default();
    let source = p.sources.iter().position(|s| s == &x.kind).unwrap_or(p.sources.len());
    let tiers = match x.kind.as_str() {
        "SBD" | "FM" => SBD_TIERS,
        "AUD" => AUD_TIERS,
        "MTX" => MTX_TIERS,
        _ => &[],
    };
    let credit =
        tiers.iter().position(|tier| tier.iter().any(|name| x.people.iter().any(|p| p == name))).unwrap_or(tiers.len());
    (source, credit, 0)
}
/// The database schema this build reads; compile_db.py sets it as user_version.
const DATABASE_VERSION: i64 = 1;
/// One BerthaML show as Gratitude presents it.
pub(crate) struct Show {
    pub(crate) id: String,
    venue: String,
    names: Vec<String>,
    place: Place,
    info: String,
    /// Song performances in stage order: performance ID and the song's title keys.
    pub(crate) songs: Vec<(String, Vec<String>)>,
}
#[cfg(test)]
pub(crate) fn test_show(id: &str, songs: Vec<(String, Vec<String>)>) -> Show {
    Show { id: id.into(), venue: String::new(), names: Vec::new(), place: Place::default(), info: String::new(), songs }
}
/// One track of an original-file track list: file name, seconds when measured, covered performance IDs.
pub(crate) type ListedTrack = (String, Option<f64>, Vec<String>);
/// A listed recording, as the linker compares it with a local folder.
pub(crate) struct Source {
    pub(crate) id: String,
    pub(crate) show: String,
    pub(crate) provider: String,
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) shnids: Vec<String>,
    pub(crate) playlists: Vec<Vec<ListedTrack>>,
}
#[derive(Default)]
pub(crate) struct Catalogue {
    records: Vec<Recording>,
    pub(crate) shows: HashMap<String, Vec<Show>>,
    excluded: HashMap<String, Vec<String>>,
    /// Recordings by their show's date.
    pub(crate) sources: HashMap<String, Vec<Source>>,
    /// SHA-256 over the compiled corpus, from the database's meta table.
    pub(crate) corpus: String,
}
/// What the offline show database says about an imported show's date.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reference {
    pub info: String,
    /// Set when the date's show, or the one the notes' venue names, is known.
    pub place: Option<Place>,
}
fn rows<T>(
    db: &rusqlite::Connection,
    sql: &str,
    f: impl FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    db.prepare(sql)?.query_map([], f)?.collect()
}
fn grouped<T>(items: Vec<(String, T)>) -> HashMap<String, Vec<T>> {
    let mut map = HashMap::<String, Vec<T>>::new();
    for (key, item) in items {
        map.entry(key).or_default().push(item);
    }
    map
}
fn corroboration(level: &str) -> &str {
    match level {
        "reviewed" => "reviewed",
        "recording-backed" => "setlist matches a recording's track list",
        "sources-agree" => "two setlist sources agree",
        "single-source" => "setlist from one source",
        "disputed" => "setlist sources disagree",
        "differs-from-sources" => "setlist edited away from its sources",
        "no-setlist" => "no setlist",
        other => other,
    }
}
impl Catalogue {
    fn read(path: &Path) -> Result<Self, String> {
        use rusqlite::OpenFlags;
        let db = rusqlite::Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("Show database is unavailable: {e}"))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        if version != DATABASE_VERSION {
            return Err(format!("Unsupported show database version {version}."));
        }
        Self::query(&db).map_err(|e| format!("Invalid show database: {e}"))
    }
    fn query(db: &rusqlite::Connection) -> rusqlite::Result<Self> {
        let text = |r: &rusqlite::Row, i: usize| r.get::<_, String>(i);
        let people: HashMap<_, _> =
            rows(db, "SELECT id, name FROM people", |r| Ok((text(r, 0)?, text(r, 1)?)))?.into_iter().collect();
        let songs: HashMap<_, _> =
            rows(db, "SELECT id, title FROM songs", |r| Ok((text(r, 0)?, text(r, 1)?)))?.into_iter().collect();
        let venues: HashMap<_, _> = rows(db, "SELECT id, name, city, place FROM venues", |r| {
            Ok((text(r, 0)?, Place { venue: text(r, 1)?, city: text(r, 2)?, region: text(r, 3)? }))
        })?
        .into_iter()
        .collect();
        let venue_names = grouped(rows(db, "SELECT venue, name FROM venue_names", |r| Ok((text(r, 0)?, text(r, 1)?)))?);
        let eras = rows(db, "SELECT start, finish, members FROM roster_eras", |r| {
            Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?))
        })?;
        let sets = grouped(rows(db, "SELECT show, n, kind, number, mode FROM sets ORDER BY show, n", |r| {
            Ok((text(r, 0)?, (r.get::<_, i64>(1)?, text(r, 2)?, r.get::<_, i64>(3)?, text(r, 4)?)))
        })?);
        let performances = grouped(rows(
            db,
            "SELECT show, set_n, song, event, transition FROM performances ORDER BY show, set_n, n",
            |r| Ok((text(r, 0)?, (r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?, text(r, 3)?, text(r, 4)?))),
        )?);
        let ids = grouped(rows(
            db,
            "SELECT show, id FROM performances WHERE song IS NOT NULL ORDER BY show, set_n, n",
            |r| Ok((text(r, 0)?, text(r, 1)?)),
        )?);
        let lineups = grouped(rows(db, "SELECT show, person, instruments FROM lineup ORDER BY show, n", |r| {
            Ok((text(r, 0)?, (text(r, 1)?, text(r, 2)?)))
        })?);
        let cites = grouped(rows(db, "SELECT show, title, href FROM show_cites ORDER BY show, n", |r| {
            Ok((text(r, 0)?, (text(r, 1)?, text(r, 2)?)))
        })?);
        let mut titles = HashMap::<String, Vec<String>>::new();
        for (song, title) in rows(db, "SELECT song, title FROM song_titles", |r| Ok((text(r, 0)?, text(r, 1)?)))? {
            titles.entry(song).or_default().push(crate::linking::title_key(&title));
        }
        let tapes: HashMap<_, _> = rows(db, "SELECT show, count(*) FROM recordings GROUP BY show", |r| {
            Ok((text(r, 0)?, r.get::<_, i64>(1)?))
        })?
        .into_iter()
        .collect();
        let mut result = Self::default();
        let shows = rows(
            db,
            "SELECT id, date, venue, billed, slot, status, coverage, corroboration FROM shows ORDER BY date, id",
            |r| {
                (0..8)
                    .map(|i| r.get::<_, Option<String>>(i).map(Option::unwrap_or_default))
                    .collect::<Result<Vec<_>, _>>()
            },
        )?;
        for row in shows {
            let Ok([id, date, venue, billed, slot, status, coverage, level]) = <[String; 8]>::try_from(row) else {
                continue;
            };
            let place = venues.get(&venue).cloned().unwrap_or_default();
            let location = [&place.venue, &place.city, &place.region]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            let mut info = format!("{date} · {location}");
            if !billed.is_empty() && billed != place.venue {
                info += &format!("\nBilled as {billed}");
            }
            if slot == "early" || slot == "late" {
                info += &format!("\n{}{} show", slot[..1].to_uppercase(), &slot[1..]);
            }
            let review = match status.as_str() {
                "draft" | "skeleton" => format!("{status} record, imported and not reviewed"),
                _ => format!("{status} record"),
            };
            info += &match corroboration(&level) {
                "" | "reviewed" => format!("\nBerthaML {id}: {review}."),
                level => format!("\nBerthaML {id}: {review}; {level}."),
            };
            let show_sets = sets.get(&id).map(Vec::as_slice).unwrap_or_default();
            let encores = show_sets.iter().filter(|s| s.1 == "encore").count();
            for (set_n, kind, number, mode) in show_sets {
                info += "\n\n";
                info += &match kind.as_str() {
                    "set" => format!("Set {number}"),
                    "encore" if encores == 1 => "Encore".into(),
                    "encore" => format!("Encore {number}"),
                    "soundcheck" => "Soundcheck".into(),
                    _ => "Set unknown".into(),
                };
                if mode != "unknown" {
                    info += &format!(" ({mode})");
                }
                let mut n = 0;
                for (_, song, event, transition) in
                    performances.get(&id).into_iter().flatten().filter(|p| p.0 == *set_n)
                {
                    match song {
                        Some(song) => {
                            n += 1;
                            let title = songs.get(song).map(String::as_str).unwrap_or(song);
                            let segue = if transition == "segue" { " >" } else { "" };
                            info += &format!("\n{n}. {title}{segue}");
                        }
                        None => info += &format!("\n({})", event.replace('-', " ")),
                    }
                }
            }
            match coverage.as_str() {
                "none" => info += "\n\nNo setlist is recorded.",
                "partial" => info += "\n\nThe setlist may be incomplete.",
                _ => {}
            }
            let name = |id: &str| people.get(id).cloned().unwrap_or_else(|| id.to_owned());
            if let Some(lineup) = lineups.get(&id) {
                let players = lineup
                    .iter()
                    .map(|(p, i)| {
                        let instruments = i.split_whitespace().map(|s| s.replace('-', " ")).collect::<Vec<_>>();
                        format!("{} ({})", name(p), instruments.join(", "))
                    })
                    .collect::<Vec<_>>();
                info += &format!("\n\nLineup: {}", players.join("; "));
            } else if let Some((_, _, members)) =
                eras.iter().find(|(start, finish, _)| *start <= date && date <= *finish)
            {
                let members = members.split_whitespace().map(name).collect::<Vec<_>>();
                info += &format!(
                    "\n\nBand members for this period, from membership dates rather than a record of who played: {}",
                    members.join(", ")
                );
            }
            match tapes.get(&id).copied().unwrap_or(0) {
                0 => {}
                1 => info += "\n\n1 circulating recording is listed for this show.",
                n => info += &format!("\n\n{n} circulating recordings are listed for this show."),
            }
            if let Some(cites) = cites.get(&id) {
                info += "\n\nSources:";
                for (title, href) in cites {
                    info += &format!("\n{title}: {href}");
                }
            }
            let mut names = venue_names.get(&venue).cloned().unwrap_or_default();
            names.push(billed);
            let show_songs = performances
                .get(&id)
                .into_iter()
                .flatten()
                .filter_map(|(_, song, _, _)| song.as_ref())
                .zip(ids.get(&id).into_iter().flatten())
                .map(|(song, performance)| (performance.clone(), titles.get(song).cloned().unwrap_or_default()))
                .collect();
            result.shows.entry(date).or_default().push(Show { id, venue, names, place, info, songs: show_songs });
        }
        result.excluded = grouped(rows(db, "SELECT date, category, venue FROM excluded_events ORDER BY date", |r| {
            Ok((text(r, 0)?, format!("{}: {}", text(r, 1)?, text(r, 2)?)))
        })?);
        result.records = rows(
            db,
            "SELECT r.provider, r.key, r.title, r.kind, r.inferred, r.modifiers, r.credits, r.lineage, s.date, \
             coalesce(v.name, '') FROM recordings r JOIN shows s ON s.id = r.show LEFT JOIN venues v ON v.id = s.venue",
            |r| {
                let (provider, key, title) = (text(r, 0)?, text(r, 1)?, text(r, 2)?);
                let (source_kind, modifiers) = (text(r, 3)?, text(r, 5)?);
                let (credits, lineage) = (text(r, 6)?, text(r, 7)?);
                let modifiers = modifiers.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
                Ok(Recording {
                    url: match provider.as_str() {
                        "Archive.org" => format!("https://archive.org/details/{key}"),
                        "etree" => {
                            format!("https://bt.etree.org/details.php?torrentId={}", key.trim_start_matches("etree-"))
                        }
                        _ => String::new(),
                    },
                    source_summary: std::iter::once(source_kind.as_str())
                        .chain(modifiers.iter().map(String::as_str))
                        .collect::<Vec<_>>()
                        .join(" | "),
                    evidence: format!("{title} {key} {credits} {lineage}"),
                    detail: if lineage.is_empty() { String::new() } else { format!("Lineage: {lineage}") },
                    title,
                    source: provider,
                    key,
                    source_kind,
                    credits,
                    date: text(r, 8)?,
                    venue: text(r, 9)?,
                    show_info: String::new(),
                    modifiers,
                    inferred: r.get::<_, i64>(4)? != 0,
                })
            },
        )?;
        let shnids = grouped(rows(db, "SELECT recording, value FROM recording_ids WHERE scheme = 'shnid'", |r| {
            Ok((text(r, 0)?, text(r, 1)?))
        })?);
        let mut playlists = HashMap::<String, Vec<(String, Vec<ListedTrack>)>>::new();
        for (recording, playlist, file, seconds, covers) in rows(
            db,
            "SELECT recording, playlist, file, seconds, covers FROM tracks ORDER BY recording, playlist, n",
            |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, r.get::<_, Option<f64>>(3)?, text(r, 4)?)),
        )? {
            let lists = playlists.entry(recording).or_default();
            if lists.last().is_none_or(|(id, _)| *id != playlist) {
                lists.push((playlist, Vec::new()));
            }
            let covers = covers.split_whitespace().map(str::to_owned).collect();
            lists.last_mut().expect("pushed above").1.push((file, seconds, covers));
        }
        for (date, source) in rows(
            db,
            "SELECT s.date, r.id, r.show, r.provider, r.key, r.title FROM recordings r JOIN shows s ON s.id = r.show",
            |r| Ok((text(r, 0)?, [text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?])),
        )? {
            let [id, show, provider, key, title] = source;
            result.sources.entry(date).or_default().push(Source {
                shnids: shnids.get(&id).cloned().unwrap_or_default(),
                playlists: playlists.remove(&id).unwrap_or_default().into_iter().map(|(_, t)| t).collect(),
                id,
                show,
                provider,
                key,
                title,
            });
        }
        result.corpus = db
            .query_row("SELECT value FROM meta WHERE key = 'corpus_sha256'", [], |r| r.get(0))
            .optional()?
            .unwrap_or_default();
        Ok(result)
    }
    /// Everything known about a date, the chosen show first.
    fn date_info(&self, date: &str, first: Option<usize>) -> String {
        let shows = self.shows.get(date).map(Vec::as_slice).unwrap_or_default();
        let mut order = (0..shows.len()).collect::<Vec<_>>();
        if let Some(i) = first {
            order.retain(|&j| j != i);
            order.insert(0, i);
        }
        let mut parts = Vec::new();
        if shows.len() > 1 {
            parts.push(format!("{} BerthaML shows on this date.", shows.len()));
        }
        parts.extend(order.into_iter().map(|i| shows[i].info.clone()));
        if let Some(excluded) = self.excluded.get(date) {
            parts.push(format!("Events on this date kept outside the show records:\n{}", excluded.join("\n")));
        }
        parts.join("\n\n")
    }
}
/// Lowercase words padded with spaces, so containment matches whole words.
fn words(text: &str) -> String {
    let text = text.to_lowercase().replace('&', " and ").replace(['\'', '’', '.'], "");
    format!(" {} ", text.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
}
type Cache = HashMap<PathBuf, (std::time::SystemTime, Arc<Catalogue>)>;
pub(crate) fn load(path: &Path) -> Result<Arc<Catalogue>, String> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let metadata = fs::metadata(path).map_err(|e| format!("Show database is unavailable: {e}"))?;
    let modified = metadata.modified().map_err(|e| e.to_string())?;
    if let Some((old, data)) = cache.get(path)
        && *old == modified
    {
        return Ok(data.clone());
    }
    let parsed = Arc::new(Catalogue::read(path)?);
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
/// Offline show reference for a local import; does not replace the folder's notes.
/// When several shows share the date at different venues, the notes' venue picks one.
pub fn reference(path: &Path, date: &str, notes_venue: &str) -> Result<Option<Reference>, String> {
    let data = load(path)?;
    let shows = data.shows.get(date).map(Vec::as_slice).unwrap_or_default();
    if shows.is_empty() && !data.excluded.contains_key(date) {
        return Ok(None);
    }
    let one = choose(shows, notes_venue);
    Ok(Some(Reference { info: data.date_info(date, one), place: one.map(|i| shows[i].place.clone()) }))
}
/// The show on a date that the notes' venue, or failing that its city, names. Early and late
/// shows at one venue count as one choice; several venues with nothing to tell them apart, none.
pub(crate) fn choose(shows: &[Show], notes_venue: &str) -> Option<usize> {
    let notes = words(notes_venue);
    let named = |test: &dyn Fn(&Show) -> bool| (0..shows.len()).filter(|&i| test(&shows[i])).collect::<Vec<_>>();
    let mut chosen = named(&|s| {
        s.names.iter().any(|n| {
            let n = words(n);
            !n.trim().is_empty() && notes.contains(&n)
        })
    });
    if chosen.is_empty() {
        chosen = named(&|s| !s.place.city.is_empty() && notes.contains(&words(&s.place.city)));
    }
    if chosen.is_empty() {
        chosen = (0..shows.len()).collect();
    }
    chosen.first().copied().filter(|&i| chosen.iter().all(|&j| shows[j].venue == shows[i].venue))
}
pub fn search(path: &Path, query: &str, provider: &str, priorities: &Priorities) -> Result<Vec<Recording>, String> {
    let data = load(path)?;
    let tokens = query.to_lowercase().split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let infos = data.shows.keys().map(|date| (date.as_str(), data.date_info(date, None))).collect::<HashMap<_, _>>();
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
                infos.get(r.date.as_str()).map(String::as_str).unwrap_or_default()
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
        r.show_info = infos.get(r.date.as_str()).cloned().unwrap_or_default();
    }
    Ok(rows)
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
    fn only_soundboard_matrix_order_is_personal() {
        let a = classify("SBD", "betty", "", "", "", "");
        let b = classify("AUD", "miller", "", "", "", "");
        let mut p = Priorities::default();
        assert_eq!(compare(&a, &b, &p), Ordering::Less);
        p.sources = vec!["AUD".into(), "MTX".into(), "SBD".into()];
        assert_eq!(compare(&a, &b, &p), Ordering::Less);
        p.people_first = true;
        assert_eq!(compare(&a, &b, &p), Ordering::Less);
        let matrix = classify("MTX", "", "", "", "", "");
        assert_eq!(compare(&matrix, &a, &p), Ordering::Less);
        assert_eq!(compare(&a, &matrix, &Priorities::default()), Ordering::Less);
    }
    #[test]
    fn priorities_persist_and_reject_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let p = Priorities { sources: vec!["AUD".into(), "SBD".into()], ..Default::default() };
        let saved = p.save(dir.path()).unwrap();
        assert_eq!(Priorities::load(dir.path()), saved);
        assert_eq!(saved.sources.len(), 5);
        assert_eq!(saved.sources[0], "SBD");
        assert_eq!(saved.people, vec!["Betty", "Miller"]);
        assert!(Priorities { sources: vec!["BANANA".into()], ..Default::default() }.validate().is_err());
    }
    #[test]
    fn supplied_tiers_are_category_specific_and_tied() {
        let p = Priorities::default();
        let recording = |kind, taper, transfer| classify(kind, "", taper, transfer, "", "");
        let betty = recording("SBD", "Betty Cantor", "");
        let bear = recording("SBD", "Owsley Stanley", "");
        let miller = recording("SBD", "", "Charlie Miller");
        assert_eq!(compare(&betty, &bear, &p), Ordering::Equal);
        assert_eq!(compare(&betty, &miller, &p), Ordering::Less);
        let wagner = recording("AUD", "Bob Wagner", "Charlie Miller");
        let weinberg = recording("AUD", "Marty Weinberg", "Charlie Miller");
        assert_eq!(compare(&wagner, &weinberg, &p), Ordering::Less);
        let hunter = recording("MTX", "", "Hunter Seamons");
        let usborne = recording("MTX", "", "Dave Usborne");
        let tobin = recording("MTX", "", "Kevin Tobin");
        assert_eq!(compare(&hunter, &usborne, &p), Ordering::Equal);
        assert_eq!(compare(&hunter, &tobin, &p), Ordering::Less);
        assert_eq!(recording("AUD", "Oade Brothers", "").people, vec!["Oade Brothers"]);
    }
    #[test]
    fn preserves_unknown_source_types() {
        let p = classify("", "gd1977-12-27", "", "", "", "");
        assert_eq!(p.kind, "UNKNOWN");
    }
    /// A database with compile_db.py's schema: two shows on one date at different venues.
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("berthaml.db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(include_str!("../../../berthaml/tools/berthaml-db.sql")).unwrap();
        db.execute_batch(
            "INSERT INTO people VALUES ('p-garcia', 'Jerry Garcia', 'Garcia, Jerry'), ('p-weir', 'Bob Weir', 'Weir, Bob');
             INSERT INTO venues VALUES ('v-fillmore', 'Fillmore Auditorium', 'San Francisco', 'CA', 'US', 'CA'),
                                       ('v-okeefe', 'O''Keefe Centre', 'Toronto', '', 'CA', 'Canada');
             INSERT INTO venue_names VALUES ('v-fillmore', 'Fillmore Auditorium'), ('v-okeefe', 'O''Keefe Centre'),
                                            ('v-okeefe', 'Hummingbird Centre');
             INSERT INTO songs VALUES ('s-bertha', 'Bertha', 'song'), ('s-good-lovin', 'Good Lovin''', 'song');
             INSERT INTO shows VALUES ('gd1967-07-31-fillmore', '1967-07-31', 'v-fillmore', '', 'unknown', 'draft', 'partial', 'sources-agree'),
                                      ('gd1967-07-31-okeefe', '1967-07-31', 'v-okeefe', 'O''Keefe', 'unknown', 'reviewed', 'complete', 'reviewed');
             INSERT INTO show_cites VALUES ('gd1967-07-31-okeefe', 1, 'Jerrybase', 'https://jerrybase.com/events/19670731-01');
             INSERT INTO sets VALUES ('gd1967-07-31-okeefe', 1, 'set', 1, 'electric'), ('gd1967-07-31-okeefe', 2, 'encore', 1, 'unknown');
             INSERT INTO performances VALUES ('gd1967-07-31-okeefe', 'p01-01', 1, 1, 's-bertha', '', 'segue', ''),
                                             ('gd1967-07-31-okeefe', 'p01-00', 1, 2, NULL, 'tuning', 'unknown', ''),
                                             ('gd1967-07-31-okeefe', 'p01-02', 1, 3, 's-good-lovin', '', 'stop', ''),
                                             ('gd1967-07-31-okeefe', 'p02-01', 2, 1, 's-bertha', '', 'unknown', '');
             INSERT INTO roster_eras VALUES ('1967-01-01', '1967-09-24', 'p-garcia p-weir');
             INSERT INTO excluded_events VALUES ('1967-07-30', 'Private', 'RCA Studio A, Hollywood, CA', 'https://jerrybase.com/');
             INSERT INTO recordings VALUES
               ('r-1', 'gd1967-07-31-okeefe', 'Archive.org', 'gd67-07-31.aud', 'Live at O''Keefe', 'AUD', 0, '', '', ''),
               ('r-2', 'gd1967-07-31-okeefe', 'etree', '1234', 'gd67-07-31.sbd.miller', 'SBD', 1, 'FM',
                'Charlie Miller (transfer/mastering)', 'Reel > DAT');",
        )
        .unwrap();
        (dir, path)
    }
    #[test]
    fn notes_venue_picks_between_shows_on_one_date() {
        let (_dir, path) = fixture();
        let toronto = reference(&path, "1967-07-31", "O'Keefe Centre, Toronto, Ontario").unwrap().unwrap();
        let place = toronto.place.unwrap();
        assert_eq!(crate::shows::album_name("1967-07-31", &place), "670731_canada-toronto-okeefe-centre");
        assert!(toronto.info.starts_with("2 BerthaML shows on this date.\n\n1967-07-31 · O'Keefe Centre, Toronto"));
        assert!(toronto.info.contains("Set 1 (electric)\n1. Bertha >\n(tuning)\n2. Good Lovin'\n\nEncore\n1. Bertha"));
        assert!(toronto.info.contains("Billed as O'Keefe"));
        assert!(
            toronto.info.contains("from membership dates rather than a record of who played: Jerry Garcia, Bob Weir")
        );
        assert!(toronto.info.contains("2 circulating recordings"));
        let by_city = reference(&path, "1967-07-31", "Somewhere, San Francisco CA").unwrap().unwrap();
        assert_eq!(by_city.place.unwrap().venue, "Fillmore Auditorium");
        assert!(reference(&path, "1967-07-31", "").unwrap().unwrap().place.is_none());
        let private = reference(&path, "1967-07-30", "").unwrap().unwrap();
        assert!(private.place.is_none() && private.info.contains("Private: RCA Studio A"));
        assert!(reference(&path, "1967-08-01", "").unwrap().is_none());
    }
    #[test]
    fn database_recordings_rank_and_carry_show_facts() {
        let (_dir, path) = fixture();
        let found = search(&path, "1967-07-31", "All sources", &Priorities::default()).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].source_kind, "SBD");
        assert!(found[0].inferred && found[0].source_summary == "SBD | FM");
        assert_eq!(found[0].url, "https://bt.etree.org/details.php?torrentId=1234");
        assert!(profile(&found[0]).people.contains(&"Charlie Miller".to_owned()));
        assert!(found[1].show_info.contains("Good Lovin'") && found[1].url.ends_with("/gd67-07-31.aud"));
        assert_eq!(search(&path, "good lovin", "etree", &Priorities::default()).unwrap().len(), 1);
    }
    #[test]
    fn refuses_other_database_versions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("berthaml.db");
        rusqlite::Connection::open(&path).unwrap().execute_batch("PRAGMA user_version = 2;").unwrap();
        assert!(reference(&path, "1977-12-27", "").unwrap_err().contains("version 2"));
    }
}
