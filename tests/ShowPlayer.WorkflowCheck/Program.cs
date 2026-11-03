using System.Diagnostics;
using System.Text.Json;
using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;

if (args.Length >= 2 && args[0] == "--verify-links")
{
    // Read-only: links each folder in Ian's Grateful Dead folder and prints what matched.
    using var linkCore = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(Environment.CurrentDirectory, "work", "link-check-" + Guid.NewGuid().ToString("N")), AudioOutput = "null" });
    foreach (var folder in Directory.GetDirectories(args[1]).Order())
    {
        var linked = linkCore.InspectShow(folder);
        int held = linked.Tracks.Count(t => t.Performances.Count > 0);
        Console.WriteLine(Path.GetFileName(folder));
        Console.WriteLine($"  {linked.ShowId} {linked.RecordingId}: {linked.Link}");
        Console.WriteLine($"  {held} of {linked.Tracks.Count} tracks hold performances");
    }
    return;
}
if (args.Length >= 2 && args[0] == "--verify-winterland")
{
    using var checkCore = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(Environment.CurrentDirectory, "work", "winterland-check-" + Guid.NewGuid().ToString("N")), AudioOutput = "null" });
    var imported = checkCore.InspectShow(args[1]);
    string[] expected = ["Bertha", "Good Lovin'", "Brown-Eyed Women", "Cassidy", "Peggy-O", "Looks Like Rain", "Dire Wolf", "Passenger", "Candyman", "El Paso", "Friend of the Devil", "The Music Never Stopped", "Cold Rain & Snow", "Lazy Lightnin'", "Supplication", "Scarlet Begonias", "Fire on the Mountain", "Estimated Prophet", "He's Gone", "Truckin'", "Wharf Rat", "Around & Around"];
    Require(imported.Tracks.Select(t => t.Title).SequenceEqual(expected), "Real-folder song titles do not match the TXT setlist");
    Require(imported.Tracks.Take(12).All(t => t.Set == "Set 1") && imported.Tracks.Skip(12).All(t => t.Set == "Set 2"), "Real-folder sets are incorrect");
    Require(imported.Artist == "Grateful Dead" && imported.Date == "1977-12-27" && imported.Album == "771227_ca-san-francisco-winterland", "Real-folder header is incorrect");
    Require(imported.Warnings.Count == 1 && imported.Warnings[0].Contains("Samson & Delilah"), "Missing encore warning is incorrect");
    Console.WriteLine("PASS actual Winterland folder: all 22 titles, 12 Set 1 / 10 Set 2, artist/date/venue, missing encore warning");
    if (args.Length >= 3) File.WriteAllText(args[2], Google.Protobuf.JsonFormatter.Default.Format(imported));
    if (args.Length >= 4)
    {
        Directory.CreateDirectory(args[3]);
        checkCore.TagShow(imported, args[3]);
        var tagTimer = Stopwatch.StartNew();
        ShowJob tagJob;
        do { Thread.Sleep(250); tagJob = checkCore.GetJob(); } while (tagJob.State == "Running" && tagTimer.Elapsed.TotalMinutes < 10);
        Require(tagJob.State == "Completed", "Tagging the actual folder failed: " + tagJob.Log);
        Console.WriteLine($"PASS actual Winterland folder tagged in {tagTimer.Elapsed.TotalSeconds:0} s: {tagJob.Output}");
    }
    return;
}

