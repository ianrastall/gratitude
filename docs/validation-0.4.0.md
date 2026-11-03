# Validation 0.4.0

2 November 2026, Windows 11 x64. Scope: the offline show reference moves from `data/gd-shows.xml` to `data/berthaml.db`, compiled from the BerthaML corpus. This is step 3 of the BerthaML plan in `berthaml/HANDOFF.md`.

## Changes

- `berthaml/tools/compile_db.py` compiles the corpus into SQLite with the Python standard library: 2,234 shows, 39,639 song performances and one tuning event, 410 songs, 557 venues, 14 people, 18,611 recordings and 654 original-file tracks, about 19 MB. `build/native.targets` runs it when any input changes; the database is a build output and is not committed.
- The import reference now shows the BerthaML show: venue and place, record status, derived corroboration, the setlist with segues, a researched lineup or the membership-period roster labelled as such, the number of listed recordings and citations. Jerrybase's private and cancelled events on the date follow.
- The album's place comes from the BerthaML venue. When shows on one date are at different venues, the notes' venue or city chooses; otherwise the notes' place stands, as it does for dates with no BerthaML show.
- Recording search reads the database's recordings, with unchanged ranking.
- Removed: `data/gd-shows.xml` (preserved byte for byte as `berthaml/sources/catalogue.xml.gz`, SHA-256 checked), the two acquisition snapshots, `tools/build_show_facts.py` and the harness's `--compile-catalogue`. The installer deletes the three old data files on upgrade.

## Comparison

`docs/berthaml-catalogue-comparison.md` covers all 2,300 dates the 0.3 catalogue placed: 2,132 album names unchanged, 26 changed, 17 dates with shows at several venues, 125 dates with no BerthaML show (all 124 recording-only dates with a venue, plus 1968-05-30). Of the 26 changes, 21 correct spelling or restore accented letters and 5 take a different venue or city from BerthaML. See `docs/catalogue.md`.

## Automated

`pwsh build/build.ps1` passed: formatting, Clippy with warnings denied, 44 Rust domain tests (new: notes venue choosing between same-date shows, Canadian album region, private-event reference, database search and ranking, refusal of another database version) and 3 ABI tests, then `ShowPlayer.WorkflowCheck`. The workflow check now requires the 1977-05-08 reference to begin with the Barton Hall venue line, state the draft record status and the roster label, and the album to be `770508_ny-ithaca-barton-hall-cornell-university`. All earlier audio, tag, SHN and playback checks passed.

`python -m unittest test_compile_db` (in `berthaml/tools`) compiles the real corpus and checks table counts against `reports/coverage.json`, foreign keys, the Winterland 1977-12-27 setlist, the 1970-05-02 researched lineup, recording type codes and album regions. `python tools/build_corpus.py --check` still reports that the offline rebuild matches.

## Delivery

`dist/Gratitude-0.4.0-win-x64-setup.exe` (163 MB) and the supplemental portable zip were built. A silent per-user install over 0.3.0 exited 0. Afterwards `{app}\data` held only `berthaml.db` (hash equal to the published copy) and `gdshowsdb-LICENSE.txt`; `%LOCALAPPDATA%\show-player` was still present; the Start Menu's `Gratitude.lnk` targets the installed `ShowPlayer.exe`; the registered uninstaller reports 0.4.0. Launching the Start shortcut opened a responsive window titled Gratitude, which was then closed.

Not checked: the installed Workshop's reference panel was not inspected visually, and no real show folder was imported in the installed app. The same core and database path are exercised by the workflow check.
