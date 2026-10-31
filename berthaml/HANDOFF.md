# HANDOFF

## Hand-owned corpus, 30 October 2026

Ian made BerthaML the next phase of Gratitude: the corpus is to become the source of truth the app lacks. This is the first of five steps, and it changes where things live, not what the data says.

`data/shows/` and the four registries are now hand-owned; `tools/build_corpus.py` reads them and generates `data/recordings/`, `playlists/` and `reports/`. Recordings moved out of the show files, which had been 71% recording data. Sources no longer flow into show files: `reports/source-drift.jsonl` and `--propose` report differences for a person to weigh. `curated/shows/` and the curated registries are gone, folded into `data/`. Language version is 1.1. `imported_show` is the old importer, kept for comparing and for drafting new events.

The split was verified against the committed 0.1.0 corpus: all 2,234 shows have identical stage content and all 18,611 recordings are unchanged. See [VALIDATION.md](VALIDATION.md). The only drift row is `gd1970-05-02`, and it is title aliases (Jordan for Cold Jordan), not a disagreement.

Remaining steps:

1. Done: split recordings out and hand over the show files.
2. Schema revision: turn recording notes into real fields (transfer credits, modifiers, shnid, inferred kind), add the song alias table, add a machine-checked `corroborated` status, and freeze IDs. Freezing must happen before the app stores any ID: 114 show IDs carry source hash suffixes, venues use `v-jb-N`, and performance IDs are positional.
3. Compile the XML into a read-only `berthaml.db` shipped with Gratitude, replacing `data/gd-shows.xml`. Compare album names and the 129 recording-only dates first.
4. Link local folders to shows and recordings: identifier, shnid, then file names and durations against track lists. Needs a store schema migration that preserves saved positions.
5. New UI against the Jupiter style standard: show browser, setlists in playback, every owned performance of a song.

Decided unless Ian objects: era rosters are added with a label saying they are roster-derived, reversing the current no-derived-lineups rule, and 1965-66 is a later capture. Before the roughly 18,000 Archive track lists are fetched, stop committing the TXT/JSON/M3U8 exports, which would reach about 54,000 files, and store the captures in a few bundled archives rather than one file each. File count is what costs here: `git add` of this 4,150-file change took about 30 minutes on Ian's hard disk, roughly 120 to 300 files a minute, despite being only about 30 MB. `sources/recording-playlists/index.json` and `cache_name` assume one file per capture and will need changing.

## Dataset implementation, 30 October 2026

The active implementation is now `tools/build_corpus.py`, using the finished schemas with backward-compatible playlist extensions. It publishes the consolidated `data/` corpus, `reports/` and source-specific `playlists/` from pinned factual snapshots and `curated/` overrides. The initial 2,236-record importer, its curation files and its original documentation are retained in the hash-verified `sources/pre-consolidation.zip`; its superseded scripts are no longer active.

The working public-event roster has 2,234 shows and 39,639 performance entries. The supplied Gratitude XML remains unchanged. Individual show IDs already linked to unambiguous source URLs were preserved. Two legacy-only event candidates and ambiguous early/late associations remain in the reconciliation evidence instead of being silently decided. Imported records remain drafts/skeletons.

Ian explicitly included both official and circulating recording editions. Each edition/recording can preserve its own track order, disc/file boundaries, measured timings, lineage and credits. `playlists/` exports human-readable TXT and structured JSON; known relative audio filenames also produce M3U8. The first Archive batch captured 34 source identifiers, with 32 assignable to the working show roster. Official coverage starts with the researched 1997 Dick's Picks 8 CD track sequence and the preserved unassigned-edition reference list. See the live generated coverage report for exact counts.

`CURATION.md` defines how to make researched overrides, close evidence-bound issues and qualify accepted unknowns. Completion is explicitly false. The next substantive work is source playlist acquisition, edition research across 317 release-page candidates, early-show reconciliation, then personnel/song/venue and mapping review. No automatic monitoring or background fetch job is installed.