string fixture = args.Length > 0 ? Path.GetFullPath(args[0]) : Path.Combine(Path.GetTempPath(), "show-player-check-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(fixture);
string tools = Path.Combine(AppContext.BaseDirectory, "tools");
string Ffmpeg(params string[] arguments) => Run(Path.Combine(tools, "ffmpeg.exe"), arguments);
static string Run(string program, params string[] arguments)
{
    var start = new ProcessStartInfo(program) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
    foreach (var argument in arguments) start.ArgumentList.Add(argument);
    using var process = Process.Start(start)!;
    var stdout = process.StandardOutput.ReadToEndAsync();
    var stderr = process.StandardError.ReadToEndAsync();
    process.WaitForExit();
    string text = stdout.GetAwaiter().GetResult(), error = stderr.GetAwaiter().GetResult();
    if (process.ExitCode != 0) throw new Exception(error);
    return text;
}
static void Require(bool condition, string message) { if (!condition) throw new Exception(message); }
string source = Path.Combine(fixture, "source's show"); Directory.CreateDirectory(source);
foreach (int track in new[] { 1, 2, 10 })
{
    Ffmpeg("-v", "error", "-y", "-f", "lavfi", "-i", $"sine=frequency={300+track*20}:duration=12", "-ar", "44100", "-ac", "2", "-c:a", "flac", "-metadata", $"title=Embedded song {track} ->", Path.Combine(source, $"track{track}.flac"));
}
string notes = "Grateful Dead\r\n1977-05-08\r\nBarton Hall\r\nSet 1:\r\n01. Test opening >\r\n02. Test transition\r\nEncore:\r\n03. Test encore\r\nSource: SBD synthetic audio for verification\r\n";
File.WriteAllText(Path.Combine(source, "show.txt"), notes);
// Collectors name checksum files after the recording; this names a listed 1977-05-08 soundboard.
File.WriteAllText(Path.Combine(source, "gd77-05-08.sbd.hicks.4982.sbeok.shnf.md5"), "");
using var core = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(fixture, "data"), AudioOutput = "null" });
core.Start();
var draft = core.InspectShow(source);
Require(draft.Tracks.Count == 3 && draft.Artist == "Grateful Dead" && draft.Date == "1977-05-08", "Import metadata mismatch");
Require(!draft.Folder.StartsWith(@"\\?\") && !draft.NotesPath.StartsWith(@"\\?\"), "The import returned a path that folder pickers refuse");
Require(Path.GetFileName(draft.Tracks[2].Path) == "track10.flac", "Natural order mismatch");
Require(draft.Tracks[2].Set == "Encore" && draft.Tracks[0].Title == "Test opening", "Set or song parsing failed");
Require(draft.SourceSummary == "SBD" && draft.ReferenceInfo.Contains("Minglewood Blues"), "Local import source label or offline show reference is missing");
Require(draft.ReferenceInfo.StartsWith("1977-05-08 · Barton Hall, Cornell University, Ithaca, NY") && draft.ReferenceInfo.Contains("BerthaML gd1977-05-08: draft record") && draft.ReferenceInfo.Contains("rather than a record of who played"), "BerthaML show reference is incomplete: " + draft.ReferenceInfo);
Require(draft.Album == "770508_ny-ithaca-barton-hall-cornell-university", "The BerthaML venue did not name the album: " + draft.Album);
Require(draft.ShowId == "gd1977-05-08" && draft.RecordingId == "r-a7630611865608b4e5e6" && draft.Link.Contains("Archive identifier"), "Import was not linked to its BerthaML recording: " + draft.Link);
Require(draft.Tracks.All(t => t.Performances.Count == 0), "Synthetic song titles were matched to setlist performances");
Console.WriteLine("PASS BerthaML link: checksum file name identifies the recording");
Require(core.SearchRecordings("1993-09-22", "All sources").Count > 0, "Catalogue search failed");
var priorities = core.GetPriorities();
Require(priorities.Sources.Take(3).SequenceEqual(new[] { "SBD", "MTX", "AUD" }), "Default source priorities are incorrect");
var cornell = core.SearchRecordings("1977-05-08", "All sources");
Require(cornell.Count > 0 && cornell[0].SourceKind == "SBD" && cornell[0].ShowInfo.Contains("Scarlet Begonias"), "XML show facts or source ranking are missing");
var matrixFirst = priorities.Clone();
matrixFirst.Sources.Clear(); matrixFirst.Sources.Add(new[] { "MTX", "SBD" });
core.SetPriorities(matrixFirst);
Require(core.SearchRecordings("1977-05-08", "All sources")[0].SourceKind == "MTX", "Source preference did not reorder recordings");
using (var reopened = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(fixture, "data"), AudioOutput = "null" }))
    Require(reopened.GetPriorities().Sources[0] == "MTX", "Source preference was not persisted");
