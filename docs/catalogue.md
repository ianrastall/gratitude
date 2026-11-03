# Offline show database

From 0.4.0 Gratitude's offline show reference is `data/berthaml.db`, a read-only SQLite database compiled from the [BerthaML corpus](../berthaml/README.md) at every build. It replaces `data/gd-shows.xml`, the 29 October 2026 XML catalogue used by 0.2 and 0.3. That file is preserved byte for byte as `berthaml/sources/catalogue.xml.gz`, where it remains one of the corpus's pinned sources.

The corpus is a draft. Most show records are imported and unreviewed, and the reference says so for each show. See [coverage](../berthaml/reports/coverage.md).

## What the app uses

- **Import reference.** For an imported folder's date, the Workshop's **Offline show reference** shows each BerthaML show on that date: venue and place, the record's review status, the derived corroboration level, the setlist with segues, a researched lineup or else the membership-period roster labelled as such, the number of listed circulating recordings and the show's citations. Canceled and private events that Jerrybase lists on the date follow, since a recording may come from one.
- **Album name.** The show's venue, city and region name the album as `YYMMDD_region-city-venue`. A US or Canadian region is its state or province code when the venue registry gives one; otherwise the country name, with British venues named by nation (England, Scotland) as 0.3 did. When several shows on the date are at different venues, the notes' venue (or failing that, city) picks one; if nothing distinguishes them, the album keeps the place parsed from the notes. Dates with no BerthaML show also keep the notes' place.
- **Recording search** (the `SearchRecordings` call, used by the workflow check) lists the 18,611 recording references assigned to shows, with their source type, modifiers, credits and lineage, ranked by the same SBD-first or MTX-first preference and credit tiers as before.

## Linking local shows

From 0.5.0 an imported folder, and every MKA in the Shows pane, is linked to a BerthaML show, the listed recording it came from when that can be told, and the setlist performance each track or chapter holds (`core/domain/src/linking.rs`). The Workshop shows the result beside the source badges.

The recording is found by the first rule that picks out a single tape:

1. **Identifier.** The folder, notes file or a checksum file (`.ffp`, `.md5`, `.st5`) is named after a listed Archive identifier or etree torrent name. `gd77-` and `gd1977-` count as the same.
2. **shnid.** A number in those names is a listed recording's shnid on that date. Four-digit years are ignored.
3. **Source name.** Those names match a listed recording's once the date and all numbers are set aside, as `gd77-10-30.mtx.dusborne.flac16` matches `gd1977-10-30.137374.mtx.dusborne.flac16`.
4. **Track files.** At least half the audio file names appear in a captured track list.
5. **Track lengths.** Every track's length is within 1.5 seconds of a captured track list of the same length.

Notes text is never searched for identifiers or shnids, because a matrix's notes list its component sources'. Archive.org and etree listings that share a shnid are one tape, named by the Archive listing. When nothing matches, the show is linked by date, or by date and the notes' venue when several shows share it; the recording stays unknown.

Each track's performances come from a matched track list's own links when the local files match it one for one, and otherwise from titles: the setlist is walked in order, so a song played twice is placed by its neighbours, and a slash or arrow in a title splits a medley. Titles compare past case, punctuation, a leading The, an ampersand and a dropped g. Tuning, banter and misspelt titles hold none.

Creating an MKA records what its source folder offered (names, file names, titles, lengths) in the resume store's `source` table, schema 3, since the MKA does not keep the folder or file names. The link itself is worked out again on every list, so it follows corpus changes. An MKA without a record, such as one made before 0.5, is linked from its title's date, its title and its chapters.

The harness prints what linking finds for a folder or MKA:

```powershell
cargo run -p show-player-harness --release -- --link <folder-or-mka> target/berthaml/berthaml.db
```

## Building it

```powershell
python berthaml/tools/compile_db.py target/berthaml/berthaml.db
```

The .NET build runs this through `build/native.targets` whenever a show file, registry, generated recording file or the compiler changes, and ships the result with `berthaml/sources/gdshowsdb-LICENSE.txt`. It needs only the Python standard library. The schema is `berthaml/tools/berthaml-db.sql`; its `user_version` is the database version, and the app refuses any other. Rust tests build a fixture database from the same schema file.

The database holds shows, sets, performances and non-song events, songs and title aliases, venues and alternative names, people, researched lineups, era rosters, excluded events, recordings with identifiers and original-file track lists with durations and candidate performance links, and per-show citations. `meta` records the dataset version, its as-of date, a SHA-256 over every compiled input and the attribution.

## Comparison with 0.3

[berthaml-catalogue-comparison.md](berthaml-catalogue-comparison.md) compares album names for every date the 0.3 catalogue placed. 2,132 are unchanged. 26 differ: 0.3 misspelled Oakland and University and lost accented letters (`caf-au-go-go` is now `café-au-go-go`), and BerthaML gives different venues or cities for five dates. 17 dates have shows at more than one venue, which 0.3 collapsed into one. All 124 of 0.3's recording-only dates with a venue, and one gdshowsdb date, are not BerthaML shows: 53 are private events and 6 cancelled ones in Jerrybase, and the rest are dates known only from recording metadata.

## Sources and limits

Show facts come from Jerrybase factual captures and gdshowsdb (MIT, Copyright Jeff Smith); recording references and track lists from Archive.org metadata and the etree snapshot. See `berthaml/sources/README.md`. Recording listings are historical references, not an availability index; Gratitude does not download recordings. Source agreement is not review. Only a person sets a record to reviewed or verified.

Personal preferences remain in `%LOCALAPPDATA%\show-player\recording-priorities.json`. Only SBD-first versus MTX-first is configurable; AUD, FM and UNKNOWN follow both. Credit tiers are the user-supplied SBD, AUD and MTX tables in `core/domain/src/catalogue.rs`, house policy rather than historical fact.
