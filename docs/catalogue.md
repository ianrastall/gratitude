# Offline show database

From 0.4.0 Gratitude's offline show reference is `data/berthaml.db`, a read-only SQLite database compiled from the [BerthaML corpus](../berthaml/README.md) at every build. It replaces `data/gd-shows.xml`, the 29 October 2026 XML catalogue used by 0.2 and 0.3. That file is preserved byte for byte as `berthaml/sources/catalogue.xml.gz`, where it remains one of the corpus's pinned sources.

The corpus is a draft. Most show records are imported and unreviewed, and the reference says so for each show. See [coverage](../berthaml/reports/coverage.md).

## What the app uses

- **Import reference.** For an imported folder's date, the Workshop's **Offline show reference** shows each BerthaML show on that date: venue and place, the record's review status, the derived corroboration level, the setlist with segues, a researched lineup or else the membership-period roster labelled as such, the number of listed circulating recordings and the show's citations. Canceled and private events that Jerrybase lists on the date follow, since a recording may come from one.
- **Album name.** The show's venue, city and region name the album as `YYMMDD_region-city-venue`. A US or Canadian region is its state or province code when the venue registry gives one; otherwise the country name, with British venues named by nation (England, Scotland) as 0.3 did. When several shows on the date are at different venues, the notes' venue (or failing that, city) picks one; if nothing distinguishes them, the album keeps the place parsed from the notes. Dates with no BerthaML show also keep the notes' place.
- **Recording search** (the `SearchRecordings` call, used by the workflow check) lists the 18,611 recording references assigned to shows, with their source type, modifiers, credits and lineage, ranked by the same SBD-first or MTX-first preference and credit tiers as before.

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
