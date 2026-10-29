using System.Diagnostics;
using System.Text.Json;
using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;

if (args.Length >= 2 && args[0] == "--verify-winterland")
{
    using var checkCore = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(Environment.CurrentDirectory, "work", "winterland-check-" + Guid.NewGuid().ToString("N")), AudioOutput = "null" });
    var imported = checkCore.InspectShow(args[1]);
    string[] expected = ["Bertha ->", "Good Lovin'", "Brown-Eyed Women", "Cassidy", "Peggy-O", "Looks Like Rain", "Dire Wolf", "Passenger", "Candyman", "El Paso", "Friend of the Devil", "The Music Never Stopped", "Cold Rain & Snow", "Lazy Lightnin' ->", "Supplication", "Scarlet Begonias ->", "Fire on the Mountain", "Estimated Prophet ->", "He's Gone", "Truckin' ->", "Wharf Rat ->", "Around & Around"];
    Require(imported.Tracks.Select(t => t.Title).SequenceEqual(expected), "Real-folder song titles do not match the TXT setlist");
    Require(imported.Tracks.Take(12).All(t => t.Set == "Set 1") && imported.Tracks.Skip(12).All(t => t.Set == "Set 2"), "Real-folder sets are incorrect");
    Require(imported.Artist == "Grateful Dead" && imported.Date == "1977-12-27" && imported.Venue == "Winterland Arena", "Real-folder header is incorrect");
    Require(imported.Warnings.Count == 1 && imported.Warnings[0].Contains("Samson & Delilah"), "Missing encore warning is incorrect");
    Console.WriteLine("PASS actual Winterland folder: all 22 titles, 12 Set 1 / 10 Set 2, artist/date/venue, missing encore warning");
    if (args.Length >= 3) File.WriteAllText(args[2], Google.Protobuf.JsonFormatter.Default.Format(imported));
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
    Ffmpeg("-v", "error", "-y", "-f", "lavfi", "-i", $"sine=frequency={300+track*20}:duration=12", "-ar", "44100", "-ac", "2", "-c:a", "flac", "-metadata", $"title=Embedded song {track}", Path.Combine(source, $"track{track}.flac"));
}
string notes = "Grateful Dead\r\n1977-05-08\r\nBarton Hall\r\nSet 1:\r\n01. Test opening >\r\n02. Test transition\r\nEncore:\r\n03. Test encore\r\nSource: synthetic audio for verification\r\n";
File.WriteAllText(Path.Combine(source, "show.txt"), notes);
using var core = AudioCore.Create(new CoreOptions { DataDirectory = Path.Combine(fixture, "data"), AudioOutput = "null" });
core.Start();
var draft = core.InspectShow(source);
Require(draft.Tracks.Count == 3 && draft.Artist == "Grateful Dead" && draft.Date == "1977-05-08", "Import metadata mismatch");
Require(Path.GetFileName(draft.Tracks[2].Path) == "track10.flac", "Natural order mismatch");
Require(draft.Tracks[2].Set == "Encore" && draft.Tracks[0].Title == "Test opening >", "Set or song parsing failed");
Require(core.SearchRecordings("1993-09-22", "All sources").Count > 0, "Catalogue search failed");
string incompleteNotes = Path.Combine(fixture, "incomplete.txt");
File.WriteAllText(incompleteNotes, "Grateful Dead\n1977-05-08\nBarton Hall\n01. One entry\n");
var fallback = core.InspectShow(source, incompleteNotes);
Require(fallback.Tracks.Select(t => t.Title).SequenceEqual(new[] { "Embedded song 1", "Embedded song 2", "Embedded song 10" }), "Embedded title fallback failed");
Require(fallback.Warnings.Count > 0, "Mismatched TXT should remain visible");
Console.WriteLine("PASS mismatched TXT falls back to embedded audio titles");
foreach (bool copy in new[] { true, false })
{
    string output = Path.Combine(fixture, copy ? "copy.mka" : "flac.mka");
    core.PackageShow(draft, output, copy);
    var timer = Stopwatch.StartNew();
    ShowJob job;
    do { Thread.Sleep(100); job = core.GetJob(); } while (job.State == "Running" && timer.Elapsed.TotalSeconds < 45);
    Require(job.State == "Completed", "Packaging failed: " + job.Log);
    using var probe = JsonDocument.Parse(Run(Path.Combine(tools, "ffprobe.exe"), "-v", "error", "-show_chapters", "-show_streams", "-show_format", "-of", "json", output));
    Require(probe.RootElement.GetProperty("chapters").GetArrayLength() == 3, "Wrong chapter count");
    Require(probe.RootElement.GetProperty("chapters")[2].GetProperty("tags").GetProperty("title").GetString() == "Encore / Test encore", "Chapter title mismatch");
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
core.Open(Path.Combine(fixture, "copy.mka"), out _); Thread.Sleep(1000);
Require(Math.Abs(core.GetSnapshot().Position - 17) < 0.5, "Resume position failed");
Console.WriteLine("PASS generated MKA playback and resume through managed/native boundary");
Console.WriteLine("Fixtures: " + fixture);