Normal commands, acquisition/resume, source comparison and the data-release command are documented in `README.md`. Python semantic/XSD validation and Windows .NET XSD validation are both supported. This changes the data product only; Gratitude's installed application, installer and runtime catalogue are unchanged.

The sections below are the original schema handoff retained for historical context. Its "Current state" and "Next recommended action" predate the populated dataset above.

## Current state

BerthaML schema set, seed registries, one sample show and a validator. The sample data validates with no errors or warnings. The validator catches all 24 injected faults in the negative test run.

The source was a Gemini draft (single `berthaml.xsd` plus an example for 1970-05-02). It was rebuilt rather than patched; see Decisions.

## Decisions

1. Registries instead of `xs:enumeration` for people, songs, venues, releases. The draft enumerated players in the schema, which means editing the schema for every new guest.
2. One namespace, `urn:berthaml:1`, with one module per root element and a driver schema. The draft used `http://gratefuldead.org/...`, a domain not under the project's control. The URN works as an identifier but is not a registered URN namespace; switch to a URI on a domain you own if the project is published.
3. XSD 1.0, not 1.1, so the schemas work in .NET. Co-constraints and cross-file references are in `tools/validate.py`.
4. Durations moved from performances to tracks on specific recordings or releases.
5. Transition values reduced to `segue|stop|unknown`. The draft's `tape-cut` mixed recording facts into performance facts, and its `segue` versus `into` distinction was not defined consistently.
6. Instruments are physical instruments only. The draft had `rhythm-guitar`, `lead-vocals` and `backing-vocals` as instruments. Lead vocals are a per-performance `<lead>` element with catalog defaults.
7. Personnel inheritance across show, set and performance. The draft only overrode per song, so its acoustic set reverted to the full electric lineup after the first song.
8. Derived values removed (date parts, day of week, set totals, set counts, play counts, first/last played, era, chronological index).
9. `certainty`, record `status` and `<cite>` added, because much early-era data is uncertain.

## Corrections to the draft's data

- Its 1970-05-02 example was largely invented: Dawson guesting on Friend Of The Devil and Garcia on pedal steel during Dire Wolf are not supported by the sources checked; the acoustic set was 11 songs, not 4; set 2 continued well past Cosmic Charlie; there was a third set. The sample show here is rebuilt from cited sources.
- Its era table put Donna Godchaux's start at March 1972. Seed data here uses 1971-12-31.

## Commands run

```
python3 tools/validate.py data            -> 0 errors, 0 warnings, exit 0
python3 tools/validate.py data --strict   -> exit 0
negative test script (24 single-fault mutations of data/) -> 24 caught
```

Run on Linux, Python 3.13, lxml 6.1.3. Not yet run on Windows or against .NET `System.Xml.Schema`.

## Files

- `schemas/common.xsd`, `show.xsd`, `people.xsd`, `songs.xsd`, `venues.xsd`, `releases.xsd`, `berthaml.xsd`
- `data/people.xml`, `songs.xml`, `venues.xml`, `releases.xml`, `shows/1970/gd1970-05-02.xml`
- `tools/validate.py`, `tools/requirements.txt`
- `README.md`, `HANDOFF.md`

## Unresolved

- Membership dates in `people.xml` are seed values from general knowledge, not checked against a source. Lesh's start (1965-06), Constanten's dates and Hornsby's dates are marked `probable`.
- Default lead vocalists in `songs.xml` are seed values.
- Sample show: acoustic set personnel beyond Garcia and Weir is unresearched; which NRPS members joined Cold Jordan is unresearched; Garcia's pedal steel with NRPS is marked `probable`. Two release track mappings are inferred and marked `probable`. Durations for some tracks are missing.
- The segue after Candyman comes from two sources, but one circulating track list shows a tuning track after it. Unresolved.
- The validator does not yet check that every performance has a lead vocalist or catalog default, nor that teases name a different song from the one being played.
- The set-kind ordering rule ignores `kind="other"`.

## Next recommended action

Write the skeleton generator: given a list of show dates and venues, emit `status="skeleton"` show files with lineups pre-filled from membership periods. Before that, verify `people.xml` membership dates against a cited source, since every skeleton inherits them.
