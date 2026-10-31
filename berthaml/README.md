# BerthaML data 0.1.0

An attributed Grateful Dead research dataset and XML vocabulary. The current dataset covers public events from 1967 through 9 July 1995, with one file per show, four shared registries and edition-specific playlists for official and circulating recordings. The schemas can also describe the earlier Jerry Garcia era from 1965-05-05.

**Canonical curation is incomplete.** The first baseline has 2,234 show records and 39,639 performance entries. It retains uncertainty rather than filling gaps with invented facts. See the generated [coverage report](reports/coverage.md), the [curation contract](CURATION.md) and [source provenance](sources/README.md).

BerthaML is currently a separate data product inside Gratitude's repository. The installed player and its existing `../data/gd-shows.xml` are unchanged. No audio is acquired by the dataset tools. See [validation](VALIDATION.md) for the first release checks.

## Layout

```
schemas/
  berthaml.xsd    driver: validates any BerthaML file
  common.xsd      ID formats, dates, closed vocabularies, record/citation block
  show.xsd        <show>
  people.xsd      <people>
  songs.xsd       <songs>
  venues.xsd      <venues>
  releases.xsd    <releases>
data/
  people.xml
  songs.xml
  venues.xml
  releases.xml
  shows/YYYY/<show-id>.xml
tools/
  validate.py
  requirements.txt
  build_corpus.py
  audit_corpus.py
  collect_playlists.py
curated/          research overrides; never overwritten by acquisition
sources/          pinned evidence and original-import recovery archive
reports/          coverage, source crosswalks and the finite issue queue
playlists/        source-specific TXT, JSON and local M3U8 exports
```

Namespace: `urn:berthaml:1`. Schemas are XSD 1.0, so they load in .NET `System.Xml.Schema`, libxml2/lxml and Xerces.

## Setup

From `D:/jupiter/gratitude/berthaml` in PowerShell:

```powershell
python -m pip install -r tools/requirements.txt
```

## Validate

```powershell
python tools/build_corpus.py
python tools/build_corpus.py --check
python tools/audit_corpus.py
python tools/validate.py data --strict
pwsh -NoProfile -File tools/validate-dotnet.ps1
python -m unittest discover -s tools -p 'test_*.py'
```

Add `--strict` to fail on warnings. Output lines are `ERROR file:line: message` or `WARN  file:line: message`, followed by a summary. Exit code 0 means no errors.

The ordinary rebuild is entirely offline and validates prospective XML before publishing it. `--check` compares a complete rebuild with every owned output. Local edits to generated files cause a refusal; put researched changes in `curated/`. `audit_corpus.py --require-complete` is a separate historical-completion gate, expected to fail for this draft.

## Grow the playlist dataset

The supplied catalogue accounts for 19,297 historical recording references. Only the first metadata batch has actual track lists so far. The official release discovery index has 317 live-release page candidates, whose specific CD/LP/digital editions still need research. See `reports/playlist-coverage.jsonl` and `reports/official-release-candidates.jsonl`.

Acquire another bounded batch, retain the newly captured evidence, then rebuild:

```powershell
python tools/collect_playlists.py --fetch --limit 100
python tools/capture_catalogue.py
python tools/build_corpus.py
```

Successful metadata captures are reused. Interrupted runs resume without redoing completed items; failed items remain retryable. Use repeatable `--identifier` options to research specific Archive recordings. Fetching is serial and paced; only metadata is requested. Acquisition never edits canonical XML or curated files.

TXT companion files include actual source track boundaries, numbering, available timings, lineage/credits and citations. JSON carries the same structured facts. M3U8 is emitted only for known relative filenames and must be copied beside the corresponding audio. Official playlists without filenames produce TXT/JSON only. The original recording notes and audio are preserved. Full edition and mapping rules are in [CURATION.md](CURATION.md).

## Check for source changes

Capture a candidate into a fresh cache and compare its factual changes before replacing pinned inputs:

```powershell
python tools/collect_index.py --fetch --cache ../work/berthaml-refresh-index --output ../work/berthaml-refresh-index.json
python tools/collect_jerrybase_facts.py --cache ../work/berthaml-refresh-index --index ../work/berthaml-refresh-index.json --output ../work/berthaml-refresh-facts.json.gz
python tools/compare_sources.py ../work/berthaml-refresh-facts.json.gz --output ../work/berthaml-refresh-diff.json
```

