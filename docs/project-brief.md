# Gratitude project brief

30 October 2026. Current scope, superseding the initial acquisition prototype.

Gratitude organizes Grateful Dead recording folders already obtained by the user, for shows up to Jerry Garcia's death on 9 August 1995. It reviews TXT/song metadata, writes a fixed tag set, packages original audio into MKA or a new folder of tagged files without changing sample rate or bit depth, and plays local MKA files with chapters and saved positions. Imports consult a bundled, attributed offline show reference for 1967-1995.

A recording/source is the core entity. Different tapes remain separate folders and MKAs. Original TXT bytes remain intact; parser output requires review. Source/modifier badges and category-specific tiers describe local recordings. Only SBD-first versus MTX-first is a user ranking choice.

The app has no online recording search, downloader, torrent engine or streaming service. Archive restrictions and unreliable torrent peers made acquisition unsuitable for this product. Existing downloads and partial downloads survive upgrades. Developer source-acquisition scripts are separate from the installed application and are not executed by ordinary builds.

Rust owns domain operations and local state. WinUI 3/C# owns pickers, navigation, appearance and the unsaved review form through a typed protobuf ABI. One MKA job runs at a time; playback is independent. Data remains under `%LOCALAPPDATA%\show-player`. Install identity and executable name remain compatible with Show Player; the visible name is Gratitude.

Future local-workflow candidates: a per-show XML data model (BerthaML), structured source/lineage editing, artwork and splitting continuous source files. Acquisition is outside the current scope.
