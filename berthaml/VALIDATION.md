# Corpus restructure validation

Validated on Windows on 30 October 2026 by comparing the working tree with the committed 0.1.0 corpus. This moved recordings out of the show files and made the show files hand-owned. It changed no fact.

- Shows: 2,234 of 2,234 have identical stage content, compared as canonical XML with recordings, their citations and the version attribute set aside.
- Recordings: 18,611 of 18,611 are identical and in the same order, now in 1,887 files. Three shows had an empty `<sources/>`, which is gone.
- Registries: identical apart from the version attribute.
- Playlists: 96 of 100 exports are byte-identical. The other 4, all `gd1970-05-02`, differ only in citation order, because the citation for its soundboard identifier moved with the recording.
- Reports: the same 5,067 issues, with the same IDs, kinds and subjects. Evidence hashes moved only for the 2,234 `show-review` and 4 `registry-review` issues, which hash files whose shape or version changed; no resolutions existed to go stale. Crosswalk, exclusions, release candidates, playlist coverage and unassigned recordings match. `coverage.json` is identical apart from two new fields.
- Drift: one row, `gd1970-05-02`, whose hand-researched song IDs differ from source titles such as Jordan for Cold Jordan.
- Exercised on real files: a hand edit to a show survived rebuilds and appeared as drift; `--propose` wrote the source's version outside `data/`; an edited generated file stopped the rebuild and was left in place.
- Regression suite: 24 tests pass. Python XSD/semantic validation has zero errors and warnings; .NET `System.Xml.Schema` passes all 4,125 XML files; the audit has zero errors.
- The data ZIP was unpacked outside the repository, where its builder and auditor pass offline.

# Data release 0.1.0 validation

Validated on Windows on 30 October 2026, using Python 3.12, lxml 6.1.3 and PyYAML 6.0.3. This is a draft research release; historical curation is incomplete.

## Results

- Prospective Python XSD and semantic validation: 2,234 shows and four registries, zero errors and zero warnings. Validation runs in memory before publishing, so a failed candidate cannot replace the corpus.
- Windows .NET `System.Xml.Schema`: all 2,238 XML files pass XSD validation.
- Offline rebuild comparison: every generated XML, playlist and report matches its owned-file manifest.
- Integrity audit: zero audit errors; all pinned sources and acquired playlist snapshots match their SHA-256 locks; all show identities and all 19,297 supplied recording references are accounted for.
- Regression suite: 19 tests pass. These cover fuzzy dates, ambiguous early/late identities, source order, comma-bearing titles, mode context, inferred classifications, preservation of local edits, prospective cross-file reference failures, original/derivative separation, fractional/invalid durations, repeated-song ambiguity, CD/LP boundaries, unsafe playlist paths and plain-text companion notes.
- The actual Cornell Dusborne and Seamons snapshots retain 22 and 21 original tracks respectively. Their source-specific boundaries are not replaced by the canonical show's song count.
- The data ZIP was unpacked into a separate workspace directory. Every packaged file checksum was verified. Its builder and auditor pass offline without the original `gd-shows.xml` or any acquisition HTML/metadata cache beside it.
- The original `D:/jupiter/gratitude/data/gd-shows.xml` remains byte-identical: SHA-256 `616f2ae3b45054351fdbb5cfa9c7df1d5013070aace9c89577d261d8c3a49089`.
- The historical completion gate correctly returns failure: show/registry reviews, 702 track mappings, the official edition inventory and 5,067 curation issues remain outstanding. Structural validity does not promote those records.

The first edition batch exports 34 playlists with 702 track entries: 32 circulating original-file playlists, the researched 1997 Dick's Picks 8 CD sequence and an unassigned-edition reference list. There are 32 M3U8 exports. Two of the 34 captured Archive identifiers cannot yet be assigned to an individual show in the working roster. Blank timings in the CD edition remain blank rather than borrowing the seed's digital timing observations.

The original importer and its data were preserved in a verified recovery ZIP before consolidation. Existing unambiguous show IDs are retained; unresolved identity candidates remain in the source snapshots and reports.

## Publication boundary

The data release contains schemas, XML, companion playlists, reports, research overrides, source snapshots, attribution and tools. It contains no audio or application binaries. Gratitude's GUI, installer and runtime catalogue are unchanged. Its repository documentation links this separate data product.

Run `python tools/release_dataset.py` for the normal validation and packaging path. The ZIP has deterministic entry timestamps and an internal SHA-256 manifest; its sibling `.zip.sha256` records the final archive hash. See `reports/coverage.md` for live counts and `CURATION.md` for the remaining work and completion rules.
