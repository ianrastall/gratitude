# Grateful Dead catalogue, schema 1

`data/gd-shows.xml` is the single runtime show/recording catalogue. It is UTF-8 XML with one `gratefulDeadCatalogue` root, covering 1967–1995. This is an attributed snapshot, not an assertion that every historical date, setlist or source is settled.

The 29 October 2026 snapshot contains 2,305 distinct dates, 2,233 performance/setlist entries and 19,297 provider-specific recording listings. Of those dates, 2,176 have gdshowsdb performance entries; the remaining 129 occur only in recording metadata or linked listings. There are 531 unplaced source records (including dates outside the requested years). Recording counts do not imply distinct tape lineages or current download availability.

## XML structure

- `sources/source`: provenance identifiers, URLs, scope and acquisition-content SHA-256. gdshowsdb is pinned to its Git revision and its MIT licence accompanies the XML.
- `recordingTaxonomy`: type/modifier meanings and the supplied personal default order.
- `shows/show`: one date group with venue/location and `basis="show-database"` or `basis="recording-metadata"`.
- `performance`: gdshowsdb UUID, location and source reference. Multiple entries on one date remain separate; recordings are not assigned to a particular performance without evidence.
- `set`: source numbering, with `role="unspecified"`; the final set is not assumed to be an encore.
- `song`: clean name and a separate `segueToNext` boolean. Continuation arrows do not enter titles.
- `recording`: provider/key/title, primary `type`, space-separated modifiers, `inferredType` and provenance references. Nested `person` and `creditText` distinguish recording, transfer/mastering or unspecified roles; `credits` supplies display text. `sourceChain`, `lineage` and `evidence` retain supplied factual metadata. `reference` points to supporting material.
- `unplacedRecords/record`: original identifier/date text, provenance and reason; these do not become invented show dates.

SBD = soundboard; MTX = matrix; AUD = audience. FOB describes microphone location, FM broadcast, PRE-FM pre-broadcast, DSBD digital soundboard, ULTRAMATRIX lineage and DTS encoding. The primary type follows explicit source metadata before identifier inference. Unknown remains UNKNOWN. Source descriptions may themselves contain errors. Person aliases are conservative; David Cantor is not assumed to be Betty Cantor-Jackson, nor Miller Auditorium to be Charlie Miller.

Personal preferences live separately in `%LOCALAPPDATA%\show-player\recording-priorities.json`. As of 0.2.1, only SBD-first versus MTX-first is configurable; AUD, FM and UNKNOWN follow both. Legacy preferences are normalized on read, retaining the relative SBD/MTX choice. Credit tiers use the user-supplied SBD, AUD and MTX tables, with members of each tier tied and unlisted credits following the listed tiers. The XML also records those tables as policy, distinct from facts. Old people/modifier/people-first fields are compatibility fields, not user controls. Priority applies before the 500-result display limit. Library lists share these preferences; the Recent menu retains recency.

## Sources and limits

- [gdshowsdb](https://github.com/jefmsmit/gdshowsdb): MIT-licensed performance facts and setlists. The XML records the exact revision and links each year to that revision.
- [Archive Grateful Dead collection](https://archive.org/details/GratefulDead): public metadata for 18,327 indexed items in the requested years, supplemented by the supplied historical URL list. Recording listings are historical references only; the app does not download or check online availability.
- [Infinite Dead](https://infinitedead.net/about/): 359 public posts covering July and October. Only links and derived factual classifications/credits are used; review prose is not distributed.
- Ian's etree JSON and Archive URL list: historical listings; reported seed counts are capture-time values.
- John Brackett, *Live Dead* (Duke University Press), supplied as PDF: historical context. It is not used as an exhaustive show index; no book text is included.

## Rebuild

Python 3 with PyYAML fetches/caches public inputs; the Rust domain compiler owns classification and XML generation:

```powershell
python tools/build_show_facts.py
cargo run -p show-player-harness --release -- --compile-catalogue work/show-facts.json data/gd-shows.xml
```

Acquisition caches and intermediate facts stay under ignored `work/`. The ordinary app build uses the committed XML offline and never fetches sources. To refresh, use a fresh cache, review factual changes/counts, update capture dates and rebuild the XML. No PDF or review prose belongs in the runtime catalogue. Preferences, audio files and source tags are unaffected.
