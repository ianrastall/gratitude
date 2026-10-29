# Validation — 0.1.1

Ian reported that `D:/music-orig/1977-12-27 Winterland SBD FLAC` imported labels rather than song names and did not recognize sets. The parser had not handled `101-d1t01 - Title` or hyphen-decorated set headings; it also used the introductory encoding note as the artist and could count a missing encore as a track.

The fix strips the compound label, recognizes decorated set/encore headings, skips the introductory encoding preamble, excludes explicit missing-song entries and reports them as warnings. Embedded FLAC/audio TITLE tags are used before filenames when TXT and audio counts differ. Original notes and audio are read without modification.

The complete supplied TXT is retained in `tests/fixtures/gd1977-12-27.24069.txt`. A Rust regression checks every song name, the artist/date/venue and set membership. Additional tag tests cover uppercase TITLE and stream tags. The managed workflow test exercises embedded-title fallback with a deliberately mismatched TXT as well as the existing packaging/audio-preservation checks.

The managed/native inspector was run against Ian's actual folder, not just a synthetic fixture. All 22 titles matched the TXT's song names, with 12 tracks in Set 1 and 10 tracks in Set 2. Artist: Grateful Dead; date: 1977-12-27; venue: Winterland Arena. There is exactly one warning: the TXT marks Samson & Delilah as missing, so no encore audio track is added.

The normal release build passed formatting, Clippy with warnings denied, 20 domain tests and 3 ABI tests, the managed/native packaging and exact audio/TXT preservation workflow, embedded-title fallback, and the local torrent/webseed check. WinUI release/style checks passed with zero warnings/errors. The 0.1.1 installer built successfully and upgraded the per-user installation with exit code 0. Hashes of existing `ui.json` and `bookshelf.db` remained unchanged across installation. The installed product version is 0.1.1, and it launches through the existing Start Menu shortcut.

The actual Winterland folder was then imported through the installed application's native folder picker. Computer-use inspection confirmed all 22 clean titles, Set 1 on the first 12 tracks, Set 2 on the final 10 tracks, the correct header fields and the missing-encore warning. The app was left open on this reviewed import for Ian; no MKA was created and no source media or TXT was modified.
