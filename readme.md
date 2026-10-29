# Gratitude

A Windows player and creator for concert recordings already on your computer. Version 0.2.2. Follows Jupiter style standard 1.1.

Gratitude works exclusively with local folders and Matroska audio (`.mka`). It does not search for online recordings, download, torrent, seed or stream online sources. The acquisition workflow, network client and bundled torrent engine have been removed. Existing downloaded files and partial downloads remain untouched.

## Use

Launch **Gratitude** from Start. Choose **Import a show...** and select an existing folder of audio files and show notes. Import accepts any artist.

The Rust core reads audio recursively (up to four subfolder levels), proposes natural filename order, and parses the largest non-checksum TXT for artist, date, venue, sets and songs. Select another TXT if needed. When TXT and audio counts differ, embedded audio titles are preferred to filenames and a warning remains visible. Review/edit metadata, titles, sets and order, then check the review box and choose **Create MKA...**.

Source badges and credits come from local notes/folder names. For a dated Grateful Dead import, **Offline show reference** shows the matching entry from the bundled 1967-1995 XML. The reference does not replace local notes, tags or proposed tracks. No reference lookup uses the network.

Copying compressed audio unchanged is the default. Optional FLAC encoding accepts supported lossless 16-bit/24-bit integer sources at their original rate and depth. Tracks must have matching codecs, sample rates, depths, sample formats and channel layouts. Export re-probes inputs and verifies output precision. No resampling, gap insertion, trimming, fades or crossfading is applied; original silence remains. Source files are never modified, and existing output is refused. This preservation policy does not control Windows/audio-device conversions during playback.

Sets appear in chapters, such as `Set 2 / Dark Star`. Segue arrows are stripped from display/exported song titles, including embedded tags; internal medley connectors become slashes. Original TXT bytes are attached as `show-notes.txt` and included in recording notes. Each source file becomes a chapter; splitting inside a file requires preparation outside Gratitude.

**MKA job** shows packaging progress and cancellation. Closing the main window cancels an active package; closing only the workshop leaves it running. Failed/cancelled packaging does not publish a partial MKA.

The local Shows pane displays colored source/modifier badges and credits. The top-right choice is **SBD first** or **MTX first**, with AUD/FM/UNKNOWN following both. The supplied category-specific tiers apply automatically; members of a tier tie. See `docs/recording-tiers.md`. Recent shows retain recency. Playback supports chapters, seeking, volume and saved positions. File > Settings controls library folder, skip interval, theme and accent.

## Offline reference

`data/gd-shows.xml` contains attributed GD show facts, setlists, historical recording references and separately marked credit-tier policy. It is not a current availability index or download service. Unknown and metadata-only dates remain labelled. See `docs/catalogue.md` for coverage, sources and the separate developer rebuild workflow.

## Build and delivery

Requires Windows 11 24H2 x64, .NET 10, Rust, Python, Inno Setup 6, shared `../jupiter-ui`, and binaries described in `third_party/README.md`.

```powershell
pwsh build/build.ps1
```

Release runs formatting, Clippy, Rust/ABI tests, managed local-import/MKA/audio checks and WinUI style checks, then publishes a self-contained installer and supplemental portable zip under `dist/`. There is no bundled aria2 or HTTP client. Upgrades retain existing application identity and `%LOCALAPPDATA%\show-player` data. Uninstall does not remove user audio or app data.

Rust owns local inspection, TXT parsing, offline references, MKA creation, playback and resume state. WinUI 3/C# owns the interface through a protobuf C ABI. Playback infrastructure is adapted from Jupiter Audio Bookshelf. See `docs/project-brief.md` and `docs/validation-0.2.2.md`.
