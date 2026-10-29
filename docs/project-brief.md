# Project brief

29 October 2026. First implementation requested by Ian in the MKA/show-player conversation.

Purpose: collect a live recording, preserve its taper/source notes, create a portable MKA, and listen to the whole show with song navigation. GD is the initial audience and online catalogue. Other artists' local folders use the same import and playback model.

The core entity is a recording/source, not merely a performance date. Different tapes of the same concert remain separate entries, folders and MKA files. Original source TXT is retained verbatim. Parser output is a proposal requiring review; metadata mismatches are visible and track titles remain editable.

The application is local, single-user and single-instance. Rust owns file inspection, parsing, catalogue queries, download/package validation, process orchestration, playback and resume persistence. C# owns navigation, native pickers, appearance, window placement and the unsaved review form. One job runs at a time; playback remains independent. Network operations are explicit downloads/source resolution. No accounts, telemetry, cloud processing, remote LLM or background catalogue crawling is used.

Data is stored under `%LOCALAPPDATA%\show-player`. Download and output locations are user-selected; default downloads are under Music/Show Player/Sources. Sources are retained, and output creation refuses overwrite. A failed/cancelled package does not publish a partial MKA. Temporary packaging needs approximately twice the final MKA size while committing the output. Downloads retain aria2 control files for retry/resume.

Version 0.1 scope: saved GD catalogue search; current etree/Archive torrent resolution; original-file selection for Archive; aria2 downloads with webseed support and optional timed seeding; folder/TXT import; metadata/song/set review and reorder; stream-copy or FLAC packaging; attached original notes; chapter playback and resume; Jupiter appearance; self-contained installer and Start Menu registration.

Further work: live catalogue refresh, persistent multi-job queues, structured source/lineage fields, artwork packaging, nested Matroska set chapters, chapter splitting inside continuous recordings, richer TXT grammars and source matching. TXT formats vary; the first parser supports common numeric and disc/track forms, unnumbered set sections, ISO/US/named-month dates and encore labels. It deliberately leaves mismatched songs under filename titles with a review warning.

Network conditions are outside the app's control. Valid torrent metadata does not guarantee active peers. Archive policy is checked per item and source restrictions are not bypassed. Direct HTTP fallback is not implemented in this version.
