//! Links a local show (an import folder or a packaged MKA) to its BerthaML show, the listed
//! recording it came from, and the setlist performance each track holds.
//!
//! Recording evidence comes only from names: the folder, the notes file and checksum files,
//! which collectors name after the recording itself. Notes text is never searched for
//! identifiers or shnids, because a matrix's notes list its component sources'.
use crate::catalogue::{self, Catalogue, Source};
use crate::shows::rx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// What a local show offers for linking. Stored with a packaged MKA, so its link can be
/// worked out again whenever the corpus changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evidence {
    pub date: String,
    /// The venue as the notes or album give it, for choosing between shows on one date.
    pub venue: String,
    /// Folder, notes and checksum file names without extensions.
    pub names: Vec<String>,
    pub tracks: Vec<EvidenceTrack>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EvidenceTrack {
    /// The source file name, or empty for an MKA chapter.
    pub file: String,
    pub title: String,
    pub seconds: f64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// BerthaML show ID, empty when the date has no single show.
    pub show: String,
    /// BerthaML recording ID, empty when no listed recording matched.
    pub recording: String,
    /// The recording's provider key, for display.
    pub recording_key: String,
    /// identifier, shnid, source-name, track-files, track-durations, date or empty.
    pub method: String,
    /// certain, probable or empty.
    pub certainty: String,
    /// One plain sentence or two saying what matched.
    pub detail: String,
    /// Performance IDs held by each local track, in track order.
    pub performances: Vec<Vec<String>>,
}

