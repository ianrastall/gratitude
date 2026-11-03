# Validation 0.9.0

3 November 2026, Windows 11 x64. Scope: tour names on show cards.

## Source

Ian suggested naming each show's tour and supplied a research report on named Grateful Dead tour legs. The report names about twenty legs, mixes tours the band billed (Europe '72, Dylan & the Dead) with archival box-set titles (May 1977, Listen to the River), and is inconsistent in places (it gives 22 Europe '72 shows but lists 17). It was used to find sources rather than as data.

Its source for the 1972 tours, the CMU "Tour-by-Tour" pages, covers only 1972 to 1977 and 1990 to 1995. Jerrybase, already BerthaML's main source, keeps a tour table for every year. `berthaml/tools/collect_tours.py` captured its 139 Grateful Dead tours and each tour's event list, pinned with hashes as `berthaml/sources/jerrybase-tours.json.gz`. Jerrybase says some tours come from band documents and others are its own grouping, so the build derives `reports/tours.json` as that source's grouping, and the app says so in the reference text. 1,466 of 2,234 shows fall in a tour, none in two. Hometown runs and one-offs belong to no tour and show none. Tours of canceled events, such as Fall 1986 and Fall 1995, cover no show.

## Display

The tour reads as "Spring 1977 tour", or as Jerrybase names it when the name already says tour ("August 1970 Caravan of Love Tour"):

- show cards, on a line under the place
- the player heading, after the place
- the All shows list, first in each show's caption; that search also matches tour names
- the reference text, as "Spring 1977 tour, as Jerrybase groups its tours"
- the standard report, as "Tour: Spring 1977 tour"

The show database moves to version 2 with `tours` and `show_tours`; the app requires it.

## Checks

59 Rust domain tests passed (the fixture now holds a tour and checks its label and reference line). `test_compile_db` checks that the database holds every tour and mapping, and that Cornell is in "Spring 1977". `build_corpus.py` rebuilt the corpus with no errors. The workflow check now requires the packaged Cornell MKA's card to read "Spring 1977 tour". The development build, run against a scratch copy of Ian's data while his installed copy was open, showed "Spring 1977 tour" on both of his cards and "Sportatorium, Pembroke Pines, FL · Spring 1977 tour" under the player heading.
