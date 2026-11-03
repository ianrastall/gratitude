//! The standard show report: one plain-text layout for every show, whatever its original
//! notes looked like. Written beside both outputs and shown for any MKA. The original
//! notes are never replaced; the report says where they are.
use crate::catalogue::{self, ShowDetail};
use crate::linking::Link;
use std::path::Path;

/// One track or chapter as the report lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportTrack {
    pub title: String,
    pub seconds: f64,
    /// The set the notes gave, used when the setlist does not place the track.
    pub set: String,
    pub performances: Vec<String>,
}

/// Everything a report is drawn from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportSource {
    pub date: String,
    /// The place as the notes gave it, used when the show is not linked.
    pub place: String,
    pub album: String,
    pub source_summary: String,
    pub credits: String,
    pub comment: String,
    /// Codec, rate, depth and channels, when known.
    pub audio: String,
    /// Where the original notes are, such as "show-notes.txt, attached to the MKA".
    pub notes: String,
    pub tracks: Vec<ReportTrack>,
    pub link: Link,
}

/// "Sunday, May 22, 1977" from "1977-05-22"; anything else comes back as it is.
pub fn long_date(iso: &str) -> String {
    let parts = iso.split('-').map(|p| p.parse::<i64>().ok()).collect::<Vec<_>>();
    let [Some(year), Some(month), Some(day)] = parts[..] else { return iso.to_owned() };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return iso.to_owned();
    }
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    // Sakamoto's method for the day of the week.
    const OFFSETS: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let weekday = (y + y / 4 - y / 100 + y / 400 + OFFSETS[(month - 1) as usize] + day).rem_euclid(7);
    format!("{}, {} {day}, {year}", DAYS[weekday as usize], MONTHS[(month - 1) as usize])
}

pub(crate) fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn status_line(show: &ShowDetail) -> String {
    let s = &show.summary;
    let review = match s.status.as_str() {
        "draft" | "skeleton" => format!("{} record, imported and not reviewed", s.status),
        other => format!("{other} record"),
    };
    match s.corroboration.as_str() {
        "" | "reviewed" => format!("BerthaML {}: {review}.", s.id),
        level => format!("BerthaML {}: {review}; {level}.", s.id),
    }
}

