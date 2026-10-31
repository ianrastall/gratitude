# Corpus coverage

Updated 2026-10-30. **Curation is incomplete.**

The working roster is the public GD event list in the pinned Jerrybase capture. It includes public broadcasts and dates known only to the month. Canceled events, private sessions and recording-only dates are kept as evidence outside that roster.

| Measure | Count |
| --- | ---: |
| shows | 2,234 |
| performances | 39,639 |
| songs | 410 |
| venues | 557 |
| people | 14 |
| official releases | 1 |
| assigned recording references | 18,611 |
| unassigned recording references | 686 |
| open issues | 5,061 |

## Review status

- draft: 2,048
- skeleton: 186

Schema validity and source agreement do not constitute historical verification. The two show sources may share upstream research. Most personnel, guest appearances, vocalists, instruments, song authorship, release mappings and durations still need research. No duration is inferred from a show or setlist.

## Open work

- context-review: 27
- dangling-segue: 1
- event-identity: 17
- official-release-editions: 317
- playlist-mappings: 22
- playlist-order: 1
- playlist-review: 30
- recording-date-assignment: 413
- recording-playlists-missing: 1,887
- registry-review: 4
- setlist-disagreement: 79
- show-review: 2,234
- venue-disagreement: 29

Machine-readable evidence: coverage.json, review-queue.jsonl, source-crosswalk.jsonl, source-drift.jsonl, corroboration.jsonl, roster-eras.json, recordings-unassigned.jsonl and event-exclusions.jsonl in this directory.

## Corroboration

- disputed: 95
- no-setlist: 186
- recording-backed: 23
- single-source: 9
- sources-agree: 1,921

These levels are derived at every build and are not review. `sources-agree` means both setlist sources give the show file's song order, and those sources may share upstream research. `recording-backed` adds a track list reaching most of the show.

## Shows by year

| Year | Events |
| --- | ---: |
| 1967 | 141 |
| 1968 | 124 |
| 1969 | 149 |
| 1970 | 146 |
| 1971 | 81 |
| 1972 | 86 |
| 1973 | 72 |
| 1974 | 40 |
| 1975 | 4 |
| 1976 | 41 |
| 1977 | 60 |
| 1978 | 81 |
| 1979 | 75 |
| 1980 | 87 |
| 1981 | 83 |
| 1982 | 61 |
| 1983 | 66 |
| 1984 | 64 |
| 1985 | 71 |
| 1986 | 46 |
| 1987 | 85 |
| 1988 | 80 |
| 1989 | 73 |
| 1990 | 74 |
| 1991 | 77 |
| 1992 | 55 |
| 1993 | 81 |
| 1994 | 84 |
| 1995 | 47 |

## Edition-specific playlists

- exported: 34
- tracks: 702
- m3u files: 32
- official playlists: 2
- circulating playlists: 32
- recording editions missing: 18579
- known official editions missing: 1
- unreviewed track mappings: 702
- official release inventory complete: False

Official release-page candidates: 317. This discovery index is not an exhaustive physical/digital edition inventory.

TXT and JSON companion files are under playlists/. M3U8 files are emitted only when actual relative source filenames are known; copy the M3U8 alongside the matching source audio. No audio is acquired. Candidate performance mappings require review.