core.SetPriorities(priorities);
Console.WriteLine("PASS BerthaML show reference and album name, setlist search, source badges, personal ranking and persisted priorities");
string incompleteNotes = Path.Combine(fixture, "incomplete.txt");
File.WriteAllText(incompleteNotes, "Grateful Dead\n1977-05-08\nBarton Hall\n01. One entry\n");
var fallback = core.InspectShow(source, incompleteNotes);
Require(fallback.Tracks.Select(t => t.Title).SequenceEqual(new[] { "Embedded song 1", "Embedded song 2", "Embedded song 10" }), "Embedded title fallback failed");
Require(fallback.Warnings.Count > 0, "Mismatched TXT should remain visible");
Console.WriteLine("PASS mismatched TXT falls back to embedded audio titles");
foreach (bool copy in new[] { true, false })
{
    draft.Tracks[0].Title = "Test opening ->"; // A manually edited title is cleaned on export too.
    string output = Path.Combine(fixture, copy ? "copy.mka" : "flac.mka");
    core.PackageShow(draft, output, copy);
    var timer = Stopwatch.StartNew();
    ShowJob job;
    do { Thread.Sleep(100); job = core.GetJob(); } while (job.State == "Running" && timer.Elapsed.TotalSeconds < 45);
    Require(job.State == "Completed", "Packaging failed: " + job.Log);
    var listed = core.ListBooks(fixture).Single(b => b.Path == output);
    Require(listed.SourceKind == "SBD", "Library lost the recording type stored in MKA notes");
    // The MKA does not hold the checksum file's name; only the recorded source folder evidence does.
    Require(listed.ShowId == "gd1977-05-08" && listed.RecordingId == "r-a7630611865608b4e5e6" && listed.ChapterPerformances.Count == 3, "Library lost the BerthaML link recorded at packaging: " + listed.Link);
    using var probe = JsonDocument.Parse(Run(Path.Combine(tools, "ffprobe.exe"), "-v", "error", "-show_chapters", "-show_streams", "-show_format", "-of", "json", output));
    Require(probe.RootElement.GetProperty("chapters").GetArrayLength() == 3, "Wrong chapter count");
    Require(probe.RootElement.GetProperty("chapters")[0].GetProperty("tags").GetProperty("title").GetString() == "Test opening", "Export retained a manually entered segue marker");
    Require(probe.RootElement.GetProperty("chapters")[2].GetProperty("tags").GetProperty("title").GetString() == "Test encore", "Chapter title mismatch");
    Require(probe.RootElement.GetProperty("chapters")[2].GetProperty("tags").GetProperty("track").GetString() == "3", "Chapter track number mismatch");
    var mkaTags = probe.RootElement.GetProperty("format").GetProperty("tags").EnumerateObject().Select(p => p.Name.ToUpperInvariant()).Order().ToArray();
    // ENCODER here is Matroska's required muxing application field, not a tag.
    Require(mkaTags.SequenceEqual(new[] { "ALBUM", "ARTIST", "COMMENT", "DATE", "ENCODER", "GENRE", "TITLE" }), "MKA tags are not the exact set: " + string.Join(",", mkaTags));
    Require(probe.RootElement.GetProperty("streams")[0].GetProperty("tags").EnumerateObject().All(p => p.Name == "DURATION"), "MKA stream kept a tag beyond the muxer's duration");
    Require(Math.Abs(double.Parse(probe.RootElement.GetProperty("format").GetProperty("duration").GetString()!, System.Globalization.CultureInfo.InvariantCulture)-36) < 0.02, "Wrong MKA duration");
    string attachment = Path.Combine(fixture, copy ? "copy-notes.txt" : "flac-notes.txt");
    Ffmpeg("-v", "error", "-y", "-dump_attachment:t:0", attachment, "-i", output, "-f", "null", "-");
    Require(File.ReadAllBytes(attachment).SequenceEqual(File.ReadAllBytes(Path.Combine(source, "show.txt"))), "Original TXT bytes changed");
    string decoded = Path.Combine(fixture, copy ? "copy.pcm" : "flac.pcm");
    Ffmpeg("-v", "error", "-y", "-i", output, "-f", "s16le", decoded);
    string expected = Path.Combine(fixture, "expected.pcm");
    if (!File.Exists(expected))
    {
        using var stream = File.Create(expected);
        foreach (var track in draft.Tracks)
        {
            string part = Path.Combine(fixture, "part.pcm");
            Ffmpeg("-v", "error", "-y", "-i", track.Path, "-f", "s16le", part);
            stream.Write(File.ReadAllBytes(part));
        }
    }
    Require(File.ReadAllBytes(decoded).SequenceEqual(File.ReadAllBytes(expected)), "Decoded audio differs from original concatenated samples");
    try { core.PackageShow(draft, output, copy); throw new Exception("Overwrite was allowed"); }
    catch (AudioCoreException) { }
    Console.WriteLine($"PASS {(copy ? "stream copy" : "FLAC encode")}: chapters, exact TXT, exact decoded audio, overwrite protection");
}
core.Open(Path.Combine(fixture, "copy.mka"), out _);
var wait = Stopwatch.StartNew();
Snapshot snapshot;
do { Thread.Sleep(100); snapshot = core.GetSnapshot(true); } while (snapshot.State == PlaybackState.Loading && wait.Elapsed.TotalSeconds < 15);
Require(snapshot.Chapters.Count == 3, "Player failed to load generated MKA chapters");
core.SeekAbsolute(17); Thread.Sleep(700); core.Pause(); Thread.Sleep(200);
core.CloseBook(); Thread.Sleep(400);
core.Open(Path.Combine(fixture, "copy.mka"), out _);
var resumeWait = Stopwatch.StartNew();
do { Thread.Sleep(100); } while (core.GetSnapshot().State != PlaybackState.Paused && resumeWait.Elapsed.TotalSeconds < 15);
var resumed = core.GetSnapshot();
Require(Math.Abs(resumed.Position - 17) < 0.5, $"Resume position failed: {resumed.State} at {resumed.Position} of {resumed.Duration}");
Console.WriteLine("PASS generated MKA playback and resume through managed/native boundary");

