# Validation — Gratitude 0.2.2

29 October 2026, Windows 11 x64.

## Local-only scope

- Removed online recording search/download screens, seeding controls, download commands and network resolution. The Rust HTTP/TLS dependency stack has been removed. Release tests no longer invoke a torrent test, and aria2 is excluded from publication and removed from the old installed program directory on upgrade.
- Reserved protobuf command 19 after removing DownloadRecording. Offline catalogue queries remain available to local reference features; they do not fetch web content. Developer metadata-acquisition scripts remain separate from the app.
- Kept distinct MKA entries, colored source/modifier badges, local recording credits, SBD/MTX preference and fixed category-specific credit tiers. Import review now also shows local source badges/credits and the matching GD offline show reference, without replacing the TXT, local tags or track proposals.

## Checks

- Formatting, Clippy with warnings denied, 28 domain tests and 3 ABI tests pass. WinUI release/style checks pass with zero warnings/errors.
- Managed/native workflow verifies local source labels, the offline Cornell reference, default source ranking, MTX preference/persistence, original notes, chapters, exact decoded audio and overwrite refusal. Playback/resume and source-format validation pass.
- Stream-copy and FLAC exports preserve every decoded sample at 44.1 kHz/16-bit, 48 kHz/24-bit and 96 kHz/24-bit. Mixed formats and stale review metadata are refused. No gap/fade/resampling filters were introduced.
- The real Winterland folder passes all 22 expected song titles, 12 Set 1 / 10 Set 2, date/venue/artist and missing-encore warning. The import reports SBD and an offline reference.
- Updated XML parses: 2,305 date groups and 19,297 historical recording references. Copy-specific types/credits and tier tables remain intact; availability wording now explicitly describes reference-only data.

## Installed delivery

The self-contained installer completed with exit 0. Start Menu launch shows Gratitude 0.2.2. Installed managed/native assemblies and XML match the publish hashes; aria2 is absent from both publication and the upgraded install. Existing UI settings, resume database and source preferences retained their exact hashes. The user's existing etree torrent and partial-download control file also retained their hashes.

Computer Use verified the simplified main screen and workshop (Import a show / MKA job / Settings), with online recording search absent. Selecting the real Winterland folder through the native picker displays all 22 tracks, clean names and sets, the SBD badge, original rate/depth/channel details, and the missing-encore warning. The offline reference expander displays Winterland's attributed setlist, including the separately referenced encore, without adding a missing local track. No output MKA was created during this read-only folder check. The app is left on the user's imported Winterland folder.