/// Renders the report. Without a show database or a link it still lists what the notes gave.
pub fn render(source: &ReportSource, database: &Path) -> String {
    let show =
        (!source.link.show.is_empty()).then(|| catalogue::show(database, &source.link.show).ok().flatten()).flatten();
    let entry = |id: &str| show.as_ref().and_then(|s| s.setlist.iter().find(|e| e.id == id));
    let mut out = Vec::<String>::new();

    out.push(crate::shows::ARTIST.to_owned());
    out.push(if source.date.is_empty() { "Date unknown".to_owned() } else { source.date.clone() });
    let place =
        show.as_ref().map(|s| s.summary.place.clone()).filter(|p| !p.is_empty()).unwrap_or(source.place.clone());
    if !place.is_empty() {
        out.push(place);
    }
    if let Some(tour) = show.as_ref().map(|s| s.summary.tour.clone()).filter(|t| !t.is_empty()) {
        out.push(format!("Tour: {tour}"));
    }
    if !source.album.is_empty() {
        out.push(format!("Album: {}", source.album));
    }

    out.push(String::new());
    let mut field = |label: &str, value: &str| {
        if !value.trim().is_empty() {
            out.push(format!("{label}: {}", value.trim()));
        }
    };
    field("Source", &source.source_summary);
    field("Credits", &source.credits);
    field("Comment", &source.comment);
    if !source.link.recording_key.is_empty() {
        field("Recording", &source.link.recording_key);
    }
    field("Audio", &source.audio);
    let total: f64 = source.tracks.iter().map(|t| t.seconds).sum();
    let count = source.tracks.len();
    field("Length", &format!("{} in {count} {}", clock(total), if count == 1 { "track" } else { "tracks" }));

    // Each track goes under the set its first performance belongs to, else the set the
    // notes gave it, else the set before it; tracks before the first placed one, such as
    // an opening jam or tuning, go under that first set.
    let digits = count.to_string().len();
    let mut lines = source
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let segue = t.performances.last().and_then(|p| entry(p)).is_some_and(|e| e.segue);
            let set = t.performances.first().and_then(|p| entry(p)).map(|e| e.set.clone()).unwrap_or(t.set.clone());
            (format!("{:0digits$}. {}{}", i + 1, t.title, if segue { " >" } else { "" }), clock(t.seconds), set)
        })
        .collect::<Vec<_>>();
    if let Some(first) = lines.iter().map(|(_, _, s)| s.clone()).find(|s| !s.is_empty()) {
        for (_, _, set) in lines.iter_mut().take_while(|(_, _, s)| s.is_empty()) {
            *set = first.clone();
        }
    }
    let width = lines.iter().map(|(l, _, _)| l.chars().count()).max().unwrap_or(0).min(56);
    let mut set = String::new();
    let mut headed = false;
    for (line, length, track_set) in &lines {
        if !track_set.is_empty() && *track_set != set {
            set = track_set.clone();
            out.push(String::new());
            out.push(set.clone());
            headed = true;
        } else if !headed {
            out.push(String::new());
            out.push("Tracks".into());
            headed = true;
        }
        let pad = width.saturating_sub(line.chars().count());
        out.push(format!("{line}{}  {length:>8}", " ".repeat(pad)));
    }

    let mut notes = Vec::new();
    if let Some(show) = &show {
        let held = source.tracks.iter().flat_map(|t| t.performances.iter()).collect::<Vec<_>>();
        let missing = show
            .setlist
            .iter()
            .filter(|e| !held.contains(&&e.id))
            .map(|e| format!("{} ({})", e.title, e.set))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            notes.push(format!("Not in this recording: {}.", missing.join(", ")));
        }
        let unplaced = source
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.performances.is_empty())
            .map(|(i, t)| format!("{:0digits$} {}", i + 1, t.title))
            .collect::<Vec<_>>();
        if !unplaced.is_empty() {
            notes.push(format!("Tracks that match no setlist song: {}.", unplaced.join(", ")));
        }
    }
    if !source.link.detail.is_empty() {
        notes.push(source.link.detail.clone());
    }
    if !notes.is_empty() {
        out.push(String::new());
        out.push("Notes".into());
        out.extend(notes.into_iter().map(|n| format!("- {n}")));
    }

    out.push(String::new());
    if let Some(show) = &show {
        out.push(status_line(show));
    }
    if !source.notes.is_empty() {
        out.push(format!("Original notes: {}", source.notes));
    }
    out.push(format!("Report written by Gratitude {}.", env!("CARGO_PKG_VERSION")));
    out.join("\n") + "\n"
}

/// The report for an import, from its reviewed fields. The link is worked out again from
/// them, so retitled or reordered tracks are placed by their final titles. `notes` says
/// where the original notes will be; empty means beside the source audio.
pub fn for_draft(draft: &crate::shows::Draft, database: &Path, notes: &str) -> String {
    let mut draft = draft.clone();
    let link = crate::linking::link(database, &crate::linking::import_evidence(&draft)).unwrap_or_default();
    draft.set_link(&link);
    let audio =
        draft.tracks.first().map(|t| audio_line(&t.codec, t.sample_rate, t.bit_depth, t.channels)).unwrap_or_default();
    let notes = match Path::new(&draft.notes_path).file_name() {
        None => String::new(),
        Some(_) if !notes.is_empty() => notes.to_owned(),
        Some(name) => format!("{}, in the source folder", name.to_string_lossy()),
    };
    render(
        &ReportSource {
            date: draft.date.clone(),
            place: draft.venue.clone(),
            album: draft.album.clone(),
            source_summary: draft.source_summary.clone(),
            credits: draft.credits.clone(),
            comment: draft.comment.clone(),
            audio,
            notes,
            tracks: draft
                .tracks
                .iter()
                .map(|t| ReportTrack {
                    title: crate::shows::song_title(&t.title),
                    seconds: t.duration,
                    set: t.set.clone(),
                    performances: t.performances.clone(),
                })
                .collect(),
            link,
        },
        database,
    )
}

