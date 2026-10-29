# Validation — 0.1.0

29 October 2026, Windows 11 x64.

## Automated checks

- `cargo fmt --all --check` and Clippy across workspace/all targets with warnings denied: passed.
- 18 Rust domain tests and 3 C ABI tests: passed. Includes real Archive TXT parsing, natural order, set/encore handling, original-file selection, cancellation and rejection of concurrent jobs, playback store behavior and ABI error handling.
- Managed binding build and WinUI release/style check: passed with zero warnings/errors.
- `ShowPlayer.WorkflowCheck`: synthetic three-track show imports through the managed/native boundary, with natural order and expected artist/date/set/song metadata. Both stream-copy and FLAC modes produce three chapters and a 36-second show. Original TXT attachment matches byte for byte. Decoded PCM equals the concatenated input samples byte for byte. Existing output is refused. Generated MKA loads in the player and restores a saved 17-second position.
- `tests/torrent_webseed.py`: locally generated torrent downloads successfully through bundled aria2 and an HTTP webseed, with correct piece hash and exact bytes. The normal release command includes this check.

## External source checks

- etree listing 624186: current detail page resolves through the Rust resolver to a working `.torrent`; bundled aria2 reads 23 files. A timed transfer of just the notes found no reachable peers and remained incomplete. This is not recorded as a successful etree media transfer.
- Archive identifier `gd1993-09-22.140821.fob.Schoeps-MK41.miller.sirmick.flac1648`: current metadata passes availability validation and returns its current torrent plus original filenames. A selected TXT transfer completed through the external Archive webseed and passed torrent verification (approximately 1 MiB of piece data for the 5 KiB notes). The downloaded TXT is retained as a parser regression fixture; its appended diagnostic tables and fingerprints are ignored for track extraction.
- Archive identifier `gd1965-11-01.sbd.bershaw.5417.sbeok.shnf`: the Rust resolver refuses it as restricted/stream-only.
- No full concert download is claimed as verified. Torrent availability and throughput depend on current peers/webseeds.

## Desktop checks

Computer-use inspection verified the main player, the import workshop and recording search. Importing the synthetic folder through the native Windows picker shows three correctly titled tracks, sets, date and venue. A date search returns the expected saved recording entries.

The final self-contained installer built successfully (approximately 163 MiB; supplemental portable zip approximately 236 MiB). Silent per-user installation exited 0. `Show Player.lnk` is registered directly in the user's Start Menu and points to `%LOCALAPPDATA%\Programs\Show Player\ShowPlayer.exe`. Launching that shortcut produced the expected installed window, verified through computer use. The installed native DLL hash matches the published DLL. The installed workshop and shared Settings window were inspected, including the workshop in dark and light appearance. Dark was restored afterward. A registered uninstaller is present. Installer identity is distinct from Audio Bookshelf, and its file operations do not remove the app's data directory or user audio. Windows contrast-theme switching was not exercised.

## Known first-version limits

The online catalogue is a saved GD snapshot, not a live index. Other artists work through local folder import. One job runs at a time. Retry resumes/rechecks retained torrent files; the queue itself is not persisted. Sets are chapter-name prefixes, not nested Matroska chapters. Artwork embedding, single-file song splitting and automatic HTTP fallback remain future work. Mixed sample rates/channel counts require preparation before import. A checksum TXT or an unusual notes layout may need manual metadata/song review.
