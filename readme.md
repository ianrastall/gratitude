# Gratitude

A Windows player and organizer for Grateful Dead recordings already on your computer, covering shows through 9 August 1995. Version 0.5.0. Follows Jupiter style standard 1.1.

Gratitude works exclusively with local folders and Matroska audio (`.mka`). It does not search for online recordings, download, torrent, seed or stream online sources. The acquisition workflow, network client and bundled torrent engine have been removed. Existing downloaded files and partial downloads remain untouched.

## Use

Launch **Gratitude** from Start. Choose **Import a show...** and select an existing folder of audio files and show notes. SHN, FLAC, WAV, AIFF, MP3, Ogg, Opus and M4A are accepted. A show dated after 9 August 1995 is flagged and cannot be written out.

The Rust core reads audio recursively (up to four subfolder levels), proposes natural filename order, and parses the largest non-checksum TXT for date, venue, sets and songs. Select another TXT if needed. When TXT and audio counts differ, embedded audio titles are preferred to filenames and a warning remains visible. Whatever the source, the title is the song name alone: track numbers, disc codes, timings, footnote marks, performance remarks and segue arrows are removed. Review the titles, order, album, date and comment, check the review box, then choose **Create MKA...** or **Write tagged files...**.

## Tags

Both outputs carry exactly these tags and remove all others:

| Tag | Value |
| --- | --- |
| Title | Song title only |
| Album | `YYMMDD_region-city-venue`, for example `771227_ca-san-francisco-winterland` |
| Date | `YYYY-MM-DD` |
| Track | Running number across the whole show, padded to the digit count of the last number |
| Artist | Grateful Dead |
| Comment | Taper and equipment, read from the notes and editable |
| Genre | Psychedelic Rock |

**Write tagged files...** creates a new folder named after the album, with files such as `01 Bertha.flac` and a copy of the notes. Source files are not changed. FLAC, MP3, Ogg, Opus and M4A keep their format and audio stream. SHN, WAV and AIFF become FLAC at the original rate and depth. MP3 uses ID3v2.4; FLAC holds the same fields as Vorbis comments, its native tag format. Each file's decoded samples and tag names are checked before the folder appears.

Source badges and credits come from local notes/folder names. For a dated import, **Offline show reference** shows the date's show from the bundled BerthaML database: setlist, review status, roster or researched lineup, listed recordings and citations. The same show supplies the album's region, city and venue; when several shows share the date, the notes' venue picks one. The import, and each MKA in the Shows pane, is also linked to the listed recording it came from, found from the folder, notes and checksum file names, and each track to its setlist performance; see `docs/catalogue.md`. No reference lookup uses the network.

Copying compressed audio unchanged is the default. Optional FLAC encoding accepts supported lossless 16-bit/24-bit integer sources at their original rate and depth. Tracks must have matching codecs, sample rates, depths, sample formats and channel layouts. Export re-probes inputs and verifies output precision. No resampling, gap insertion, trimming, fades or crossfading is applied; original silence remains. Source files are never modified, and existing output is refused. This preservation policy does not control Windows/audio-device conversions during playback.

In an MKA, each source file becomes a chapter holding the song title and track number; splitting inside a file requires preparation outside Gratitude. A medley inside one file keeps a slash between songs. Original TXT bytes are attached as `show-notes.txt`. Matroska has no Shorten mapping, so SHN is stored as FLAC there too.

**Job** shows packaging progress and cancellation. Closing the main window cancels an active package; closing only the workshop leaves it running. Failed/cancelled packaging does not publish a partial MKA.

The local Shows pane displays colored source/modifier badges and credits. Clicking a show plays it. The top-right choice is **SBD first** or **MTX first**, with AUD/FM/UNKNOWN following both. The supplied category-specific tiers apply automatically; members of a tier tie. See `docs/recording-tiers.md`. Recent shows retain recency. Playback supports chapters, seeking, volume and saved positions. File > Settings controls library folder, skip interval, theme and accent. A red-family Windows accent is replaced once with a blue-teal so that red only ever means an error; Settings can restore it.

## Offline reference

The [BerthaML corpus](berthaml/README.md) in `berthaml/` is Gratitude's source of truth for shows: one hand-edited XML file per show, shared registries, recordings and track lists. Every build compiles it into the read-only `data/berthaml.db` that the app ships. The corpus is a draft; each reference states its record's review status. See [coverage](berthaml/reports/coverage.md) and `docs/catalogue.md`.

The database is not a current availability index or download service.

## Build and delivery

Requires Windows 11 24H2 x64, .NET 10, Rust, Python 3 (standard library only), Inno Setup 6, shared `../jupiter-ui`, and binaries described in `third_party/README.md`.

```powershell
pwsh build/build.ps1
```

Release runs formatting, Clippy, Rust/ABI tests, managed local-import/MKA/audio checks and WinUI style checks, then publishes a self-contained installer and supplemental portable zip under `dist/`. There is no bundled aria2 or HTTP client. Upgrades retain existing application identity and `%LOCALAPPDATA%\show-player` data. Uninstall does not remove user audio or app data.

Rust owns local inspection, TXT parsing, offline references from the BerthaML database, MKA creation, playback and resume state. WinUI 3/C# owns the interface through a protobuf C ABI. Playback infrastructure is adapted from Jupiter Audio Bookshelf. See `docs/project-brief.md` and `docs/validation-0.5.0.md`.
