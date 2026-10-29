# Validation — Gratitude 0.2.1

29 October 2026, Windows 11 x64.

## Behavior and policy

The app is named Gratitude throughout its windows, About, product metadata and installer. Existing application/installer identity, executable name and data location remain stable for upgrades. New Start Menu entry: Gratitude; the obsolete Show Player shortcut is removed by the installer.

Only SBD-first or MTX-first is configurable. Legacy free-form preferences normalize to those choices by retaining the relative order of SBD/MTX; AUD, FM and UNKNOWN always follow. The supplied category tables are fixed policy: SBD tiers 3/7 people, AUD 5/6/6, MTX 2/4. Members of a tier tie; unlisted names follow listed tiers. Historical/quality commentary from the supplied text is not incorporated as fact. The XML includes the policy tables separately from show facts and has been rebuilt with expanded aliases; dated-entry and recording counts remain 2,305 and 19,297.

## Checks

- Formatting and Clippy with warnings denied pass. 29 domain and 3 ABI tests pass, including category-specific tiers, tied group members, source preference and legacy preference normalization. A Miller transfer does not give an audience tape a soundboard-engineer tier.
- Managed/native workflow confirms default SBD ranking, switching Cornell to MTX first and persistence through a reopened core. Audio/attachment preservation, rates/depths/sample comparisons, mixed-format refusal, playback/resume and local torrent webseeding pass.
- WinUI build/style checks pass with zero warnings/errors. Colored source/modifier boxes appear in search, recording details and library templates. White-on-fill contrast is 9.33:1 SBD, 9.55:1 MTX, 8.00:1 AUD, 8.52:1 modifiers; UNKNOWN is neutral. Labels retain source text and separate inferred-type notices. Contrast-mode token substitution is implemented; OS contrast switching was not exercised.
- Computer Use caught a preview crash from `AccessibilitySettings.HighContrastChanged`. It was replaced with the `UISettings.ColorValuesChanged` event used by Jupiter.UI. The corrected build opens the workshop and renders results normally. Dark/light appearance and live SBD/MTX switching were inspected; SBD-first and Dark were restored. The final published assembly contains the corrected event handler.

## Installed upgrade

The final installer completed with exit 0. The existing install directory was retained; Start now contains `Gratitude.lnk`, with the old `Show Player.lnk` removed. Product metadata reports Gratitude 0.2.1. Launching the new shortcut produced the installed Gratitude window with SBD-first and Dark restored. The installed managed assembly, native core and XML hashes match the final publish folder, including the corrected color-change handler. The upgrade left `ui.json`, `bookshelf.db` and `recording-priorities.json` byte-for-byte unchanged from the post-verification snapshot. No audio or source notes were modified.