/// "FLAC, 44,100 Hz, 16-bit, 2 channels", leaving out what is unknown.
pub fn audio_line(codec: &str, rate: u32, depth: u32, channels: u32) -> String {
    let mut parts = Vec::new();
    if !codec.is_empty() {
        parts.push(codec.to_uppercase());
    }
    if rate > 0 {
        let thousands = rate / 1000;
        parts.push(format!("{thousands},{:03} Hz", rate % 1000));
    }
    if depth > 0 {
        parts.push(format!("{depth}-bit"));
    }
    match channels {
        0 => {}
        1 => parts.push("1 channel".into()),
        n => parts.push(format!("{n} channels")),
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lists_tracks_under_sets_with_lengths() {
        let source = ReportSource {
            date: "1977-05-08".into(),
            place: "Barton Hall - Ithaca, NY".into(),
            album: "770508_ny-ithaca-barton-hall".into(),
            source_summary: "SBD".into(),
            comment: "Taper: Betty Cantor-Jackson".into(),
            audio: audio_line("flac", 44100, 16, 2),
            notes: "show-notes.txt".into(),
            tracks: vec![
                ReportTrack {
                    title: "Minglewood Blues".into(),
                    seconds: 331.0,
                    set: "Set 1".into(),
                    ..Default::default()
                },
                ReportTrack { title: "Tuning".into(), seconds: 65.4, ..Default::default() },
                ReportTrack {
                    title: "Scarlet Begonias".into(),
                    seconds: 3725.0,
                    set: "Set 2".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let report = render(&source, Path::new("no-such.db"));
        assert!(report.starts_with(
            "Grateful Dead\n1977-05-08\nBarton Hall - Ithaca, NY\nAlbum: 770508_ny-ithaca-barton-hall\n\n"
        ));
        assert!(report.contains("Audio: FLAC, 44,100 Hz, 16-bit, 2 channels\nLength: 1:08:41 in 3 tracks"));
        assert!(report.contains("\n\nSet 1\n1. Minglewood Blues      5:31\n2. Tuning                1:05\n\nSet 2\n3. Scarlet Begonias   1:02:05\n"));
        assert!(report.ends_with(&format!(
            "Original notes: show-notes.txt\nReport written by Gratitude {}.\n",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(!report.contains("Credits:"), "empty fields are left out");
    }
    #[test]
    fn an_opening_jam_joins_the_first_set() {
        let track = |title: &str, set: &str| ReportTrack {
            title: title.into(),
            seconds: 60.0,
            set: set.into(),
            ..Default::default()
        };
        let source = ReportSource {
            date: "1977-05-22".into(),
            tracks: vec![track("Funiculi Funicula", ""), track("The Music Never Stopped", "Set 1")],
            ..Default::default()
        };
        let report = render(&source, Path::new("no-such.db"));
        assert!(report.contains("\n\nSet 1\n1. Funiculi Funicula"), "{report}");
        assert!(!report.contains("Tracks\n"));
    }
    #[test]
    fn long_dates_name_the_weekday() {
        assert_eq!(long_date("1977-05-22"), "Sunday, May 22, 1977");
        assert_eq!(long_date("1977-05-08"), "Sunday, May 8, 1977");
        assert_eq!(long_date("1995-07-09"), "Sunday, July 9, 1995");
        assert_eq!(long_date("1970-08"), "1970-08");
    }
    #[test]
    fn clock_and_audio_lines() {
        assert_eq!(clock(59.6), "1:00");
        assert_eq!(clock(3600.0), "1:00:00");
        assert_eq!(audio_line("flac", 96000, 24, 2), "FLAC, 96,000 Hz, 24-bit, 2 channels");
        assert_eq!(audio_line("", 0, 0, 0), "");
    }
}
