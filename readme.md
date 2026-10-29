# Show Player

A Windows player and creator for concert recordings packaged as Matroska audio (`.mka`). Version 0.2.0. Working name: Show Player.

0.2.0 adds a single attributed XML catalogue for 1967–1995, show setlists, prominent recording types/modifiers/credits, and personal recording priorities at the top right of the player and workshop. See `docs/catalogue.md` for coverage, structure and rebuilding.

0.1.2 removes segue arrows from song/chapter titles, including embedded tags and playback of existing MKAs. Export preserves the original sample rate and bit depth, rejects mismatched source formats and verifies output precision before saving. No gaps, fades or silence trimming are applied; existing source silence is retained.

0.1.1 fixes import of decorated set headings (`--Set 1--`), compound track labels (`101-d1t01 - Bertha ->`), introductory encoding notes and explicitly missing songs. Verified against Ian's 22-track 1977-12-27 Winterland folder. When the TXT song count differs from the audio count, embedded audio titles are used before filenames.

The bundled recording catalogue is Grateful Dead-specific. Local folder import accepts any artist; the file model does not assume GD song names or dates.

## Use

Launch **Show Player** from Start. Choose **Import a show…** to select a folder of audio files and show notes, or **Find recordings…** to search the saved GD listings.

Import reads audio files recursively (up to four subfolder levels), proposes a natural filename order, reads the largest non-checksum TXT and extracts artist, date, venue, sets and song names. Choose a different TXT if needed. Review the metadata, edit song titles and set labels, and move tracks up/down. Confirm the review box, then choose **Create MKA…**. Original audio is never changed. The output must have a new filename.

The default mode copies compressed audio without re-encoding. Uncheck **Copy compressed audio unchanged** to encode supported lossless 16-bit or 24-bit integer audio as FLAC at the original rate and depth. Tracks must have matching codecs, sample rates, bit depths, sample formats and channel layouts; otherwise packaging is refused. Shorten may require FLAC mode. Automatic conversion filters are disabled. Export re-probes the files and verifies the resulting MKA's rate/depth/channel count before saving. This file-preservation policy does not control conversions performed by Windows or the audio device during playback.

Sets are represented in chapter names, such as `Set 2 / Dark Star`. Continuation indicators such as `>`, `->` and `→` are removed from song names; an internal medley connector becomes a slash. This affects display/exported chapter names, not the source tags or notes. The original TXT is attached unchanged as `show-notes.txt`; its text also appears under **Recording notes** in the player. Each audio file becomes one chapter. A recording that needs cuts inside one file requires preparation outside this version.

**Find recordings** searches `data/gd-shows.xml`: show facts/setlists from gdshowsdb, Archive recording metadata, Infinite Dead source links and Ian's historical listings. Search a date such as `1993-09-22`, venue, song, person or source identifier. Up to 500 matches are shown after personal ranking; narrow the search or choose a provider to see other matches. Search results and seed counts are historical snapshots, not a live index. Metadata-only dates, unknown facts and inferred source classifications are labelled.

The top-right **Priorities** control saves source, people and modifier order per Windows user. Defaults are SBD, MTX, AUD, FM, UNKNOWN, then Betty, Miller. Choose whether source type or people rank first. FOB is a location modifier; FM can be a broadcast modifier or the primary type when no more specific type is known. Credits preserve recording versus transfer/mastering roles where supplied. Recent shows keep chronological order and also display source information.

Before a download, the app resolves the current etree torrent link or queries Archive metadata. Archive restricted/stream-only items are refused; downloadable items select original files rather than all derived encodings. Downloads run through bundled aria2, including HTTP web seeds where the torrent supplies them. The default is 30 minutes of seeding after download, with a 256 KiB/s upload cap. BitTorrent also uploads during download. Choose 0, 30 or 120 minutes before starting.

Only one download or packaging job runs at a time. **Job** shows tool activity and **Cancel job** stops it. Retry the same recording to resume/recheck retained download files. **Open result** imports a completed download or plays a completed MKA. Closing the main window cancels an active job. Closing just the workshop leaves it running. Original downloads stay separate from the MKA for seeding.

Playback supports chapter navigation, volume, seeking and saved positions. **File > Settings** controls the MKA library folder, skip interval, theme and Windows/custom accent.

## Build

Windows 11 24H2 x64, .NET SDK 10.0.113, Rust 1.99.1, Inno Setup 6 and Python 3.12+. Shared `../jupiter-ui` is required. See `third_party/README.md` for the bundled playback and process tools.

```powershell
pwsh build/build.ps1
```

The normal release runs formatting, Clippy, Rust tests, managed/native workflow verification and a local torrent/webseed check, publishes self-contained runtimes, and creates an installer plus supplemental portable zip under `dist/`. Installers use a distinct Show Player identity and preserve `%LOCALAPPDATA%\show-player` and all user audio on upgrade/uninstall.

## Structure

| Location | Responsibility |
| --- | --- |
| `core/domain` | Rust playback/resume, catalogue, TXT import, metadata and cancellable tool jobs |
| `core/ffi`, `proto` | Rust-owned protobuf C ABI and generated message schema |
| `src/ShowPlayer.Interop` | Managed native lifetime and typed operations |
| `src/ShowPlayer.App` | WinUI 3/Jupiter.UI player and workshop |
| `data` | Runtime XML catalogue, source licence and historical rebuild inputs |
| `tests` | Core/ABI, packaging/audio preservation, playback/resume and webseed verification |

Playback and installer infrastructure is adapted from Ian's Jupiter Audio Bookshelf project; Show Player has separate application, installation and data identities. See `docs/project-brief.md`, `docs/jupiter-exceptions.md` and `docs/validation.md`.
