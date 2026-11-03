using Google.Protobuf;
using Jupiter.ShowPlayer.Interop.Messages;

namespace Jupiter.ShowPlayer.Interop;

public sealed class AudioCoreException(string message, ErrorKind kind = ErrorKind.Internal) : Exception(message)
{
    public ErrorKind Kind { get; } = kind;
}

public sealed record CoreOptions
{
    /// <summary>Where the resume store lives. Null selects %LOCALAPPDATA%\show-player.</summary>
    public string? DataDirectory { get; init; }

    /// <summary>libmpv-2.dll by absolute path. Null selects the copy beside the application.</summary>
    public string? EnginePath { get; init; }

    /// <summary>The engine's audio output driver. Null selects its default; "null" is silent.</summary>
    public string? AudioOutput { get; init; }

    /// <summary>Milliseconds between saves while playing. Zero selects 5000.</summary>
    public uint SaveIntervalMs { get; init; }
}

/// <summary>
/// Typed access to the Rust playback core. Calls may come from any thread.
/// Playback commands queue work and return at once; their outcome arrives
/// through <see cref="EventsArrived"/>, raised on the core's event thread.
/// <see cref="ListBooks"/> reads the folder and the store, so it belongs off the UI thread.
/// </summary>
public sealed class AudioCore : IDisposable
{
    private readonly CoreSafeHandle _handle;
    private long _nextRequestId;
    private Thread? _pump;
    private bool _stopped;
    private bool _disposed;

    private AudioCore(CoreSafeHandle handle) => _handle = handle;

    /// <summary>
    /// Raised on the event thread with each batch, oldest first. A handler
    /// must be quick, must not throw and must not call <see cref="Shutdown"/>
    /// or <see cref="Dispose"/>. A UI posts the batch to its dispatcher.
    /// </summary>
    public event Action<IReadOnlyList<Event>>? EventsArrived;

    /// <summary>Raised on the event thread if the core becomes unusable.</summary>
    public event Action<string>? Faulted;

    /// <summary>Loads the native core, checks its ABI version, opens the store and starts the engine.</summary>
    public static unsafe AudioCore Create(CoreOptions options)
    {
        uint version;
        try
        {
            version = NativeMethods.ab_abi_version();
        }
        catch (Exception e) when (e is DllNotFoundException or BadImageFormatException or EntryPointNotFoundException)
        {
            throw new AudioCoreException($"The playback core (show_player_ffi.dll) could not be loaded: {e.Message}");
        }
        if (version != NativeMethods.AbiVersion)
        {
            throw new AudioCoreException($"The playback core has ABI version {version}; this app needs {NativeMethods.AbiVersion}.");
        }

        var config = new CoreConfig
        {
            DataDir = options.DataDirectory ?? "",
            EnginePath = options.EnginePath ?? "",
            AudioOutput = options.AudioOutput ?? "",
            SaveIntervalMs = options.SaveIntervalMs,
        }.ToByteArray();
        NativeStatus status;
        CoreSafeHandle core;
        ResultSafeHandle error;
        fixed (byte* ptr = config)
        {
            status = NativeMethods.ab_create(ptr, (nuint)config.Length, out core, out error);
        }
        using (error)
        {
            if (status == NativeStatus.Ok && !core.IsInvalid)
            {
                return new AudioCore(core);
            }
            core.Dispose();
            if (!error.IsInvalid && Response.Parser.ParseFrom(error.CopyBytes()).Error is { } e)
            {
                throw new AudioCoreException(e.Message, e.Kind);
            }
            throw new AudioCoreException($"The playback core could not start ({status}).");
        }
    }

    /// <summary>Starts delivering events. Events raised before this are queued, not lost.</summary>
    public void Start()
    {
        if (_pump is not null)
        {
            return;
        }
        _pump = new Thread(Pump) { IsBackground = true, Name = "show-player events" };
        _pump.Start();
    }

    private void Pump()
    {
        while (true)
        {
            NativeStatus status;
            ResultSafeHandle result;
            try
            {
                status = NativeMethods.ab_wait_events(_handle, 250, out result);
            }
            catch (ObjectDisposedException)
            {
                return;
            }
            using (result)
            {
                if (status == NativeStatus.ShutDown)
                {
                    return;
                }
                if (status != NativeStatus.Ok)
                {
                    Faulted?.Invoke($"The playback core stopped responding ({status}).");
                    return;
                }
                var batch = EventBatch.Parser.ParseFrom(result.CopyBytes());
                if (batch.Events.Count > 0)
                {
                    try
                    {
                        EventsArrived?.Invoke(batch.Events);
                    }
                    catch (Exception)
                    {
                        // A handler must not throw; the pump keeps running if one does.
                    }
                }
            }
        }
    }

    private unsafe Response Call(Request request)
    {
        request.RequestId = (ulong)Interlocked.Increment(ref _nextRequestId);
        var bytes = request.ToByteArray();
        NativeStatus status;
        ResultSafeHandle result;
        fixed (byte* ptr = bytes)
        {
            status = NativeMethods.ab_call(_handle, ptr, (nuint)bytes.Length, out result);
        }
        using (result)
        {
            if (status != NativeStatus.Ok)
            {
                throw new AudioCoreException($"The playback core rejected a call ({status}).");
            }
            var response = Response.Parser.ParseFrom(result.CopyBytes());
            if (response.RequestId != request.RequestId)
            {
                throw new AudioCoreException("The playback core answered a different request.");
            }
            return response;
        }
    }

