# BerthaML data 0.2.0

An attributed Grateful Dead research dataset and XML vocabulary. The current dataset covers public events from 1967 through 9 July 1995, with one hand-edited file per show, four shared registries, one generated recordings file per show with known tapes, and edition-specific playlists for official and circulating recordings. The schemas can also describe the earlier Jerry Garcia era from 1965-05-05.

**Canonical curation is incomplete.** The first baseline has 2,234 show records and 39,639 performance entries. It retains uncertainty rather than filling gaps with invented facts. See the generated [coverage report](reports/coverage.md), the [curation contract](CURATION.md) and [source provenance](sources/README.md).

BerthaML is currently a separate data product inside Gratitude's repository. The installed player and its existing `../data/gd-shows.xml` are unchanged. No audio is acquired by the dataset tools. See [validation](VALIDATION.md) for the first release checks.

## Layout

```
schemas/
  berthaml.xsd    driver: validates any BerthaML file
  common.xsd      ID formats, dates, closed vocabularies, record/citation block
  show.xsd        <show>
  recordings.xsd  <recordings>
  people.xsd      <people>
  songs.xsd       <songs>
  venues.xsd      <venues>
  releases.xsd    <releases>
data/
  shows/YYYY/<show-id>.xml        hand-owned: what happened on stage
  people.xml                      hand-owned
  songs.xml                       hand-owned
  venues.xml                      hand-owned
  releases.xml                    hand-owned
  recordings/YYYY/<show-id>.xml   generated: tapes, track lists, timings
tools/
  validate.py
  requirements.txt
  build_corpus.py
  audit_corpus.py
  collect_playlists.py
curated/          hand-entered recordings, identity and review decisions
sources/          pinned evidence and original-import recovery archive
reports/          coverage, source drift, crosswalks and the finite issue queue
playlists/        source-specific TXT, JSON and local M3U8 exports
```

**The show files and registries are the source of truth.** Edit them directly; git is their history. The build reads them and never writes them. It generates `data/recordings/`, `playlists/` and `reports/`, and refuses to overwrite a generated file that was edited by hand. See [CURATION.md](CURATION.md).

Namespace: `urn:berthaml:1`. Language version 1.1: show files no longer hold recordings, recording facts are fields rather than notes, and IDs are frozen. Schemas are XSD 1.0, so they load in .NET `System.Xml.Schema`, libxml2/lxml and Xerces.

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

The ordinary rebuild is entirely offline and validates the show files with the prospective generated XML before publishing anything, so a broken hand edit stops the build and leaves the generated files as they were. `--check` compares a complete rebuild with every generated file. `audit_corpus.py --require-complete` is a separate historical-completion gate, expected to fail for this draft.

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

Once a capture is pinned, a rebuild writes `reports/source-drift.jsonl`: every place a pinned source states something a show file does not, including new events, songs and venues. Sources never change a show file, and a source's silence is never drift, so research that goes beyond a source stays quiet. To see the record a source implies, write it somewhere outside `data/`:

```powershell
python tools/build_corpus.py --propose ../work/berthaml-proposals
```

Copy what you accept into the show file by hand.

## Data release

`python tools/release_dataset.py` runs the offline rebuild comparison, audit, tests and Windows .NET schema validation, then creates `../dist/berthaml-data-<version>.zip` and its SHA-256 file. The package contains XML, playlists, schemas, research overrides, pinned factual sources, attribution and rebuild tools. It is a draft dataset release, not a new Gratitude application release or a claim that historical curation is finished.

## Design

### Registries instead of enumerations

People, songs, venues and releases are open-ended (hundreds of guests, ~500 songs), so they are not enumerated in the schema. They live in registry files, and show files refer to them by ID. ID formats are fixed by pattern:

| Kind | Pattern | Example |
| :---- | :---- | :---- |
| Person | `p-...` | `p-garcia` |
| Song | `s-...` | `s-dire-wolf` |
| Venue | `v-...` | `v-harpur-college`, `v-capitol-theatre-port-chester` |
| Release | `rel-...` | `rel-dicks-picks-08` |
| Show | `gdYYYY-MM-DD[-suffix]` | `gd1970-05-02`, `gd1969-02-11-early`, `gd1967-06-01-tompkins`, `gd1970-08-xx` |
| Recording | `r-` and a hash of provider and key | `r-59409a8caeb5abe038d9` |

XSD 1.0 cannot check references across files, so `tools/validate.py` does.

### IDs are permanent

Gratitude will store these IDs, so an issued ID never changes meaning and never silently disappears. `curated/id-ledger.json` lists every issued ID and every retirement, including the 712 renames made before the first freeze. A second show on a date takes `-early` or `-late` when a source says so, otherwise a word from its venue; the suffix makes no claim about order.

