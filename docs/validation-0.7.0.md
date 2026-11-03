# Validation 0.7.0

3 November 2026, Windows 11 x64. Scope: a standard report for every show, a clearer icon, and the Start menu icon.

## Standard report

Ian asked for show information in one standard format rather than each source's own notes. `core/domain/src/report.rs` renders every report in the same layout:

1. Artist, date, place (BerthaML's when linked, otherwise the notes'), album.
2. Source badges, credits, comment (taper and equipment), the listed recording it was linked to, audio format, length and track count. Empty fields are left out.
3. The tracks under set headings, numbered as the tags are, with lengths and segue marks from the setlist. A track goes under its first performance's set, else the set the notes gave it, else the set before it; tracks ahead of the first placed one join that first set.
4. Notes: setlist songs the recording lacks, tracks that match no setlist song, and how the show was linked.
5. The BerthaML record status, where the original notes are, and the Gratitude version.

The original notes are never replaced. An MKA attaches the report as `show-report.txt` after the unchanged `show-notes.txt`; a tagged folder holds `show-report.txt` beside the copied notes. The Workshop shows the report and rewrites it as titles, order, album, date and comment change; the report relinks from the final titles. The Shows pane shows the report for the open MKA, built from its link, chapters, tags and audio stream, and File > Export report... saves it. For MKAs from earlier versions, whose Comment tag held the whole notes, the report keeps only taper and equipment, and chapter titles lose the "Set 1 / " prefix those versions wrote (now part of `song_title`).

Real reports, through `abharness --report`:

- `gd1977-10-01.140578.sbd.betty...flac16` (folder): 22 tracks in two sets, recording linked by its etree name. It lists "Estimated Prophet" as not in the recording and track 16 "Esimated Prophet" as matching no song, which is the notes' misspelling to correct in the Workshop.
- `G:\GDL\770423-ma-springfield.mka` (Show Player 0.1): clean titles, three sets, and "Equipment: SBD -> Master Reel -> Dat -> CD" instead of the whole notes.
- `G:\GDL\770522-fl-pembroke.mka` (made by Ian with 0.6.0): 24 tracks; the opening "Funiculi Funicula" now sits in Set 1 rather than under its own heading.

## Icon

The dark tile hid the red rose on dark backgrounds. `build/make_icon.ps1` now draws a light tile with a darker rim; previews at 16 to 256 pixels on dark and light backgrounds show the rose clearly.

The Start menu kept showing Show Player's icon although Explorer's own cache already had the rose. Start caches icons by AppUserModelID, so the ID changes from `IanRastall.ShowPlayer` to `IanRastall.Gratitude` in the app and the shortcut, and the installer runs `ie4uinit -show` to refresh the per-user icon cache. A taskbar pin made under the old ID is re-pinned once.

## Automated

`pwsh build/build.ps1` passed: 57 Rust domain tests (new: report layout, an opening jam joining the first set, clock and audio lines, set-prefixed titles), the ABI tests, the WinUI style check and `ShowPlayer.WorkflowCheck`. The workflow check now dumps the MKA's second attachment and requires the standard report with the Barton Hall place, "Original notes: show-notes.txt, attached to this MKA" and the linked recording; requires `show-report.txt` in the tagged folder with the album and comment; and requires `RenderReport` and `BookReport` to give the expected reports.

## Delivery

The installer installed silently over 0.6.0. The new Start shortcut carries `IanRastall.Gratitude` and not the old ID, and Explorer gives it the new icon. Launched from Start, the app opened Ian's newest show and its Show report pane rendered. Saved positions for both of Ian's MKAs were unchanged. The Start menu's own display of the icon was not seen, because opening Start would take over Ian's screen.
