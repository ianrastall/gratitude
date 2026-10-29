# Gratitude project brief

29 October 2026. Current scope, superseding the initial acquisition prototype.

Gratitude parses and works on recording folders already obtained by the user. It reviews TXT/song/set metadata, packages original audio into MKA without changing sample rate or bit depth, and plays local MKA files with chapters and saved positions. Any artist can use the local workflow. GD imports can consult a bundled, attributed offline show reference for 1967-1995.

A recording/source is the core entity. Different tapes remain separate folders and MKAs. Original TXT bytes remain intact; parser output requires review. Source/modifier badges and category-specific tiers describe local recordings. Only SBD-first versus MTX-first is a user ranking choice.

The app has no online recording search, downloader, torrent engine or streaming service. Archive restrictions and unreliable torrent peers made acquisition unsuitable for this product. Existing downloads and partial downloads survive upgrades. Developer source-acquisition scripts are separate from the installed application and are not executed by ordinary builds.

Rust owns domain operations and local state. WinUI 3/C# owns pickers, navigation, appearance and the unsaved review form through a typed protobuf ABI. One MKA job runs at a time; playback is independent. Data remains under `%LOCALAPPDATA%\show-player`. Install identity and executable name remain compatible with Show Player; the visible name is Gratitude.

Future local-workflow candidates: structured source/lineage editing, artwork, richer TXT grammars, nested set chapters and splitting continuous source files. Acquisition is outside the current scope.