var preservationSources = new List<string>();
foreach (var (rate, bits) in new[] { (44100, 16), (48000, 24), (96000, 24) })
{
    string directory = Path.Combine(fixture, $"preserve-{rate}-{bits}");
    Directory.CreateDirectory(directory);
    for (int i = 1; i <= 2; i++)
        Ffmpeg("-v", "error", "-y", "-f", "lavfi", "-i", $"aevalsrc=0.2*sin(2*PI*{400+i*19}*t)+0.000123*sin(2*PI*991*t):s={rate}:d=0.{(i==1?"317839":"261113")}", "-ac", "2", "-sample_fmt", bits == 16 ? "s16" : "s32", "-bits_per_raw_sample", bits.ToString(), "-c:a", "flac", Path.Combine(directory, $"track{i}.flac"));
    File.WriteAllText(Path.Combine(directory, "notes.txt"), "Test artist\n1977-05-08\nTest venue\nSet 1\n01. First ->\n02. Second >\n");
    var high = core.InspectShow(directory);
    Require(high.Tracks.All(t => t.SampleRate == rate && t.BitDepth == bits), "Source precision was not identified correctly");
    Require(high.Tracks.Select(t => t.Title).SequenceEqual(new[] { "First", "Second" }), "High-resolution source titles retained segue marks");
    preservationSources.Add(high.Tracks[0].Path);
    string expectedPcm = Path.Combine(directory, "expected.pcm");
    using (var expectedStream = File.Create(expectedPcm))
        foreach (var track in high.Tracks)
        {
            string part = Path.Combine(directory, "part.pcm");
            Ffmpeg("-v", "error", "-y", "-i", track.Path, "-f", "s32le", part);
            expectedStream.Write(File.ReadAllBytes(part));
        }
    if (bits == 24) Require(File.ReadAllBytes(expectedPcm).Where((_, i) => i % 4 == 1).Any(b => b != 0), "24-bit fixture does not exercise precision beyond 16 bits");
    foreach (bool copy in new[] { true, false })
    {
        string output = Path.Combine(directory, copy ? "copy.mka" : "flac.mka");
        core.PackageShow(high, output, copy);
        var limit = Stopwatch.StartNew();
        ShowJob result;
        do { Thread.Sleep(100); result = core.GetJob(); } while (result.State == "Running" && limit.Elapsed.TotalSeconds < 30);
        Require(result.State == "Completed", "Preservation export failed: " + result.Log);
        using var probe = JsonDocument.Parse(Run(Path.Combine(tools, "ffprobe.exe"), "-v", "error", "-select_streams", "a:0", "-show_streams", "-of", "json", output));
        var stream = probe.RootElement.GetProperty("streams")[0];
        Require(stream.GetProperty("sample_rate").GetString() == rate.ToString() && stream.GetProperty("bits_per_raw_sample").GetString() == bits.ToString(), "Output rate or bit depth changed");
        string decoded = Path.Combine(directory, copy ? "copy.pcm" : "flac.pcm");
        Ffmpeg("-v", "error", "-y", "-i", output, "-f", "s32le", decoded);
        Require(File.ReadAllBytes(decoded).SequenceEqual(File.ReadAllBytes(expectedPcm)), "Audio samples or join changed");
        Console.WriteLine($"PASS {rate} Hz / {bits} bit {(copy ? "copy" : "FLAC")}: original rate, depth and every sample preserved across joins");
    }
}
string mixedFolder = Path.Combine(fixture, "mixed-formats"); Directory.CreateDirectory(mixedFolder);
File.Copy(preservationSources[0], Path.Combine(mixedFolder, "track1.flac"));
File.Copy(preservationSources[2], Path.Combine(mixedFolder, "track2.flac"));
var mixed = core.InspectShow(mixedFolder);
Require(mixed.Warnings.Any(w => w.Contains("never resamples")), "Mixed-rate sources must be flagged");
try { core.PackageShow(mixed, Path.Combine(fixture, "must-not-resample.mka"), false); throw new Exception("Mixed-rate conversion was allowed"); }
catch (AudioCoreException) { }
Require(!File.Exists(Path.Combine(fixture, "must-not-resample.mka")), "Mixed-rate output was written");
Console.WriteLine("PASS actual mixed source rates and depths refused without resampling");
var staleReview = core.InspectShow(Path.GetDirectoryName(preservationSources[1])!);
foreach (var track in staleReview.Tracks) track.SampleRate = 96000; // Reviewed metadata is not authoritative over actual source files.
string refusedOutput = Path.Combine(fixture, "must-not-trust-review-rate.mka");
core.PackageShow(staleReview, refusedOutput, false);
var refusalWait = Stopwatch.StartNew();
ShowJob refusedJob;
do { Thread.Sleep(100); refusedJob = core.GetJob(); } while (refusedJob.State == "Running" && refusalWait.Elapsed.TotalSeconds < 15);
Require(refusedJob.State == "Failed" && !File.Exists(refusedOutput), "Export trusted a rate that differs from actual source audio");
Console.WriteLine("PASS export re-probes source format and refuses stale/altered review metadata");

