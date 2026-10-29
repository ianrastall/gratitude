// Binding check. Speaks the same stdin and JSON-lines protocol as the Rust
// abharness.exe, but reaches the core through the C ABI and the C# interop
// assembly, so the stage 3 resume tests can be repeated through managed code
// unchanged.
//
// Usage: ShowPlayer.BindingCheck [--data <dir>] [--ao <driver>] [--interval <ms>] [--gc-stress yes]
// --gc-stress runs collections and finalizers continuously while abandoning
// result handles to their finalizers, to flush out lifetime mistakes in the binding.

using System.Diagnostics;
using System.Globalization;
using System.Text;
using System.Text.Json;
using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;

var clock = Stopwatch.StartNew();
var output = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false)) { AutoFlush = false };
var gate = new object();
long lastEventMs = 0;
int shutdownReturned = 0;
int lateEvents = 0;

string? dataDirectory = null, audioOutput = null;
uint interval = 0;
bool gcStress = false;
for (int i = 0; i + 1 < args.Length; i += 2)
{
    switch (args[i])
    {
        case "--data": dataDirectory = args[i + 1]; break;
        case "--ao": audioOutput = args[i + 1]; break;
        case "--interval": interval = uint.Parse(args[i + 1], CultureInfo.InvariantCulture); break;
        case "--gc-stress": gcStress = args[i + 1] == "yes"; break;
        default:
            Console.Error.WriteLine("unknown option");
            return 2;
    }
}

void Line(string json)
{
    lock (gate)
    {
        output.Write(json);
        output.Write('\n');
        output.Flush();
    }
}

static string N(double value) => value.ToString("F3", CultureInfo.InvariantCulture);
static string S(string value) => JsonSerializer.Serialize(value);

static string StateName(PlaybackState state) => state switch
{
    PlaybackState.Idle => "idle",
    PlaybackState.Loading => "loading",
    PlaybackState.Paused => "paused",
    PlaybackState.Playing => "playing",
    PlaybackState.Finished => "finished",
    PlaybackState.Missing => "missing",
    PlaybackState.Failed => "failed",
    _ => "?",
};

static string EventName(EventType type) => type switch
{
    EventType.StateChanged => "state",
    EventType.BookLoaded => "book_loaded",
    EventType.LoadFailed => "load_failed",
    EventType.Position => "position",
    EventType.ChapterChanged => "chapter",
    EventType.Finished => "finished",
    EventType.SaveFailed => "save_failed",
    EventType.SaveRecovered => "save_recovered",
    EventType.VolumeChanged => "volume",
    _ => "?",
};

// The protocol prints the earlier ab_status numbers for errors.
static int Code(ErrorKind kind) => kind switch
{
    ErrorKind.None => 0,
    ErrorKind.InvalidArgument => -1,
    ErrorKind.InvalidState => -2,
    ErrorKind.NotFound => -3,
    ErrorKind.Io => -4,
    ErrorKind.Store => -5,
    ErrorKind.Engine => -6,
    ErrorKind.UnknownCommand => -7,
    ErrorKind.ShuttingDown => -8,
    _ => -10,
};

void Reply(string command, int status) =>
    Line($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"reply\",\"command\":{S(command)},\"status\":{status}}}");

void Log(int level, string message) =>
    Line($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"log\",\"level\":{level},\"message\":{S(message)}}}");

AudioCore core;
try
{
    core = AudioCore.Create(new CoreOptions { DataDirectory = dataDirectory, AudioOutput = audioOutput, SaveIntervalMs = interval });
}
catch (AudioCoreException error)
{
    Log(0, error.Message);
    Reply("create", Code(error.Kind));
    return 1;
}

core.Faulted += message => Log(0, message);
core.EventsArrived += batch =>
{
    foreach (var e in batch)
    {
        if (e.Type == EventType.Log)
        {
            Log(e.LogLevel, e.Message);
            continue;
        }
        if (Volatile.Read(ref shutdownReturned) != 0) Interlocked.Increment(ref lateEvents);
        Interlocked.Exchange(ref lastEventMs, clock.ElapsedMilliseconds);
        var json = new StringBuilder(160);
        json.Append($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"{EventName(e.Type)}\",\"gen\":{e.Generation},");
        json.Append($"\"state\":\"{StateName(e.State)}\",\"pos\":{N(e.Position)},\"dur\":{N(e.Duration)},\"chapter\":{e.Chapter}");
        if (e.RequestId != 0) json.Append($",\"request\":{e.RequestId}");
        if (e.Error != ErrorKind.None || e.EngineError != 0) json.Append($",\"error\":{Code(e.Error)},\"engine_error\":{e.EngineError}");
        if (e.Type == EventType.VolumeChanged)
            json.Append($",\"volume\":{e.Volume.ToString("F1", CultureInfo.InvariantCulture)},\"mute\":{(e.Mute ? 1 : 0)}");
        json.Append('}');
        Line(json.ToString());
    }
};
core.Start();
Reply("create", 0);