After adding a show, venue, song, person or release, run `python tools/issue_ids.py`. To rename or merge one, change the data, then record it so stored references can follow:

```powershell
python tools/issue_ids.py --retire s-old-title s-new-title
```

The validator refuses an issued ID that has gone with no retirement, and a retired ID that comes back into use. The audit refuses IDs that have not been issued.

Performance IDs such as `p01-03` are opaque, not positions. Never renumber them: give an inserted song a fresh ID such as `p01-03a`, because tracks and releases point at them.

A venue's `<sourceRef>` is how a source event finds its entry, as a show's `<cite>` of its event is how the build pairs the file with its source.

Instruments, set kinds, modes, transitions and record status are genuinely closed and are enumerated in `common.xsd`.

### Personnel inheritance

```
show <lineup>
  -> set <personnel> (replaces) or <personnelChanges> (delta)
    -> performance <personnel> (replaces) or <personnelChanges> (delta)
```

`<personnelChanges>` holds `<remove>`, `<add>` and `<change>`. `<change>` replaces that person's instrument list. Use `<personnel>` where the stage empties out, e.g. Drums.

The show's `<lineup>` is authoritative. An absent lineup means unresearched personnel, and nothing is written into a show file on its behalf.

Membership periods and their `<absence>` entries give a roster for any date. A roster is derived when needed, listed by era in `reports/roster-eras.json`, and must always be presented as roster-derived rather than as who played. It cannot show a member who missed a night, or a guest: Bruce Hornsby, for one, missed 22 shows inside his period.

### Durations belong to tracks, not performances

A duration is a measurement of a specific recording or release, and track boundaries differ between sources. One track often spans several performances (Good Lovin' > Drums > Good Lovin'). So durations sit on `<track>` elements, which list the performance IDs they cover:

- circulating recordings: `<recording>/<track>` in `data/recordings/`
- official releases: `<release>/<track>` in `releases.xml`, with `show` and `covers`
- specific official editions: `<release>/<edition>/<track>`
- specific circulating original-file variants: `<recording>/<playlist>/<track>` in `data/recordings/`

Tracks retain source titles and optional real filenames. `covers` can be absent while mapping is unresearched. Automated links carry `mappingStatus="candidate"`; they do not certify a historical mapping. Because `covers` crosses files, `tools/validate.py` resolves it rather than the schema.

### Sandwiches and reprises

A performance that resumes an earlier one sets `continues` to the earlier performance's ID. Named suites (That's It For The Other One, Weather Report Suite) are catalog entries of `kind="suite"`; their parts carry `<partOf>`.

### Transitions

`transition` on each performance is `segue`, `stop` or `unknown`. It describes what happened on stage. Tape cuts are recorded on recordings, not here.

### Recording facts

Everything a consumer needs is a field. `kind` is what the source states; `inferredKind` is a guess held apart from it. `modifiers` holds FOB, FM, PRE-FM, DSBD, ULTRAMATRIX and DTS. `<identifier>` gives the provider's key first, then any `shnid`, which is read from the collectors' dotted naming and so is `probable`, or `uncertain` when a name holds more than one. An etree key is a torrent number, not a shnid. `<credit>` keeps the source's combined `transfer-mastering` role rather than choosing one. `<personMatch>` is a name the catalogue recognised, not an attribution. Placeholders such as "See info file" and "Unknown" are not kept as credits or lineage; the original text stays in the pinned sources.

### Song titles

A song has one `<title>` and any number of `<altTitle>`. Matching ignores case, punctuation, a leading The, `&` for `and`, and a dropped g, so only real differences need an entry: New Minglewood Blues, Keep Your Day Job, GDTRFB. An alias is a claim that two titles are one song, so reprises and separate songs that share words stay separate.

### Uncertainty and provenance

- Each file has `<record status="skeleton|draft|reviewed|verified">` with `<cite>` entries. Only a person sets `reviewed` or `verified`.
- `reports/corroboration.jsonl` gives each show a derived level: `reviewed`, `recording-backed`, `sources-agree`, `single-source`, `disputed`, `differs-from-sources` or `no-setlist`. It is recalculated at every build rather than stored, so it cannot go stale when a file is edited, and it is never a substitute for review. The two setlist sources may share upstream research, so only `recording-backed` adds independent evidence.
- Most facts accept `certainty="certain|probable|uncertain"`.
- Dates accept year or year-month precision when the day is unknown.
- `<program coverage="complete|partial|none">` states how much of the setlist is known.

### Deliberately not stored

Values derivable from the data are left out so they cannot drift: set totals, set counts, play counts, first and last played dates, era names, day of week, chronological index, rosters and corroboration levels.

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
- each recordings file names an existing show and agrees with its file name
- song credits use either a person ID or a text name
- every issued ID still exists or has a retirement that leads to a current ID