This reports added, removed and changed facts without modifying the corpus. No scheduled monitoring is installed.

## Data release

`python tools/release_dataset.py` runs the offline rebuild comparison, audit, tests and Windows .NET schema validation, then creates `../dist/berthaml-data-0.1.0.zip` and its SHA-256 file. The package contains XML, playlists, schemas, research overrides, pinned factual sources, attribution and rebuild tools. It is a draft dataset release, not a new Gratitude application release or a claim that historical curation is finished.

## Design

### Registries instead of enumerations

People, songs, venues and releases are open-ended (hundreds of guests, ~500 songs), so they are not enumerated in the schema. They live in registry files, and show files refer to them by ID. ID formats are fixed by pattern:

| Kind | Pattern | Example |
| :---- | :---- | :---- |
| Person | `p-...` | `p-garcia` |
| Song | `s-...` | `s-dire-wolf` |
| Venue | `v-...` | `v-harpur-college` |
| Release | `rel-...` | `rel-dicks-picks-08` |
| Show | `gdYYYY-MM-DD[-suffix]` | `gd1970-05-02`, `gd1966-xx-xx` |

XSD 1.0 cannot check references across files, so `tools/validate.py` does.

Instruments, set kinds, modes, transitions and record status are genuinely closed and are enumerated in `common.xsd`.

### Personnel inheritance

```
show <lineup>
  -> set <personnel> (replaces) or <personnelChanges> (delta)
    -> performance <personnel> (replaces) or <personnelChanges> (delta)
```

`<personnelChanges>` holds `<remove>`, `<add>` and `<change>`. `<change>` replaces that person's instrument list. Use `<personnel>` where the stage empties out, e.g. Drums.

The show's `<lineup>` is authoritative. An absent lineup means unresearched personnel. This importer does not generate on-stage lineups from membership periods; those periods support warnings and research only.

### Durations belong to tracks, not performances

A duration is a measurement of a specific recording or release, and track boundaries differ between sources. One track often spans several performances (Good Lovin' > Drums > Good Lovin'). So durations sit on `<track>` elements, which list the performance IDs they cover:

- circulating recordings: `<recording>/<track>` inside the show file
- official releases: `<release>/<track>` in `releases.xml`, with `show` and `covers`
- specific official editions: `<release>/<edition>/<track>`
- specific circulating original-file variants: `<recording>/<playlist>/<track>`

Tracks retain source titles and optional real filenames. `covers` can be absent while mapping is unresearched. Automated links carry `mappingStatus="candidate"`; they do not certify a historical mapping.

### Sandwiches and reprises

A performance that resumes an earlier one sets `continues` to the earlier performance's ID. Named suites (That's It For The Other One, Weather Report Suite) are catalog entries of `kind="suite"`; their parts carry `<partOf>`.

### Transitions

`transition` on each performance is `segue`, `stop` or `unknown`. It describes what happened on stage. Tape cuts are recorded on recordings, not here.

### Uncertainty and provenance

- Each file has `<record status="skeleton|draft|reviewed|verified">` with `<cite>` entries.
- Most facts accept `certainty="certain|probable|uncertain"`.
- Dates accept year or year-month precision when the day is unknown.
- `<program coverage="complete|partial|none">` states how much of the setlist is known.

### Deliberately not stored

Values derivable from the data are left out so they cannot drift: set totals, set counts, play counts, first and last played dates, era names, day of week, chronological index.

## Validator checks

Schema validation, then:

- references to people, songs, venues, releases and shows resolve
- show ID agrees with the date and the file name; no duplicate IDs
- two shows on one date both have `<slot>` and an ID suffix
- sets ordered soundcheck, set, encore; numbered 1..n within each kind
- `mode="mixed"` sets state a mode on every performance
- personnel changes are consistent with what is inherited
- `continues` targets an earlier performance of the same song
- lead vocalists are on stage (warning)
- lineup members fall inside a membership period, and active members missing from a lineup are flagged (warnings)
- release and recording tracks cover existing performance or event IDs
- song credits use either a person ID or a text name
