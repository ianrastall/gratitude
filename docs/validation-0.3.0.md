# Validation 0.3.0

30 October 2026. Scope: Gratitude is now Grateful Dead only, through 9 August 1995. Song titles, tags, SHN import, show opening, the accent and the icon changed.

## Song titles

`song_title` is the one rule for titles from a TXT, an embedded tag, a file name or a manual edit. It removes doubled track numbers, disc and track codes, timings, footnote marks, performance remarks and segues. Rust tests cover 29 forms of one title, and parenthesised or numbered titles that must survive, such as `Mannish Boy (I'm A Man)` and `Riot In Cell Block #9`. None of the 403 song names in `data/gd-shows.xml` starts with a digit, so a leading number on a line is always a label. A second bare number stays only if it differs from the first (`03 500 Miles`).

## Tags

| Tag | Value |
| --- | --- |
| Title | Song title only |
| Album | `YYMMDD_region-city-venue`, from the offline catalogue when it knows the date, otherwise the notes |
| Date | `YYYY-MM-DD` |
| Track | Running number across the show, padded to the digit count of the last number |
| Artist | Grateful Dead |
| Comment | Taper and equipment from the notes |
| Genre | Psychedelic Rock |

**Write tagged files** creates a new folder named after the album. MP3 is written as ID3v2.4 with no ID3v1 or APE tag. FLAC holds the same fields as Vorbis comments, its native format; other players do not reliably read ID3v2 in FLAC. FFmpeg writes a FLAC comment as `DESCRIPTION`, so the field is renamed `COMMENT` in place, without touching audio. Before the folder is published, each file's decoded samples are compared with its source and its tag names are checked against the allowed list. Any mismatch leaves nothing behind.

The MKA holds the same fields. Song title and track number are on each chapter; sets no longer appear in chapter names, because the title is only the song. Its segment title is the album, so the Shows pane has a name to list. The original TXT remains attached as `show-notes.txt`.

Tag inspectors will also show two technical fields FFmpeg writes that cannot be turned off: on MKA, Matroska's required muxing application (`ENCODER`) and a per-stream `DURATION`; on MP3, the Xing header's encoder string, which keeps VBR seeking accurate and is not an ID3 frame. FLAC has neither.

## SHN

FFmpeg reports no duration for Shorten, which stopped every SHN import with "Unknown duration". Import now counts decoded samples. Matroska cannot hold Shorten, and Shorten cannot hold tags, so both outputs store SHN as FLAC at its original rate and depth. The check decodes SHN, MKA and tagged FLAC and requires identical samples.

No real SHN file was on this machine. `tests/make_shn.py` writes a valid version 2 file for the check; the first real SHN folder should still be tried.

## Other changes

- Clicking a show plays it. Previously the last show reopened paused at launch and a click on it did nothing, which looked like a failed open. Clicking another show always worked.
- A red-family Windows accent is replaced once with `#2E7D9A`. See `docs/jupiter-exceptions.md`.
- The icon is a generic rose drawn by `build/make_icon.ps1`. It uses no band artwork.
- The Job page's Activity log follows new output, and stops following while the person has scrolled up.
- Local imports now read the source type from the folder name first, because notes can mention another source in passing. `gd1977-10-01...sbd.betty.patched` had shown AUD.
- The parser was run read-only across the four shows in `D:/music/grateful-dead`. That found a matrix TXT whose `2 Source Matrix` line ended the setlist early; the TXT is now a fixture. Paths are returned without the `\\?\` prefix, which the folder picker refused.

## Automated

`pwsh build/build.ps1` runs formatting, Clippy, Rust tests, the ABI check and `tests/ShowPlayer.WorkflowCheck`, which now also covers exact tag sets on FLAC and MP3, old tag removal, unchanged sources, refusal of a replacement folder, refusal of a show after 9 August 1995, and SHN through both outputs.
