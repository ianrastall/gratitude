# Validation 0.8.0

3 November 2026, Windows 11 x64. Scope: every show card and the player heading in one layout, and credits with roles.

## The problem

Ian showed two cards from the Shows pane. The Springfield MKA, made by Show Player 0.1, read well because its title was a hand-written description. The Pembroke Pines MKA, made by 0.6, showed `770522_fl-pembroke-pines-sportatorium`, since from 0.3 an MKA's title is its album name, and the cards printed titles. Both cards credited people as "role unspecified; name inferred".

## Cards and heading

Each book now carries its show date (`date`, and `show_date` written out as "Sunday, May 22, 1977") and its place (venue, city and region), from its BerthaML show; unlinked books fall back to their own date and title. A card shows the source badges, the date in bold, the place, the credits with roles and the progress. The player heading shows the same date and place, then the source, credits and BerthaML record status, and the window title is the date. The folder is listed by show date. The Recent menu uses date and place. The All shows pane's list of copies shows each copy's credits and file name, since the show is already known there. Searching the Shows pane also matches dates, places and credits.

## Roles

`catalogue::credited_in` reads who recorded and who transferred or mastered from the notes ("Taper: ...", "Recorded by ...", "All Transfers and Mastering By ..."), ignoring lineage chains. A recognised person with no stated role takes the role of the user-supplied tier they are in: the board engineers and audience tapers recorded; Miller, Latvala, Lemieux, Norman, Stubblebine, Kidd Candelario and Rex Jackson transferred and mastered; the matrix tier made matrices. Someone the notes credit by name who is not in the alias list is still shown. Credits read as one line, such as "Taper: Rob Bertrando · Transfer and mastering: Charlie Miller", in the cards, heading, Workshop, Recent tooltips and the standard report. "Hunter's Matrix" in a folder name now names Hunter Seamons.

## Player setlist

Chapters ahead of the first placed one (the Pembroke Pines opening "Funiculi Funicula") now sit under that chapter's set, as the report already did.

## Development

A run with its own `--data` folder now has its own single-instance lock, so a test build can run while Ian's installed copy is open; ordinary launches share the unscoped lock as before.

## Checks

59 Rust domain tests passed, including role reading from notes and tiers and weekday names. The workflow check now requires the packaged MKA's card to read "Sunday, May 8, 1977" and "Barton Hall, Cornell University, Ithaca, NY". The development build was captured with a scratch copy of Ian's data while his installed copy stayed open: Springfield reads "Saturday, April 23, 1977 / Springfield Civic Center Arena, Springfield, MA / Transfer and mastering: Charlie Miller", and Pembroke Pines "Sunday, May 22, 1977 / Sportatorium, Pembroke Pines, FL / Taper: Rob Bertrando".
