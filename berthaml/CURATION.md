# Curation contract

The project has two linked products: the historical show corpus and a playlist for each recording/release edition. A show describes what happened on stage. An edition describes how that performance was recorded, edited, divided and issued. One never substitutes for the other.

The first data release is a research baseline. A valid XML file, two agreeing databases or a generated companion TXT is not a verified historical record.

## Scope and completion

Performances run from 1 January 1967 through 9 July 1995. The working roster includes public performances and broadcasts, including events with only month precision. Private rehearsals/studio sessions and canceled appearances are inventoried outside the show corpus. Two gdshowsdb-only event candidates remain in the reconciliation queue, rather than being silently accepted or deleted from the evidence.

The edition inventory covers the official and circulating editions discovered in the pinned captures, as of 30 October 2026. New transfers and official editions can appear after that date even though the historical performances are finite. Later discoveries belong to a new version with a reviewed change report. The 317 official release-page candidates are a discovery list, not a complete inventory of every CD, LP, digital, bonus-disc or box-set edition.

Completion requires every event identity and edition in the agreed inventory to be accounted for; sourced show facts and source-specific track facts; reviewed registries and records; and no open curation issues. A fact may finish as **accepted unknown** after documented source research. Inventing a lineup, a missing song, a duration or a performance mapping does not close a gap.

`python tools/audit_corpus.py --require-complete` deliberately fails while those requirements remain unmet. Ordinary structural checks can pass while curation is incomplete.

## Where to work

- `data/` is generated, validated XML. Edit the corresponding source under `curated/`, then rebuild. A modified generated file is detected and preserved by refusing the rebuild.
- `curated/people.xml`, `songs.xml`, `venues.xml` and `releases.xml` contain researched registry overrides. Acquired song and venue entries are added during generation.
- `curated/shows/YYYY/<id>.xml` replaces the acquired show record. Preserve the ID and cited evidence; this supports reviewed personnel, guests, segues and source mappings without regeneration undoing them.
- `curated/identities.json` contains deliberate source-to-record ID overrides. Existing unambiguous show IDs were retained through `sources/legacy-identity-map.json`; a source suffix is never interpreted as early/late without evidence.
- `reports/review-queue.jsonl` contains stable issue IDs, evidence hashes, states and subjects. `reports/coverage.json` is the machine-readable progress summary.

Copy a generated show into the matching `curated/shows/` path before editing it. Acquisition never edits these overrides.

For a resolved issue, add an entry to `curated/resolutions.json` containing its `id`, current `evidence_sha256`, `state` (`resolved`, `accepted-unknown` or `excluded`), `reason`, `reviewed` date and `citations`. Stale evidence hashes fail the build. Review status remains a separate property of the XML record. The supplied schema defines `reviewed` as checked by a person and `verified` as checked against multiple independent sources; imports retain `draft` or `skeleton`.

## Recording and release playlists

A circulating `<recording>` is keyed to its actual provider identifier. Its `<playlist>` children preserve original-file variants. A track stores the source title and filename, disc/source number when supplied, and its measured duration. The running `n` is separate from `sourceNumber`. MP3 derivatives are not appended to an original FLAC/SHN playlist. Unknown ordering or timing remains unconfirmed.

An official `<edition id="...">` can contain its own `<track>` list, preserving different CD/LP/digital boundaries. A release-level track list is a reference list whose edition is unassigned; it is not automatically applied to every edition. This prevents digital timing observations from becoming asserted CD timings. The first researched example is the 24-track 1997 three-CD edition of Dick's Picks 8; its source did not supply durations, so those timings remain blank.

`covers` links a track to one or more canonical performances/events. It may be absent while research is pending. Automated exact-title matches are marked `mappingStatus="candidate"` and `certainty="probable"`; repeated ambiguous songs stay unmapped. A researched but unrecoverable mapping may be marked `unknown`, with cited reasoning in the curation record. A studio or otherwise inapplicable show mapping may be explicitly marked `not-applicable`. A tape cut does not become an on-stage stop.

`playlists/recording/` and `playlists/official/` contain generated JSON and human-readable TXT companion files. The TXT includes playlist identity, numbered tracks, source-specific timings, mapping status, available lineage/credits and citations. M3U8 is emitted only when all real relative filenames are known. Copy it beside its matching audio files to play it. An official track list with no filenames produces no invented playable paths. Original TXT bytes and source audio remain untouched.

## Research sequence

1. Reconcile the early ambiguous identities and 186 events whose setlists are currently absent. Compare source disagreements before promoting facts.
2. Resume original-file playlist metadata acquisition in bounded batches. Review uncertain ordering, damaged/mislabeled source dates, unmatched tracks and medley boundaries.
3. Work the official release candidates into specific edition records and track lists, including bonus discs, compilations, edits and box sets. Establish a reviewed inventory before claiming full release coverage.
4. Research personnel/guests, per-performance instruments/vocals, song identity/authorship and physical venue identities. Membership periods supply warnings, not automatic stage lineups.
5. Resolve remaining unknowns with evidence, review the registries and individual records, and run the completion gate before a canonical data release.

No scheduled background acquisition is installed. Fetching, review, rebuilding and publishing are explicit, resumable steps.