string Tags(string path) => Run(Path.Combine(tools, "ffprobe.exe"), "-v", "error", "-show_entries", "format_tags:stream_tags", "-of", "json", path);
string Samples(string path) => Ffmpeg("-v", "error", "-i", path, "-map", "0:a:0", "-c:a", "pcm_s32le", "-f", "md5", "-");
void WaitForJob(string name)
{
    var limit = Stopwatch.StartNew();
    ShowJob job;
    do { Thread.Sleep(100); job = core.GetJob(); } while (job.State == "Running" && limit.Elapsed.TotalSeconds < 60);
    Require(job.State == "Completed", name + " failed: " + job.Log);
}
string Field(JsonElement tags, string name) => tags.EnumerateObject().Single(p => string.Equals(p.Name, name, StringComparison.OrdinalIgnoreCase)).Value.GetString()!;

string tagged = Path.Combine(fixture, "tagged"); Directory.CreateDirectory(tagged);
string dirtySource = Path.Combine(fixture, "dirty-tags"); Directory.CreateDirectory(dirtySource);
for (int i = 1; i <= 2; i++)
    Ffmpeg("-v", "error", "-y", "-f", "lavfi", "-i", $"sine=frequency={300 + i * 40}:duration=3", "-ar", "44100", "-ac", "2", "-c:a", "flac", "-metadata", "title=d1t01 Wrong", "-metadata", "disc=1", "-metadata", "albumartist=Someone", "-metadata", "encodedby=Tool", Path.Combine(dirtySource, $"gd77-05-08d1t0{i}.flac"));