    private Response Query(Request request)
    {
        var response = Call(request);
        if (response.ResultCase == Response.ResultOneofCase.Error)
        {
            throw new AudioCoreException(response.Error.Message, response.Error.Kind);
        }
        return response;
    }

    /// <summary>Runs a playback command. Returns None, or why it was refused.</summary>
    private ErrorKind Command(Request request) =>
        Call(request) is { ResultCase: Response.ResultOneofCase.Error } response ? response.Error.Kind : ErrorKind.None;

    private static readonly Empty NoArguments = new();

    public ShowDraft InspectShow(string folder, string notesPath = "") =>
        Query(new Request { InspectShow = new InspectShow { Folder = folder, NotesPath = notesPath } }).ShowDraft;
    public IReadOnlyList<Recording> SearchRecordings(string query, string source) =>
        Query(new Request { SearchRecordings = new SearchRecordings { Query = query, Source = source } }).Recordings.Items;
    public ShowJob PackageShow(ShowDraft draft, string output, bool copyAudio) =>
        Query(new Request { PackageShow = new PackageShow { Draft = draft, Output = output, CopyAudio = copyAudio } }).Job;
    public ShowJob TagShow(ShowDraft draft, string folder) =>
        Query(new Request { TagShow = new TagShow { Draft = draft, Folder = folder } }).Job;
    public IReadOnlyList<ShowSummary> ListShows() => Query(new Request { ListShows = NoArguments }).Shows.Shows;
    public ShowDetail GetShow(string id) => Query(new Request { GetShow = new GetShow { Id = id } }).Show;
    public IReadOnlyList<SongEntry> ListSongs(string folder) => Query(new Request { ListSongs = new ListSongs { Folder = folder } }).Songs.Songs;
    public Book DescribeBook(string path) => Query(new Request { DescribeBook = new DescribeBook { Path = path } }).Book;
    public ShowJob GetJob() => Query(new Request { GetJob = NoArguments }).Job;
    public void CancelJob() => Query(new Request { CancelJob = NoArguments });
    public RecordingPriorities GetPriorities() => Query(new Request { GetPriorities = NoArguments }).Priorities;
    public RecordingPriorities SetPriorities(RecordingPriorities priorities) => Query(new Request { SetPriorities = priorities }).Priorities;

    /// <summary>Opens a book. The returned id comes back in its BookLoaded or LoadFailed event.</summary>
    public ErrorKind Open(string path, out ulong requestId)
    {
        var request = new Request { Open = new Open { Path = path } };
        var result = Command(request);
        requestId = request.RequestId;
        return result;
    }

    public ErrorKind CloseBook() => Command(new Request { CloseBook = NoArguments });

    public ErrorKind Play() => Command(new Request { Play = NoArguments });

    public ErrorKind Pause() => Command(new Request { Pause = NoArguments });

    public ErrorKind SeekAbsolute(double seconds) => Command(new Request { SeekAbsolute = new Seek { Seconds = seconds } });

    public ErrorKind SeekRelative(double seconds) => Command(new Request { SeekRelative = new Seek { Seconds = seconds } });

    public ErrorKind SelectChapter(int index) => Command(new Request { ChapterSelect = new ChapterSelect { Index = index } });

    public ErrorKind NextChapter() => Command(new Request { ChapterNext = NoArguments });

    public ErrorKind PreviousChapter() => Command(new Request { ChapterPrevious = NoArguments });

    public ErrorKind SetVolume(double volume) => Command(new Request { SetVolume = new SetVolume { Volume = volume } });

    public ErrorKind SetMute(bool mute) => Command(new Request { SetMute = new SetMute { Mute = mute } });

    /// <summary>A consistent copy of the player's state; chapters only when asked for.</summary>
    public Snapshot GetSnapshot(bool includeChapters = false) =>
        Query(new Request { GetSnapshot = new GetSnapshot { IncludeChapters = includeChapters } }).Snapshot;

    /// <summary>The books in a folder, by title, with their saved progress.</summary>
    public IReadOnlyList<Book> ListBooks(string folder) =>
        Query(new Request { ListBooks = new ListBooks { Folder = folder } }).Books.Books;

    /// <summary>Books played most recently first.</summary>
    public IReadOnlyList<Book> RecentBooks(uint limit = 10) =>
        Query(new Request { RecentBooks = new RecentBooks { Limit = limit } }).Books.Books;

    /// <summary>
    /// Saves the position, stops the engine and closes the store. No event is
    /// queued after a successful return; the remaining ones are still delivered.
    /// On <see cref="NativeStatus.Timeout"/> the position is already saved and the process may exit.
    /// </summary>
    public NativeStatus Shutdown(int timeoutMs)
    {
        if (_stopped)
        {
            return NativeStatus.Ok;
        }
        var status = NativeMethods.ab_shutdown(_handle, timeoutMs);
        if (status == NativeStatus.Ok)
        {
            _stopped = true;
        }
        return status;
    }

    /// <summary>Shuts down if needed, waits for the last events to be delivered and frees the core.</summary>
    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        Shutdown(5000);
        if (Thread.CurrentThread != _pump)
        {
            _pump?.Join(_stopped ? Timeout.Infinite : 1000);
        }
        // A core that did not stop is left running: its thread still uses it,
        // and the process is about to end.
        if (_stopped)
        {
            _handle.Dispose();
        }
    }
}
