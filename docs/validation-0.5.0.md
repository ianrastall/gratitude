# Validation 0.5.0

2 November 2026, Windows 11 x64. Scope: BerthaML step 4. Imports and MKAs are linked to BerthaML shows, recordings and setlist performances, and the resume store moves to schema 3. See `docs/catalogue.md` for the rules.

## Changes

- `core/domain/src/linking.rs`: the linker, its evidence from an import folder or an MKA, and the folder-name gathering.
- The import's Draft carries the show and recording IDs, a sentence saying what matched, and each track's performance IDs; the Workshop shows the sentence. Book list entries carry the same, per chapter.
- Store schema 3 adds `source`. A finished MKA job records its source folder's evidence through a second, short-lived connection; a failure is logged and does not undo the MKA.
- `matroska::read_chapters` reads chapter titles and lengths, so older MKAs link from their chapters.
- The catalogue now loads show performances with title keys, and each date's recordings with shnids and track lists.

## Automated

`pwsh build/build.ps1` passed. 53 Rust domain tests, including seven for linking (two- and four-digit year identifiers, shnid and source name from names only, a matrix's notes not linking it to its soundboard source, Archive and etree listings of one tape, track files and lengths, the in-order title walk with medleys and a repeated song, dropped-g titles, folder and checksum names), a schema 2 to 3 upgrade that keeps positions, and chapter reading. `ShowPlayer.WorkflowCheck` now requires the synthetic 1977-05-08 import to link `gd77-05-08.sbd.hicks.4982.sbeok.shnf` through a checksum file's name, and the packaged MKA's library entry to keep that link and three chapter entries, which only the recorded source evidence can supply. All earlier checks passed.

## Real folders

`dotnet run --project tests/ShowPlayer.WorkflowCheck -c Release -- --verify-links D:/music/grateful-dead`, read-only:

| Folder | Link | Tracks with performances |
| --- | --- | ---: |
| `gd77-04-23.sbd.miller.88401.sbeok.flac16` | Archive identifier, certain | 20 of 21 |
| `gd1977-10-01.140578.sbd.betty.patched...flac16` | etree torrent name, certain; named by its Archive listing `...flac1644` | 20 of 22 |
| `1977-10-30 Assembly Hall Dusborne Matrix` | source name of the notes file, probable: `gd1977-10-30.137374.mtx.dusborne.flac16` | 19 of 21 |
| `1977-05-04 The Palladium Hunter_s Matrix` | show by date; no name identifies the tape | 17 of 18 |

The unmatched tracks are tuning, "Tuning/Take A Step Back" and "Esimated Prophet", a misspelling in the 1977-10-01 notes. The Dusborne notes list the soundboard source's shnid 99838, which is not used. `G:\GDL\770423-ma-springfield.mka`, made by an earlier version, links to gd1977-04-23 by its title's date, with 20 of 21 chapters matched.

No local folder has a captured track list yet (34 recordings do), so the track-file and track-length rules are covered by tests only.

## Delivery

`dist/Gratitude-0.5.0-win-x64-setup.exe` (163 MB) installed silently over 0.4.0 with exit 0; the uninstaller reports 0.5.0. Launching the Start shortcut opened the app, which reopened the last played show. That run upgraded Ian's resume store to schema 3, adding `source`, with the saved position of `770423-ma-springfield.mka` unchanged at 2,112,473 ms. The window was then closed. The Workshop's link line was not inspected visually.