File.WriteAllText(Path.Combine(dirtySource, "gd77-05-08.txt"), "Grateful Dead\n1977-05-08\nBarton Hall\nSource: SBD > Reel\nSet 1:\n01. 01 Minglewood Blues ->\n02. 02 Loser (1)\n");
var dirty = core.InspectShow(dirtySource);
Require(dirty.Tracks.Select(t => t.Title).SequenceEqual(new[] { "Minglewood Blues", "Loser" }), "Doubled numbers reached the song title");
Require(dirty.Album.StartsWith("770508_ny-ithaca-") && dirty.Comment == "Equipment: SBD > Reel", $"Album or comment wrong: {dirty.Album} | {dirty.Comment}");
core.TagShow(dirty, tagged); WaitForJob("Tagging");
string[] written = Directory.GetFiles(Path.Combine(tagged, dirty.Album), "*.flac").OrderBy(p => p, StringComparer.Ordinal).ToArray();
Require(written.Select(Path.GetFileName).SequenceEqual(new[] { "1 Minglewood Blues.flac", "2 Loser.flac" }), "Tagged file names are wrong: " + string.Join(",", written.Select(Path.GetFileName)));
Require(File.Exists(Path.Combine(tagged, dirty.Album, "gd77-05-08.txt")), "The notes were not copied");
using (var probe = JsonDocument.Parse(Tags(written[1])))
{
    var tags = probe.RootElement.GetProperty("format").GetProperty("tags");
    Require(tags.EnumerateObject().Count() == 7, "Tagged file does not hold exactly seven tags: " + tags);
    Require(Field(tags, "title") == "Loser" && Field(tags, "album") == dirty.Album && Field(tags, "date") == "1977-05-08" && Field(tags, "track") == "2"
        && Field(tags, "artist") == "Grateful Dead" && Field(tags, "comment") == dirty.Comment && Field(tags, "genre") == "Psychedelic Rock", "Tagged values are wrong: " + tags);
}
string flacHeader = System.Text.Encoding.UTF8.GetString(File.ReadAllBytes(written[1])[..4096]);
Require(flacHeader.Contains("COMMENT=" + dirty.Comment) && !flacHeader.Contains("DESCRIPTION="), "FLAC comment is not in the COMMENT field");
Require(Samples(written[1]) == Samples(dirty.Tracks[1].Path), "Tagging changed the audio");
Require(Tags(dirty.Tracks[1].Path).Contains("Someone"), "Tagging changed the source file");
try { core.TagShow(dirty, tagged); throw new Exception("Replacing a tagged folder was allowed"); }
catch (AudioCoreException) { }
Console.WriteLine("PASS tagged folder: exact tag set, old tags gone, samples and source unchanged, no replacement");

string mp3Source = Path.Combine(fixture, "mp3-show"); Directory.CreateDirectory(mp3Source);
Ffmpeg("-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-ar", "44100", "-ac", "2", "-c:a", "libmp3lame", "-metadata", "publisher=Someone", "-id3v2_version", "3", "-write_id3v1", "1", Path.Combine(mp3Source, "01 Sugaree.mp3"));
var mp3 = core.InspectShow(mp3Source);
Require(mp3.Tracks[0].Title == "Sugaree", "File name title was not cleaned");
mp3.Date = "1977-05-08"; mp3.Album = "770508_ny-ithaca-barton-hall-mp3";
core.TagShow(mp3, tagged); WaitForJob("MP3 tagging");
string mp3Out = Directory.GetFiles(Path.Combine(tagged, mp3.Album), "*.mp3").Single();
byte[] mp3Bytes = File.ReadAllBytes(mp3Out);
Require(mp3Bytes[0] == 'I' && mp3Bytes[1] == 'D' && mp3Bytes[2] == '3' && mp3Bytes[3] == 4, "MP3 output is not ID3v2.4");
Require(!(mp3Bytes[^128] == 'T' && mp3Bytes[^127] == 'A' && mp3Bytes[^126] == 'G'), "MP3 output kept ID3v1");
Require(!Tags(mp3Out).Contains("Someone"), "MP3 output kept an old tag");
Require(!System.Text.Encoding.Latin1.GetString(mp3Bytes[..4096]).Contains("TSSE"), "MP3 output has an ID3 encoder frame");
Console.WriteLine("PASS MP3: ID3v2.4 only, old tags gone");

