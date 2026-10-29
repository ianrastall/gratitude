# Show Player handoff

Active root: `D:/jupiter/show-player`. Do not work from D:/GitHub backups. Inherit `D:/jupiter/AGENTS.md` and shared standards. Working name: Show Player. Initial version: 0.1.0.

Read `readme.md`, `docs/project-brief.md` and `docs/validation.md`. Run `pwsh build/build.ps1` for normal release verification and installer creation. Close the development executable before rebuilding because Windows locks its EXE and native DLL.

The initial catalogue snapshots came from `D:/all/gd_torrents.json` and `D:/all/grateful_dead_torrent_urls.txt`, copied into `data/`. They contain 1,373 etree records and 17,548 Archive URL lines, 15,204 unique URLs. Catalogue search deduplicates Archive identifiers and normalizes gdYY-MM-DD aliases. Do not treat saved seed counts as live, or the etree snapshot's `size` strings as trustworthy byte counts.

Core additions are in `core/domain/src/shows.rs`, typed messages in `proto/show_player.proto`, dispatch in `core/ffi/src/lib.rs`, mappings in `core/ffi/src/convert.rs`, and managed facade in `src/ShowPlayer.Interop/AudioCore.cs`. `WorkshopWindow` implements import, recording discovery and job views. Existing player code is adapted from Audio Bookshelf, with a separate DLL/name/data/installer identity and MKA-only library.

Local CLI helpers: `cargo run -p show-player-harness -- --inspect-show <folder>`, `--resolve-etree <torrent-id>`, `--resolve-archive <identifier>`. Resolver helpers retrieve metadata only. The original interactive playback test protocol remains available.

Pending work is explicit in the brief. No full concert transfer is claimed as verified until it actually completes. Do not label the historical catalogue a live search. Preserve original audio and notes when creating MKA files.