using var stressStop = new CancellationTokenSource();
Thread? stress = null;
long collections = 0, abandoned = 0;
if (gcStress)
{
    stress = new Thread(() =>
    {
        while (!stressStop.IsCancellationRequested)
        {
            // Snapshots read and dropped at once: their result handles are released while the GC runs.
            for (int i = 0; i < 8; i++)
            {
                try
                {
                    _ = core.GetSnapshot(includeChapters: true);
                    abandoned++;
                }
                catch (Exception)
                {
                    break;  // the core is shutting down
                }
            }
            _ = new byte[64 * 1024];
            GC.Collect();
            GC.WaitForPendingFinalizers();
            collections++;
            Thread.Sleep(2);
        }
    }) { IsBackground = true, Name = "gc-stress" };
    stress.Start();
}

void Settle()
{
    long begin = clock.ElapsedMilliseconds;
    while (true)
    {
        Thread.Sleep(20);
        long now = clock.ElapsedMilliseconds;
        PlaybackState state = core.GetSnapshot().State;
        bool quiet = now - Interlocked.Read(ref lastEventMs) >= 150 && now - begin >= 150;
        if ((quiet && state != PlaybackState.Loading) || now - begin > 10000) return;
    }
}

void PrintStatus(bool chapterList)
{
    var s = core.GetSnapshot(includeChapters: true);
    var chapters = s.Chapters;
    if (chapterList)
    {
        var list = string.Join(",", chapters.Select(c => $"{{\"start\":{N(c.Start)},\"end\":{N(c.End)},\"title\":{S(c.Title)}}}"));
        Line($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"chapters\",\"gen\":{s.Generation},\"list\":[{list}]}}");
        return;
    }
    Chapter? current = s.Chapter >= 0 && s.Chapter < chapters.Count ? chapters[s.Chapter] : null;
    Line($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"status\",\"gen\":{s.Generation},\"state\":\"{StateName(s.State)}\"," +
         $"\"pos\":{N(s.Position)},\"dur\":{N(s.Duration)},\"chapter\":{s.Chapter},\"chapter_count\":{s.ChapterCount}," +
         $"\"chapter_title\":{S(current?.Title ?? "")},\"chapter_start\":{N(current?.Start ?? 0)}," +
         $"\"error\":{Code(s.Error)},\"engine_error\":{s.EngineError}," +
         $"\"volume\":{s.Volume.ToString("F1", CultureInfo.InvariantCulture)},\"mute\":{(s.Mute ? 1 : 0)}," +
         $"\"save_failed\":{(s.SaveFailed ? 1 : 0)},\"was_finished\":{(s.WasFinished ? 1 : 0)},\"file_size\":{s.FileSize}," +
         $"\"path\":{S(s.Path)},\"title\":{S(s.Title)},\"author\":{S(s.Author)},\"comment\":{S(s.Comment)},\"codec\":{S(s.Codec)}}}");
}

using var input = new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false));
while (input.ReadLine() is { } line)
{
    if (line.Length == 0) continue;
    int space = line.IndexOf(' ');
    string name = space < 0 ? line : line[..space];
    string argument = space < 0 ? "" : line[(space + 1)..];
    double.TryParse(argument, NumberStyles.Float, CultureInfo.InvariantCulture, out double number);
    int.TryParse(argument, NumberStyles.Integer, CultureInfo.InvariantCulture, out int integer);

    if (name == "quit") break;
    switch (name)
    {
        case "status": PrintStatus(false); continue;
        case "chapters": PrintStatus(true); continue;
        case "settle": Settle(); continue;
        case "sleep": Thread.Sleep(integer); continue;
    }
    ErrorKind status = name switch
    {
        "open" => core.Open(argument, out _),
        "close" => core.CloseBook(),
        "play" => core.Play(),
        "pause" => core.Pause(),
        "seek" => core.SeekAbsolute(number),
        "rel" => core.SeekRelative(number),
        "chapter" => core.SelectChapter(integer),
        "next" => core.NextChapter(),
        "prev" => core.PreviousChapter(),
        "vol" => core.SetVolume(number),
        "mute" => core.SetMute(integer != 0),
        _ => ErrorKind.UnknownCommand,
    };
    Reply(name, Code(status));
}

stressStop.Cancel();
stress?.Join();
NativeStatus shutdown = core.Shutdown(3000);
if (shutdown != NativeStatus.Ok)
{
    Reply("shutdown", (int)shutdown);
    return 1;  // the position is already saved; let the process end
}
// Events queued before shutdown returned are still delivered by Dispose; none may arrive after it.
core.Dispose();
Volatile.Write(ref shutdownReturned, 1);
Thread.Sleep(gcStress ? 150 : 50);
if (gcStress)
{
    GC.Collect();
    GC.WaitForPendingFinalizers();
    Line($"{{\"t\":{clock.ElapsedMilliseconds},\"ev\":\"gc\",\"collections\":{collections},\"snapshots\":{abandoned}}}");
}
if (lateEvents != 0)
{
    Log(0, $"{lateEvents} events were raised after the last queued one");
    Reply("shutdown", -10);
    return 3;
}
Reply("shutdown", 0);
return 0;
