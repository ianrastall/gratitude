# Validation — 0.2.0

29 October 2026, Windows 11 x64.

## Catalogue

- XML parses in both Rust roxmltree and Python ElementTree: 2,305 date groups, 2,233 performance entries, 19,297 recordings, all 29 years represented. 2,176 dates have performance entries and 129 are explicitly metadata-only. 531 source records remain unplaced.
- Recording classification totals: 6,343 SBD, 1,989 MTX, 7,318 AUD, 224 FM, 3,423 UNKNOWN. These counts describe listings, not unique tape lineages or available downloads.
- Source revision/content hashes, per-year setlist references, per-record provenance and gdshowsdb's MIT licence accompany the catalogue. Infinite Dead: all 359 public posts scanned for source links; review prose and PDF/book text are excluded.
- Winterland 1977-12-27 setlist begins Bertha, Good Lovin', Brown Eyed Women, Cassidy; no continuation markers enter song names. Segue booleans remain separate.

## Automated checks

- Formatting, Clippy with warnings denied, 28 domain tests and 3 ABI tests pass.
- Managed/native workflow verifies XML setlist search, default SBD priority, changing to AUD first, and persisted preferences reopened through a second core instance. Original priorities restored afterward.
- Generated MKA notes supply the library's SBD label. Original TXT attachment and decoded PCM remain exact in copy and FLAC modes.
- 44.1 kHz/16-bit, 48 kHz/24-bit and 96 kHz/24-bit exports preserve rate, depth and every decoded sample across joins. Mixed formats and stale review metadata are refused. Playback/resume and the local torrent webseed check pass.
- Classification tests distinguish matrix/broadcast/location; explicit source metadata wins over filename hints. David Cantor and Miller Auditorium do not become Betty/Charlie credits. Invalid preferences and XML DTDs are rejected.

## Desktop delivery

The normal release command completed with zero build warnings/errors and produced the 164.3 MiB installer and supplemental portable zip. The release UI was inspected through Computer Use: top-right priorities in both windows, Cornell search with prominent SBD/MTX/AUD badges and recording/transfer credits, an audience-first change immediately reordering results, and the attributed show/setlist expander. Defaults were restored and their persisted file checked.

The first silent upgrade stopped because the release preview was still open. After closing the preview windows normally, installation exited 0. The installed executable reports 0.2.0; its XML hash matches the published catalogue. The existing `ui.json` and `bookshelf.db` hashes were unchanged. The Start Menu shortcut points to the per-user installed executable, and launching it produced the expected installed window and restored default priority caption. Dark appearance was retained.