/// A song title for comparison, past case, punctuation, a leading The, an ampersand and a
/// dropped g. Like BerthaML's title_key, except that both "Lovin'" and "Loving" become
/// "lovin", so a file name that lost its apostrophe still matches.
pub(crate) fn title_key(title: &str) -> String {
    let text = title.to_lowercase().replace('&', " and ");
    let text = rx(r"([a-z])in['’]").replace_all(&text, "${1}in");
    let text = rx(r"([a-z])ing\b").replace_all(&text, "${1}in");
    let text = rx(r"^\s*the\s+").replace(&text, "");
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// A recording name with the two-digit year written out, so gd77-04-23 meets gd1977-04-23.
fn identifier(name: &str) -> String {
    let name = name.trim().to_lowercase();
    rx(r"^gd(\d\d)([-.]\d\d[-.]\d\d)").replace(&name, "gd19$1$2").into_owned()
}
/// The parts of a name that describe the source: no date and no numbers, which are shnids,
/// torrent numbers or disc counts.
fn source_name(name: &str) -> String {
    let name = identifier(name);
    let name = rx(r"^gd\d{4}[-.]\d\d[-.]\d\d").replace(&name, "");
    name.split(['.', '_', ' ', '-'])
        .filter(|t| !t.is_empty() && !t.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(".")
}
fn numbers(name: &str) -> BTreeSet<String> {
    name.split(['.', '_', ' ', '-'])
        .filter(|t| (3..=7).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit()))
        // A year in a folder name such as "1977-10-30 Assembly Hall" is not a shnid.
        .filter(|t| !(t.len() == 4 && ("1965".."1996").contains(t)))
        .map(str::to_owned)
        .collect()
}
fn keys(source: &Source) -> Vec<String> {
    // An etree key is a torrent number; its title is the recording's name.
    let name = if source.provider == "etree" { &source.title } else { &source.key };
    vec![identifier(name)]
}
/// The single tape a rule picks out, or none when it picks several or nothing. A tape listed
/// more than once is named by its Archive listing.
fn only(found: Vec<&Source>) -> Option<&Source> {
    let first = *found.first()?;
    found
        .iter()
        .all(|s| same_tape(first, s))
        .then(|| found.iter().find(|s| s.provider == "Archive.org").copied().unwrap_or(first))
}
/// Listings that share a shnid are one tape, as when Archive.org and etree both list it.
fn same_tape(a: &Source, b: &Source) -> bool {
    a.id == b.id || a.shnids.iter().any(|n| b.shnids.contains(n))
}
/// The listed recording a local show came from, with its method, certainty and reason.
fn recording<'a>(
    sources: &'a [Source],
    evidence: &Evidence,
) -> Option<(&'a Source, &'static str, &'static str, String)> {
    let names = evidence.names.iter().filter(|n| !n.trim().is_empty()).collect::<Vec<_>>();
    let ids = names.iter().map(|n| identifier(n)).collect::<Vec<_>>();
    if let Some(s) = only(sources.iter().filter(|s| keys(s).iter().any(|k| ids.contains(k))).collect()) {
        let what = if s.provider == "etree" { "etree torrent name" } else { "Archive identifier" };
        return Some((s, "identifier", "certain", format!("its {what} matches the folder or file names")));
    }
    let local = names.iter().flat_map(|n| numbers(n)).collect::<BTreeSet<_>>();
    if let Some(s) = only(sources.iter().filter(|s| s.shnids.iter().any(|n| local.contains(n))).collect()) {
        let shnid = s.shnids.iter().find(|n| local.contains(*n)).cloned().unwrap_or_default();
        return Some((s, "shnid", "probable", format!("shnid {shnid} is in the folder or file names")));
    }
    let local = names.iter().map(|n| source_name(n)).filter(|n| n.contains('.')).collect::<Vec<_>>();
    if let Some(s) = only(sources.iter().filter(|s| keys(s).iter().any(|k| local.contains(&source_name(k)))).collect())
    {
        return Some((
            s,
            "source-name",
            "probable",
            "its name matches the folder or file names once dates and numbers are set aside".into(),
        ));
    }
    let files =
        evidence.tracks.iter().filter(|t| !t.file.is_empty()).map(|t| t.file.to_lowercase()).collect::<Vec<_>>();
    if !files.is_empty() {
        let score = |s: &Source| {
            s.playlists
                .iter()
                .map(|p| files.iter().filter(|f| p.iter().any(|(file, _, _)| file.to_lowercase() == **f)).count())
                .max()
                .unwrap_or(0)
        };
        let best = sources.iter().map(score).max().unwrap_or(0);
        if best * 2 >= files.len()
            && best > 0
            && let Some(s) = only(sources.iter().filter(|s| score(s) == best).collect())
        {
            let certainty = if best == files.len() { "certain" } else { "probable" };
            return Some((
                s,
                "track-files",
                certainty,
                format!("{best} of {} file names match its track list", files.len()),
            ));
        }
    }
    let seconds = evidence.tracks.iter().map(|t| t.seconds).collect::<Vec<_>>();
    let timed = |s: &&Source| {
        s.playlists.iter().any(|p| {
            p.len() == seconds.len()
                && p.iter()
                    .zip(&seconds)
                    .all(|((_, listed, _), local)| listed.is_some_and(|l| (l - local).abs() <= 1.5))
        })
    };
    if !seconds.is_empty()
        && let Some(s) = only(sources.iter().filter(timed).collect())
    {
        return Some((
            s,
            "track-durations",
            "probable",
            format!("all {} track durations match its track list within 1.5 seconds", seconds.len()),
        ));
    }
    None
}
/// Each track's performances, walking the setlist in order so a song played twice is placed
/// by its neighbours. A track whose title matches nothing ahead (tuning, a misnamed file)
/// holds none and does not move the walk.
fn by_title(songs: &[(String, Vec<String>)], tracks: &[EvidenceTrack]) -> Vec<Vec<String>> {
    let mut next = 0;
    let mut found_any = false;
    tracks
        .iter()
        .map(|track| {
            let whole = title_key(&track.title);
            let parts = if songs.iter().any(|(_, k)| k.contains(&whole)) {
                vec![whole]
            } else {
                rx(r"\s*(?:/|>)\s*").split(&track.title).map(title_key).collect()
            };
            let mut held = Vec::new();
            for part in parts.into_iter().filter(|p| !p.is_empty()) {
                // The first match may skip ahead, as when a tape starts late; later ones stay close.
                let limit = if found_any { (next + 8).min(songs.len()) } else { songs.len() };
                if let Some(j) = (next..limit).find(|&j| songs[j].1.contains(&part)) {
                    held.push(songs[j].0.clone());
                    next = j + 1;
                    found_any = true;
                }
            }
            held
        })
        .collect()
}
pub fn link(database: &Path, evidence: &Evidence) -> Result<Link, String> {
    Ok(link_in(&*catalogue::load(database)?, evidence))
}
pub(crate) fn link_in(data: &Catalogue, evidence: &Evidence) -> Link {
    let shows = data.shows.get(&evidence.date).map(Vec::as_slice).unwrap_or_default();
    let sources = data.sources.get(&evidence.date).map(Vec::as_slice).unwrap_or_default();
    let mut link = Link::default();
    let mut reason;
    if let Some((found, method, certainty, why)) = recording(sources, evidence) {
        // An etree match names the same tape as its Archive listing, which carries the identifier.
        let source = only(sources.iter().filter(|s| same_tape(found, s)).collect()).unwrap_or(found);
        link.show = source.show.clone();
        link.recording = source.id.clone();
        link.recording_key = if source.provider == "etree" { source.title.clone() } else { source.key.clone() };
        link.method = method.into();
        link.certainty = certainty.into();
        reason = format!("Recording {} on {}: {why}.", link.recording_key, link.show);
        // A track list that the local files match one for one says what each track holds.
        if matches!(method, "track-files" | "track-durations") {
            let files = evidence.tracks.iter().map(|t| t.file.to_lowercase()).collect::<Vec<_>>();
            let list = source.playlists.iter().find(|p| {
                p.len() == files.len()
                    && (method == "track-durations"
                        || p.iter().zip(&files).all(|((f, _, _), l)| f.to_lowercase() == *l))
            });
            if let Some(list) = list.filter(|l| l.iter().any(|(_, _, c)| !c.is_empty())) {
                link.performances = list.iter().map(|(_, _, covers)| covers.clone()).collect();
            }
        }
    } else if let Some(i) = catalogue::choose(shows, &evidence.venue) {
        link.show = shows[i].id.clone();
        link.method = "date".into();
        link.certainty = "probable".into();
        let several = if shows.len() > 1 { " and venue" } else { "" };
        reason = format!(
            "Show {} by date{several}. No listed recording matched its names, file names or track lengths.",
            link.show
        );
    } else if shows.len() > 1 {
        reason = format!("{} BerthaML shows on {}, and nothing names one of them.", shows.len(), evidence.date);
    } else {
        reason = format!(
            "No BerthaML show on {}.",
            if evidence.date.is_empty() { "an unknown date" } else { &evidence.date }
        );
    }
    if link.performances.is_empty()
        && let Some(show) = shows.iter().find(|s| s.id == link.show)
    {
        link.performances = by_title(&show.songs, &evidence.tracks);
    }
    let held = link.performances.iter().filter(|p| !p.is_empty()).count();
    if !link.show.is_empty() {
        reason += &format!(" {held} of {} tracks matched to setlist performances.", evidence.tracks.len());
    }
    link.detail = reason;
    link
}
/// What an import offers: the folder's names, and each track's file name, title and length.
pub fn import_evidence(draft: &crate::shows::Draft) -> Evidence {
    Evidence {
        date: draft.date.clone(),
        venue: draft.venue.clone(),
        names: folder_names(Path::new(&draft.folder), Path::new(&draft.notes_path)),
        tracks: draft
            .tracks
            .iter()
            .map(|t| EvidenceTrack {
                file: Path::new(&t.path).file_name().unwrap_or_default().to_string_lossy().into_owned(),
                title: t.title.clone(),
                seconds: t.duration,
            })
            .collect(),
    }
}
/// What an MKA offers when its source folder was not recorded: its date, from the title
/// (`771227_...` or `1977-12-27`) or file name, the title as its place, and its chapters.
pub fn mka_evidence(path: &Path, title: &str) -> Evidence {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let date = [title, stem.as_str()]
        .iter()
        .find_map(|text| {
            let full = rx(r"\b(19[6-9]\d-[01]\d-[0-3]\d)\b").captures(text).map(|c| c[1].to_owned());
            let album =
                rx(r"^([6-9]\d)([01]\d)([0-3]\d)_").captures(text).map(|c| format!("19{}-{}-{}", &c[1], &c[2], &c[3]));
            full.or(album)
        })
        .or_else(|| crate::shows::date_from_name(&stem))
        .unwrap_or_default();
    Evidence {
        date,
        venue: title.replace(['_', '-'], " "),
        names: vec![stem],
        tracks: crate::matroska::read_chapters(path)
            .into_iter()
            .map(|(title, seconds)| EvidenceTrack { title, seconds, ..Default::default() })
            .collect(),
    }
}
/// The names a folder offers: its own, its notes file's, and its checksum files', which
/// collectors name after the recording.
pub fn folder_names(folder: &Path, notes_path: &Path) -> Vec<String> {
    let stem = |p: &Path| {
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let name = rx(r"(?i)\.(txt|ffp|md5|st5|sha1|sfv)$").replace(&name, "").into_owned();
        rx(r"(?i)[._]shntool_len$").replace(&name, "").into_owned()
    };
    let mut names = vec![folder.file_name().unwrap_or_default().to_string_lossy().into_owned()];
    if notes_path.is_file() {
        names.push(stem(notes_path));
    }
    if let Ok(entries) = std::fs::read_dir(folder) {
        let mut checksums = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| rx(r"(?i)\.(ffp|md5|st5)$").is_match(&p.to_string_lossy()))
            .map(|p| stem(&p))
            .collect::<Vec<_>>();
        checksums.sort();
        names.extend(checksums);
    }
    names.retain(|n| !n.is_empty());
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: &str, key: &str, shnids: &[&str], playlists: Vec<Vec<crate::catalogue::ListedTrack>>) -> Source {
        Source {
            id: id.into(),
            show: "gd1977-10-30".into(),
            provider: "Archive.org".into(),
            key: key.into(),
            title: String::new(),
            shnids: shnids.iter().map(|s| (*s).into()).collect(),
            playlists,
        }
    }
    fn catalogue() -> Catalogue {
        let mut data = Catalogue::default();
        let songs = [
            ("p01", "Promised Land"),
            ("p02", "Playing In The Band"),
            ("p03", "Drums"),
            ("p04", "Playing In The Band"),
            ("p05", "Good Lovin'"),
        ];
        data.shows.insert(
            "1977-10-30".into(),
            vec![crate::catalogue::test_show(
                "gd1977-10-30",
                songs.iter().map(|(id, t)| ((*id).to_owned(), vec![title_key(t)])).collect(),
            )],
        );
        let list = |files: &[&str], seconds: &[f64]| {
            files
                .iter()
                .zip(seconds)
                .enumerate()
                .map(|(i, (f, s))| ((*f).to_owned(), Some(*s), vec![format!("p0{}", i + 1)]))
                .collect()
        };
        data.sources.insert(
            "1977-10-30".into(),
            vec![
                source("r-sbd", "gd1977-10-30.sbd-fix.miller.99838.sbeok.flac16", &["99838"], vec![]),
                source("r-mtx", "gd1977-10-30.137374.mtx.dusborne.flac16", &["137374"], vec![]),
                source(
                    "r-aud",
                    "gd77-10-30.aud.wise.12345.flac16",
                    &["12345"],
                    vec![list(&["a1.flac", "a2.flac"], &[300.0, 600.0])],
                ),
                Source {
                    provider: "etree".into(),
                    title: "gd1977-10-30.137374.mtx.dusborne.flac16".into(),
                    ..source("r-mtx-etree", "590366", &["137374"], vec![])
                },
            ],
        );
        data
    }
    fn evidence(names: &[&str]) -> Evidence {
        Evidence {
            date: "1977-10-30".into(),
            names: names.iter().map(|s| (*s).into()).collect(),
            tracks: ["Promised Land", "Playin' in the Band", "Drums / Playing in the Band", "tuning", "Good Lovin"]
                .iter()
                .map(|t| EvidenceTrack { title: (*t).into(), seconds: 1.0, ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn identifiers_meet_across_two_and_four_digit_years() {
        let link = link_in(&catalogue(), &evidence(&["gd77-10-30.sbd-fix.miller.99838.sbeok.flac16"]));
        assert_eq!(
            (link.recording.as_str(), link.method.as_str(), link.certainty.as_str()),
            ("r-sbd", "identifier", "certain")
        );
    }
    #[test]
    fn shnid_and_source_name_come_from_names_only() {
        let link = link_in(&catalogue(), &evidence(&["1977-10-30 Assembly Hall", "gd1977-10-30.137374.mtx.flac1644"]));
        assert_eq!((link.recording.as_str(), link.method.as_str()), ("r-mtx", "shnid"));
        let link = link_in(
            &catalogue(),
            &evidence(&["1977-10-30 Assembly Hall Dusborne Matrix", "gd77-10-30.mtx.dusborne.flac16"]),
        );
        assert_eq!((link.recording.as_str(), link.method.as_str()), ("r-mtx", "source-name"));
        // A matrix's notes naming its soundboard source must not link it to that source.
        let mut notes_only = evidence(&["1977-10-30 Assembly Hall"]);
        notes_only.venue = "Assembly Hall (shnid:99838)".into();
        let link = link_in(&catalogue(), &notes_only);
        assert_eq!((link.recording.as_str(), link.show.as_str(), link.method.as_str()), ("", "gd1977-10-30", "date"));
    }
    #[test]
    fn track_lists_link_by_file_name_and_duration() {
        let mut files = evidence(&["my copy"]);
        files.tracks = vec![
            EvidenceTrack { file: "A1.flac".into(), title: "x".into(), seconds: 1.0 },
            EvidenceTrack { file: "a2.flac".into(), title: "y".into(), seconds: 1.0 },
        ];
        let link = link_in(&catalogue(), &files);
        assert_eq!(
            (link.recording.as_str(), link.method.as_str(), link.certainty.as_str()),
            ("r-aud", "track-files", "certain")
        );
        assert_eq!(link.performances, vec![vec!["p01".to_owned()], vec!["p02".to_owned()]]);
        files.tracks[0].file.clear();
        files.tracks[1].file.clear();
        files.tracks[0].seconds = 300.9;
        files.tracks[1].seconds = 599.0;
        assert_eq!(link_in(&catalogue(), &files).method, "track-durations");
        files.tracks[1].seconds = 590.0;
        assert_eq!(link_in(&catalogue(), &files).method, "date");
    }
    #[test]
    fn titles_walk_the_setlist_in_order() {
        let link = link_in(&catalogue(), &evidence(&[]));
        let held = link.performances.iter().map(|p| p.join(" ")).collect::<Vec<_>>();
        assert_eq!(held, ["p01", "p02", "p03 p04", "", "p05"]);
        assert!(link.detail.ends_with("4 of 5 tracks matched to setlist performances."));
        assert_eq!(link_in(&catalogue(), &Evidence { date: "1977-10-31".into(), ..Default::default() }).show, "");
    }
    #[test]
    fn title_keys_forgive_a_dropped_g() {
        for (a, b) in [
            ("Good Lovin'", "Good Loving"),
            ("Dancin In The Street", "Dancing in the Street"),
            ("Peggy-O", "Peggy O"),
            ("The Music Never Stopped", "Music Never Stopped"),
            ("Around & Around", "Around and Around"),
        ] {
            assert_eq!(title_key(a), title_key(b));
        }
        assert_ne!(title_key("Rain"), title_key("Raining"));
    }
    #[test]
    fn mka_dates_come_from_album_title_or_file_name() {
        let path = Path::new("D:/shows/770423-ma-springfield.mka");
        assert_eq!(mka_evidence(path, "771227_ca-san-francisco-winterland").date, "1977-12-27");
        assert_eq!(
            mka_evidence(path, "Grateful Dead - April 23, 1977 · 1977-04-23 · Civic Center Arena").date,
            "1977-04-23"
        );
        assert_eq!(mka_evidence(path, "").date, "1977-04-23");
    }
    #[test]
    fn folder_names_include_notes_and_checksum_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("1977-10-30 Assembly Hall Dusborne Matrix");
        std::fs::create_dir(&folder).unwrap();
        for name in [
            "gd77-10-30.mtx.dusborne.flac16.txt",
            "gd77-10-30.mtx.dusborne.flac16.ffp",
            "gd77-10-30.mtx.dusborne.flac16.shntool_len.txt",
            "foo_dr.txt",
        ] {
            std::fs::write(folder.join(name), b"").unwrap();
        }
        let names = folder_names(&folder, &folder.join("gd77-10-30.mtx.dusborne.flac16.txt"));
        assert_eq!(names, ["1977-10-30 Assembly Hall Dusborne Matrix", "gd77-10-30.mtx.dusborne.flac16"]);
    }
}