string late = Path.Combine(fixture, "after-1995"); Directory.CreateDirectory(late);
File.Copy(written[0], Path.Combine(late, "t1.flac"));
File.WriteAllText(Path.Combine(late, "n.txt"), "Fare Thee Well\n2015-07-05\nSoldier Field\n01. Truckin\n");
var lateDraft = core.InspectShow(late);
Require(lateDraft.Warnings.Any(w => w.Contains("1995-08-09")), "A show after August 1995 was not flagged");
try { core.PackageShow(lateDraft, Path.Combine(fixture, "late.mka"), true); throw new Exception("A show after August 1995 was accepted"); }
catch (AudioCoreException) { }
Console.WriteLine("PASS shows after 9 August 1995 are refused");

if (args.Length > 1)
{
    // Shorten has no free encoder, so its fixture comes from tests/make_shn.py.
    string shn = Path.GetFullPath(args[1]);
    var shnDraft = core.InspectShow(shn);
    Require(shnDraft.Tracks.Count == 2 && shnDraft.Tracks.All(t => t.Codec == "shorten" && t.Duration > 0 && t.BitDepth == 16), "SHN import failed");
    Require(Math.Abs(shnDraft.Tracks[0].Duration - 3) < 0.001 && Math.Abs(shnDraft.Tracks[1].Duration - 2.5) < 0.001, "SHN durations are wrong");
    Require(shnDraft.Tracks.Select(t => t.Title).SequenceEqual(new[] { "Bertha", "Sugaree" }), "SHN titles are wrong");
    string shnMka = Path.Combine(fixture, "shn.mka");
    core.PackageShow(shnDraft, shnMka, true); WaitForJob("SHN MKA");
    string shnWhole = Path.Combine(fixture, "shn-whole.pcm"), shnExpected = Path.Combine(fixture, "shn-expected.pcm");
    Ffmpeg("-v", "error", "-y", "-i", shnMka, "-f", "s16le", shnWhole);
    using (var expectedStream = File.Create(shnExpected))
        foreach (var track in shnDraft.Tracks)
        {
            string part = Path.Combine(fixture, "shn-part.pcm");
            Ffmpeg("-v", "error", "-y", "-i", track.Path, "-f", "s16le", part);
            expectedStream.Write(File.ReadAllBytes(part));
        }
    Require(File.ReadAllBytes(shnWhole).SequenceEqual(File.ReadAllBytes(shnExpected)), "SHN samples changed in the MKA");
    core.TagShow(shnDraft, tagged); WaitForJob("SHN tagging");
    string[] shnFiles = Directory.GetFiles(Path.Combine(tagged, shnDraft.Album), "*.flac").OrderBy(p => p, StringComparer.Ordinal).ToArray();
    Require(shnFiles.Length == 2 && Samples(shnFiles[0]) == Samples(shnDraft.Tracks[0].Path), "SHN samples changed in the tagged FLAC");
    using var probe = JsonDocument.Parse(Tags(shnFiles[1]));
    Require(probe.RootElement.GetProperty("format").GetProperty("tags").EnumerateObject().Count() == 7, "SHN output does not hold exactly seven tags");
    Console.WriteLine("PASS SHN: duration found, MKA and tagged FLAC hold every sample");
}
Console.WriteLine("Fixtures: " + fixture);
