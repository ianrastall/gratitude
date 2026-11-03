# Validation 0.6.0

2 November 2026, Windows 11 x64. Scope: BerthaML step 5. The main window becomes a suite workspace with Player, All shows and Songs; the player lays chapters on the show's setlist.

## Changes

- **Navigation** (standard section 7, suite workspace): a `NavigationView` with Player, All shows and Songs, and Settings in its footer, which opens the existing Settings window. View > Player, All shows and Songs (Ctrl+1, Ctrl+2, Ctrl+3) repeat it in the menu bar. The pane is compact (48) below 1440 effective pixels, where the player needs the width for its transport labels beside the Shows pane, and open (224) above; the pane button still toggles it.
- **Player**: when the open show is linked, its chapters sit under set headings, and setlist songs the recording lacks appear in place as "Not in this recording". Tuning and banter chapters keep their order. Under the title, a line gives the date, place, BerthaML ID and record status, with the link's reason as its tooltip.
- **All shows** (collection workspace): all 2,234 BerthaML shows, with search by date, venue or city, a year filter and "Only shows in the shows folder". The detail pane shows the selected show's reference text and any MKAs of it in the folder; clicking one plays it.
- **Songs** (collection workspace): all 410 songs with their play counts and how many performances the folder holds, filtered by default to those it holds. Choosing a performance opens its show at that chapter and plays it.
- **ABI**: `ListShows`, `GetShow`, `ListSongs` and `DescribeBook`. Show summaries and setlists come from the database; the songs list joins each linked book's chapter performances to the setlist.

## Automated

`pwsh build/build.ps1` passed, including the WinUI style check with no violations, 54 Rust domain tests (new: setlists with set labels and permanent IDs, play counts) and `ShowPlayer.WorkflowCheck`, which now also requires more than 2,000 shows, the Cornell setlist with Scarlet Begonias in set 2, a single book's link through `DescribeBook`, and a song list in which the synthetic titles hold nothing.

## Interface checks

The development build was driven through Windows UI Automation and captured with `tests/capture_window.py`:

- Player on `G:\GDL\770423-ma-springfield.mka`: 21 chapters under Set 1 and Set 2, with the link line "1977-04-23 · Springfield Civic Center Arena, Springfield, MA · BerthaML gd1977-04-23, draft record".
- All shows: searching "1977-04" lists seven shows, Springfield marked "In folder"; selecting it shows its setlist and the MKA.
- Songs: 20 of 410 songs are in the folder; Scarlet Begonias lists its Springfield performance at chapter 9. Choosing it, in a scratch data folder with silent output (`--data <folder> --ao null`), switched to the player and played chapter 9 of 21.
- The navigation pane was captured with real audio output at the first-run size (compact, with icons), starting maximized (expanded) and maximized after starting (expanded). `NavigationView` in Auto mode drew an open pane with compact, label-less items when layout settled late, as it does with a real audio device; that showed in the first installed 0.6.0 build. The pane now uses Left mode, and `FitNavigationPane` opens or closes it once the control has loaded and whenever the width crosses 1440.

The list automation names are "Show list" and "Song list", distinct from the navigation items.

## Delivery

`dist/Gratitude-0.6.0-win-x64-setup.exe` (163 MB) installed silently over 0.5.0 with exit 0, its files matching the published build. Launched from Start with Ian's maximized layout and real audio device, it showed the expanded navigation pane and the Springfield setlist. Ian's saved position for the Springfield MKA was unchanged, and the store stays at schema 3.

## Not covered

Keyboard-only navigation of the new areas was not walked through, and the light theme was not captured. Searching All shows by song title is not offered; the Songs area covers that.
