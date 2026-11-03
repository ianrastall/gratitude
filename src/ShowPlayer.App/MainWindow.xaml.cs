using System.Reflection;
using System.Runtime.InteropServices;
using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;
using Microsoft.UI;
using Microsoft.UI.Input;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.Windows.Storage.Pickers;
using Windows.Graphics;
using Windows.System;

namespace Jupiter.ShowPlayer.App;

/// <summary>
/// One row of the chapter list: a chapter (Index is its chapter), a set heading, or a setlist
/// song this recording does not hold (Index -1 for both).
/// </summary>
public sealed record ChapterRow(int Index, string Number, string Title, string Length, bool Heading = false)
{
    public Visibility HeadingVisibility => Heading ? Visibility.Visible : Visibility.Collapsed;
    public Visibility RowVisibility => Heading ? Visibility.Collapsed : Visibility.Visible;

    // What a screen reader announces for the row.
    public override string ToString() => Heading ? Title : Index < 0 ? $"{Title}, {Length}" : $"{Number}. {Title}, {Length}";
}

/// <summary>One BerthaML show in the All shows list.</summary>
public sealed record ShowRow(string Id, string Date, string Place, string Detail, string Owned)
{
    public override string ToString() => $"{Date}, {Place}, {Detail} {Owned}";
}

/// <summary>One song in the Songs list.</summary>
public sealed record SongRow(string Id, string Title, string Detail)
{
    public override string ToString() => $"{Title}, {Detail}";
}

/// <summary>A performance of a song held by a chapter of a show in the folder.</summary>
public sealed record PerformanceRow(string Path, int Chapter, string Heading, string Detail)
{
    public override string ToString() => $"{Heading}, {Detail}";
}

/// <summary>One row of the Books pane.</summary>
public sealed record BookRow(string Path, string Heading, string Place, string Detail, string SourceInfo, string Credits)
{
    public override string ToString() => string.Join(", ", new[] { Heading, Place, Credits, Detail }.Where(t => t.Length > 0));

    /// <summary>A card for a book: the show's date and place when linked, else its own title.</summary>
    public static BookRow From(Book b, string detail) => new(b.Path, BookHeading(b), b.ShowDate.Length > 0 ? b.Place : "", detail, b.SourceSummary, b.Credits);

    public static string BookHeading(Book b) => b.ShowDate.Length > 0 ? b.ShowDate : b.Title;

    public string FileName => System.IO.Path.GetFileName(Path);
    public Visibility PlaceVisibility => Place.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
    public Visibility CreditsVisibility => Credits.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
}

/// <summary>
/// The player window. It shows what the core reports and sends it commands;
/// it computes no positions itself.
/// </summary>
public sealed partial class MainWindow : Window
{
    private static readonly string[] OpenTypes = [".mka"];
    private const double VolumeStep = 5;

    private readonly StartupOptions _options;
    private readonly AppSettings _settings;
    private readonly AudioCore? _core;
    private readonly string? _startupError;
    private SettingsWindow? _settingsWindow;
    private WorkshopWindow? _workshop;

    public void OpenShow(string path) => OpenBook(path);
    public void OpenAppearanceSettings() => OnSettings(this, new RoutedEventArgs());
    private void OpenWorkshop(string page)
    {
        if (_core is null) return;
        if (_workshop is null)
        {
            _workshop = new WorkshopWindow(this, _core, page);
            _workshop.Closed += (_, _) => _workshop = null;
        }
        else _workshop.Navigate(page);
        _workshop.Activate();
    }
    private void OnImportShow(object sender, RoutedEventArgs e) => OpenWorkshop("import");
    private void OnShowJob(object sender, RoutedEventArgs e) => OpenWorkshop("job");
    private void OnRecordingPriorities(object sender, RoutedEventArgs e) => PrioritiesControl.OpenEditor();
    private void OnPriorityChange() => _ = RefreshBooksAsync();

    // A projection of the core's state, kept from its events and snapshots.
    private ulong _generation;          // newest generation seen in an event
    private ulong _chaptersGeneration;  // generation the chapter list belongs to
    private ulong _startupRequest;      // the open issued at startup
    private ulong _playRequest;         // the open that plays once loaded
    private PlaybackState _state = PlaybackState.Idle;
    private int _chapter = -1;
    private double _position, _duration, _volume = 100;
    private bool _mute, _saveFailed, _wasFinished;
    private string _bookPath = "", _bookTitle = "", _codec = "", _lastBook = "";
    private string _bookHeading = "", _bookPlace = "";  // the linked show's date and place
    private long _fileSize;
    private List<Chapter> _chapters = [];
    private List<ChapterRow> _chapterRows = [];
    private Book? _bookLink;            // the open book's BerthaML link
    private string _bookReport = "";    // and its standard report
    private ShowDetail? _bookShow;      // and its show, for the setlist
    private int _linkRequest;           // the newest link asked for; older answers are dropped
    private int _playChapter = -1;      // chapter to start at once the requested open loads
    private string _area = "player";
    private List<ShowSummary>? _allShows;
    private List<SongEntry>? _songs;
    private string? _songsProblem;
    private List<Book> _books = [];
    private string? _booksProblem;
    private int _booksRequest;          // the newest book list asked for; older answers are dropped
    private string? _storeNotice;
    private string? _dismissedNotice;
    private bool _settingSlider;
    private long _sliderTouched;        // tick count of the listener's last move of the position slider
    private bool _closed;

    public MainWindow(StartupOptions options)
    {
        _options = options;
        _settings = App.Current.Settings;
        InitializeComponent();
        App.Current.Accent.Attach(Root);
        App.Current.Theme.Attach(this);
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "AppIcon.ico"));
        Closed += OnClosed;
        Notice.CloseButtonClick += (_, _) => _dismissedNotice = NoticeKey();
        RestoreBounds();
        ApplyBooksPane();
        ShowSkipLabels();

        try
        {
            _core = AudioCore.Create(new CoreOptions { DataDirectory = options.DataDirectory, AudioOutput = options.AudioOutput });
        }
        catch (AudioCoreException e)
        {
            _startupError = e.Message;
            AppLog.Write($"The player could not start: {e.Message}");
            OpenItem.IsEnabled = EmptyOpenButton.IsEnabled = VolumeSlider.IsEnabled = MuteButton.IsEnabled = false;
            ShowAll();
            return;
        }

        // Events arrive on the core's event thread. The only thing done there
        // is handing them to the UI thread.
        _core.EventsArrived += batch => DispatcherQueue.TryEnqueue(() => OnCoreEvents(batch));
        _core.Faulted += message => DispatcherQueue.TryEnqueue(() =>
        {
            AppLog.Write(message);
            _storeNotice = message;
            ShowNotice();
        });

        Snapshot initial = _core.GetSnapshot();
        _lastBook = initial.LastBook;
        _volume = initial.Volume;
        _mute = initial.Mute;
        _core.Start();
        PrioritiesControl.Configure(_core);
        App.Current.RecordingPrioritiesChanged += OnPriorityChange;

        // With no book named on the command line, carry on with the last one.
        string? startup = options.OpenPath ?? (_lastBook.Length > 0 ? _lastBook : null);
        if (startup is not null)
        {
            _core.Open(startup, out _startupRequest);
        }
        ShowAll();
        _ = RefreshBooksAsync();
        _ = RefreshRecentAsync();
    }

    public FrameworkElement RootElement => Root;

    /// <summary>Brought forward by a second launch.</summary>
    public void ShowAndActivate()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }
        AppWindow.Show();
        Activate();
        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    // ------------------------------------------------------------------
    // Core events

    private void OnCoreEvents(IReadOnlyList<Event> batch)
    {
        if (_closed || _core is null)
        {
            return;
        }
        bool reread = false, reloadChapters = false, loaded = false, refreshBooks = false;
        foreach (Event e in batch)
        {
            if (e.Type == EventType.Log)
            {
                AppLog.Write($"core [{e.LogLevel}] {e.Message}");
                if (e.Message.StartsWith("The saved positions were damaged", StringComparison.Ordinal))
                {
                    _storeNotice = e.Message;
                }
                continue;
            }
            if (e.Generation < _generation)
            {
                continue;  // about a book that has since been replaced
            }
            _generation = e.Generation;
            _state = e.State;
            _chapter = e.Chapter;
            _position = e.Position;
            _duration = e.Duration;
            switch (e.Type)
            {
                case EventType.VolumeChanged:
                    _volume = e.Volume;
                    _mute = e.Mute;
                    break;
                case EventType.Position or EventType.ChapterChanged:
                    break;
                case EventType.BookLoaded:
                    reread = reloadChapters = loaded = true;
                    if (e.RequestId == _playRequest)
                    {
                        if (_playChapter >= 0)
                        {
                            _core.SelectChapter(_playChapter);
                            _playChapter = -1;
                        }
                        _core.Play();
                    }
                    if (e.RequestId == _startupRequest)
                    {
                        if (_options.SeekTo is { } seconds)
                        {
                            _core.SeekAbsolute(seconds);
                        }
                        if (_options.AutoPlay)
                        {
                            _core.Play();
                        }
                    }
                    break;
                case EventType.LoadFailed or EventType.Finished:
                    reread = refreshBooks = true;
                    break;
                case EventType.StateChanged:
                    reread = true;
                    // A pause has just saved the position.
                    refreshBooks |= e.State is PlaybackState.Paused or PlaybackState.Idle;
                    break;
                default:
                    reread = true;
                    break;
            }
        }
        if (reread)
        {
            ReadSnapshot(reloadChapters || _generation != _chaptersGeneration);
        }
        if (loaded)
        {
            // Keyboard focus on the main control once there is something to play.
            PlayPauseButton.Focus(FocusState.Programmatic);
            refreshBooks = true;
            _ = RefreshRecentAsync();
            _ = LoadLinkAsync(_bookPath);
        }
        ShowAll();
        if (refreshBooks)
        {
            _ = RefreshBooksAsync();
        }
    }

    private void ReadSnapshot(bool reloadChapters)
    {
        Snapshot s;
        try
        {
            s = _core!.GetSnapshot(includeChapters: reloadChapters);
        }
        catch (AudioCoreException e)
        {
            AppLog.Write($"Reading the player state failed: {e.Message}");
            return;
        }
        if (s.Generation != _generation)
        {
            return;  // already superseded; its own events follow
        }
        _lastBook = s.LastBook;
        _bookPath = s.Path;
        _saveFailed = s.SaveFailed;
        _wasFinished = s.WasFinished;
        _codec = s.Codec;
        _fileSize = s.FileSize;
        _bookTitle = s.Title.Length > 0 ? s.Title : s.Path.Length > 0 ? Path.GetFileNameWithoutExtension(s.Path) : "";
        AuthorText.Text = _bookPlace.Length > 0 ? _bookPlace : s.Author;
        if (!reloadChapters || s.State == PlaybackState.Loading)
        {
            return;
        }
        _chaptersGeneration = s.Generation;
        _chapters = [.. s.Chapters];
        ShowChapters();
    }

    /// <summary>
    /// The chapters, laid out on the show's setlist when the book is linked to one: set
    /// headings, and setlist songs this recording does not hold in their places.
    /// </summary>
    private void ShowChapters()
    {
        var rows = new List<ChapterRow>();
        var setlist = _bookShow?.Setlist;
        var held = _bookLink is { } link && SamePath(link.Path, _bookPath) ? link.ChapterPerformances : null;
        bool laidOut = setlist is { Count: > 0 } && held is not null && held.Count == _chapters.Count && held.Any(h => h.Length > 0);
        string set = "";
        int next = 0;
        void Heading(string label)
        {
            if (label.Length > 0 && label != set)
            {
                set = label;
                rows.Add(new ChapterRow(-1, "", label, "", Heading: true));
            }
        }
        void Missing(int until)
        {
            for (; next < until; next++)
            {
                Heading(setlist![next].Set);
                rows.Add(new ChapterRow(-1, "", setlist[next].Title, "Not in this recording"));
            }
        }
        // Chapters ahead of the first placed one, such as an opening jam or tuning, go under
        // that chapter's set, after any setlist songs the recording starts too late for.
        if (laidOut && held![0].Length == 0)
        {
            string? firstHeld = held.Select(h => h.Split(' ', StringSplitOptions.RemoveEmptyEntries).FirstOrDefault()).FirstOrDefault(id => id is not null);
            int first = firstHeld is null ? -1 : setlist!.ToList().FindIndex(e => e.Id == firstHeld);
            if (first >= 0)
            {
                Missing(first);
                Heading(setlist![first].Set);
            }
        }
        for (int i = 0; i < _chapters.Count; i++)
        {
            if (laidOut)
            {
                string[] ids = held![i].Split(' ', StringSplitOptions.RemoveEmptyEntries);
                int first = ids.Length > 0 ? setlist!.ToList().FindIndex(e => e.Id == ids[0]) : -1;
                if (first >= 0)
                {
                    if (first >= next)
                    {
                        Missing(first);
                    }
                    Heading(setlist![first].Set);
                    next = Math.Max(next, setlist.ToList().FindIndex(e => e.Id == ids[^1]) + 1);
                }
            }
            Chapter c = _chapters[i];
            rows.Add(new ChapterRow(i, $"{i + 1}", c.Title.Length > 0 ? c.Title : $"Chapter {i + 1}", Clock(c.End - c.Start)));
        }
        if (laidOut)
        {
            Missing(setlist!.Count);
        }
        _chapterRows = rows;
        ChapterList.ItemsSource = rows;
        string count = _chapters.Count == 1 ? "1 chapter" : $"{_chapters.Count} chapters";
        ChaptersHeading.Text = laidOut ? $"Setlist · {count}" : count;
        SelectCurrentChapter(force: true);
    }

    /// <summary>Reads the open book's BerthaML link and its show, then lays the chapters on the setlist.</summary>
    private async Task LoadLinkAsync(string path)
    {
        if (_core is null || path.Length == 0)
        {
            return;
        }
        int request = ++_linkRequest;
        AudioCore core = _core;
        Book? link = null;
        ShowDetail? show = null;
        string report = "";
        try
        {
            (link, show, report) = await Task.Run(() =>
            {
                Book book = core.DescribeBook(path);
                return (book, book.ShowId.Length > 0 ? core.GetShow(book.ShowId) : null, core.BookReport(path));
            });
        }
        catch (AudioCoreException e)
        {
            AppLog.Write($"The show's BerthaML link could not be read: {e.Message}");
        }
        if (request != _linkRequest || _closed || !SamePath(path, _bookPath))
        {
            return;
        }
        _bookLink = link;
        _bookShow = show;
        _bookReport = report;
        PlaybackNotes.Text = report;
        ExportReportItem.IsEnabled = report.Length > 0;
        _bookHeading = link is { ShowDate.Length: > 0 } ? link.ShowDate : "";
        _bookPlace = link?.Place ?? "";
        if (_bookPlace.Length > 0)
        {
            AuthorText.Text = _bookPlace;
        }
        var facts = new List<string>();
        if (link is { SourceSummary.Length: > 0 }) facts.Add(link.SourceSummary.Replace(" | ", " · "));
        if (link is { Credits.Length: > 0 }) facts.Add(link.Credits);
        if (show is not null) facts.Add($"BerthaML {show.Summary.Id}, {show.Summary.Status} record");
        ShowLinkText.Text = string.Join(" · ", facts);
        ShowHeading();
        ToolTipService.SetToolTip(ShowLinkText, link?.Link);
        ShowChapters();
    }

    // ------------------------------------------------------------------
    // Showing the state

    private bool HasBook => _bookPath.Length > 0 && _state != PlaybackState.Idle;

    private bool Playable => _state is PlaybackState.Paused or PlaybackState.Playing or PlaybackState.Finished;

    private void ShowAll()
    {
        ShowHeading();
        ShowTransport();
        ShowVolume();
        ShowStatus();
        ShowNotice();
    }

    private void ShowHeading()
    {
        bool hasBook = HasBook;
        bool open = hasBook && _state is not (PlaybackState.Missing or PlaybackState.Failed);
        EmptyState.Visibility = hasBook ? Visibility.Collapsed : Visibility.Visible;
        EmptyText.Text = _startupError is not null
            ? "Playback is unavailable."
            : "Choose a show in the Shows pane, or open a file.";
        BookHeading.Visibility = hasBook ? Visibility.Visible : Visibility.Collapsed;
        ChapterArea.Visibility = PositionRow.Visibility = TransportRow.Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        TitleText.Text = _bookHeading.Length > 0 ? _bookHeading : _bookTitle;
        ToolTipService.SetToolTip(TitleText, _bookPath);
        CloseBookItem.IsEnabled = hasBook;
        // Standard section 6: "Document name - App name" while a book is open.
        Title = open && _state != PlaybackState.Loading ? $"{(_bookHeading.Length > 0 ? _bookHeading : _bookTitle)} - Gratitude" : "Gratitude";
    }

    private void ShowTransport()
    {
        bool playable = Playable;
        bool canToggle = _state is PlaybackState.Paused or PlaybackState.Playing;
        PlayPauseButton.IsEnabled = PlayPauseItem.IsEnabled = canToggle;
        bool playing = _state == PlaybackState.Playing;
        PlayPauseLabel.Text = PlayPauseItem.Text = playing ? "Pause" : "Play";
        PlayPauseIcon.Glyph = playing ? "\uE769" : "\uE768";
        AutomationNameFor(PlayPauseButton, playing ? "Pause" : "Play");
        PreviousButton.IsEnabled = NextButton.IsEnabled = BackButton.IsEnabled = ForwardButton.IsEnabled = PositionSlider.IsEnabled = playable;
        PreviousItem.IsEnabled = NextItem.IsEnabled = BackItem.IsEnabled = ForwardItem.IsEnabled = playable;
        SelectCurrentChapter();

        Chapter? current = CurrentChapter();
        double length = current is null ? 0 : current.End - current.Start;
        double within = current is null ? 0 : Math.Clamp(_position - current.Start, 0, length);
        ElapsedText.Text = Clock(within);
        ChapterLengthText.Text = Clock(length);

        // While the listener is dragging the thumb, reports a moment behind must not pull it back.
        if (Environment.TickCount64 - _sliderTouched < 500 && PositionSlider.Maximum == Math.Max(length, 1))
        {
            return;
        }
        _settingSlider = true;
        PositionSlider.Maximum = Math.Max(length, 1);
        PositionSlider.Value = within;
        _settingSlider = false;
    }

    private Chapter? CurrentChapter() =>
        _chapter >= 0 && _chapter < _chapters.Count && _chaptersGeneration == _generation ? _chapters[_chapter] : null;

    private void SelectCurrentChapter(bool force = false)
    {
        int chapter = _chaptersGeneration == _generation && _chapter < _chapters.Count ? _chapter : -1;
        int index = chapter < 0 ? -1 : _chapterRows.FindIndex(r => r.Index == chapter);
        if (ChapterList.SelectedIndex == index && !force)
        {
            return;
        }
        ChapterList.SelectedIndex = index;
        if (index >= 0)
        {
            ChapterList.ScrollIntoView(_chapterRows[index]);
        }
    }

    private void ShowVolume()
    {
        _settingSlider = true;
        VolumeSlider.Value = _volume;
        _settingSlider = false;
        VolumeText.Text = $"{_volume:0}%";
        MuteButton.IsChecked = MuteItem.IsChecked = _mute;
        MuteIcon.Glyph = _mute ? "\uE74F" : "\uE767";
        AutomationNameFor(MuteButton, _mute ? "Unmute" : "Mute");
    }

    private void ShowStatus()
    {
        string text = _state switch
        {
            PlaybackState.Idle => _startupError is null ? "No show open" : "Playback is unavailable",
            PlaybackState.Loading => "Opening…",
            PlaybackState.Missing => "File not found",
            PlaybackState.Failed => "Could not be played",
            PlaybackState.Finished => $"Finished · {Clock(_duration)}",
            _ => $"{(_state == PlaybackState.Playing ? "Playing" : "Paused")} · Chapter {_chapter + 1} of {_chapters.Count}"
                 + $" · {Clock(_position)} of {Clock(_duration)}",
        };
        if (_wasFinished && _state == PlaybackState.Paused && _position < 1)
        {
            text += " · Finished before";
        }
        StatusText.Text = text;
        DetailText.Text = HasBook && _codec.Length > 0 ? $"{_codec} · {Size(_fileSize)}" : "";
    }

    /// <summary>The most important problem, in an InfoBar at the top of the content (standard section 10).</summary>
    private (InfoBarSeverity Severity, string Title, string Message)? CurrentNotice()
    {
        if (_startupError is not null)
        {
            return (InfoBarSeverity.Error, "The player could not start",
                $"{_startupError} Reinstalling Gratitude restores its files; your saved positions are kept.");
        }
        if (_state == PlaybackState.Missing)
        {
            return (InfoBarSeverity.Warning, "The show's file could not be found",
                $"{_bookPath} is not there. Your place in it is kept for when the file returns.");
        }
        if (_state == PlaybackState.Failed)
        {
            return (InfoBarSeverity.Error, "The show could not be played",
                "The file may be incomplete or unreadable. Your place in it is kept.");
        }
        if (_saveFailed)
        {
            return (InfoBarSeverity.Warning, "Your place is not being saved",
                "The saved positions could not be written. Playback continues, and saving is retried.");
        }
        if (_storeNotice is not null)
        {
            return (InfoBarSeverity.Warning, "Saved positions", _storeNotice);
        }
        return null;
    }

    private string? NoticeKey() => CurrentNotice() is { } n ? $"{n.Title}|{n.Message}" : null;

    private void ShowNotice()
    {
        if (CurrentNotice() is not { } notice || NoticeKey() == _dismissedNotice)
        {
            Notice.IsOpen = false;
            return;
        }
        Notice.Severity = notice.Severity;
        Notice.Title = notice.Title;
        Notice.Message = notice.Message;
        Notice.IsOpen = true;
    }

    private static void AutomationNameFor(DependencyObject element, string name) =>
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(element, name);

    private static string Clock(double seconds)
    {
        var time = TimeSpan.FromSeconds(Math.Max(0, Math.Floor(seconds)));
        return time.TotalHours >= 1 ? $"{(int)time.TotalHours}:{time:mm\\:ss}" : $"{time.Minutes}:{time:ss}";
    }

    private static string Size(long bytes) => bytes switch
    {
        >= 1L << 30 => $"{bytes / (double)(1L << 30):0.0} GB",
        >= 1L << 20 => $"{bytes / (double)(1L << 20):0} MB",
        _ => $"{bytes / 1024.0:0} KB",
    };

    // ------------------------------------------------------------------
    // Books pane and recent books

    /// <summary>The chosen folder; failing that, wherever the last book came from.</summary>
    private string? LibraryFolder()
    {
        if (_settings.LibraryFolder is { Length: > 0 } chosen)
        {
            return chosen;
        }
        string? folder = _lastBook.Length > 0 ? Path.GetDirectoryName(_lastBook) : null;
        return folder is not null && Directory.Exists(folder) ? folder : null;
    }

    internal async Task RefreshBooksAsync()
    {
        string? folder = LibraryFolder();
        FolderText.Text = folder ?? "No shows folder chosen";
        ToolTipService.SetToolTip(FolderText, folder);
        int request = ++_booksRequest;
        if (_core is null || folder is null)
        {
            _books = [];
            _booksProblem = folder is null ? "Choose the folder that holds your shows." : null;
            ShowBooks();
            return;
        }
        AudioCore core = _core;
        try
        {
            var books = await Task.Run(() => core.ListBooks(folder));
            if (request != _booksRequest || _closed)
            {
                return;
            }
            _books = [.. books];
            _booksProblem = null;
            if (_songs is not null)
            {
                _ = RefreshSongsAsync();
            }
        }
        catch (AudioCoreException e)
        {
            if (request != _booksRequest || _closed)
            {
                return;
            }
            _books = [];
            _booksProblem = $"The folder could not be read. {e.Message}";
        }
        ShowBooks();
    }

    private void ShowBooks()
    {
        string search = BookSearch.Text.Trim();
        var rows = _books
            .Where(b => search.Length == 0 || new[] { b.Title, b.ShowDate, b.Date, b.Place, b.Credits }.Any(t => t.Contains(search, StringComparison.CurrentCultureIgnoreCase)))
            .Select(b => BookRow.From(b, BookDetail(b)))
            .ToList();
        BookList.ItemsSource = rows;
        bool empty = rows.Count == 0;
        BookList.Visibility = empty ? Visibility.Collapsed : Visibility.Visible;
        BooksEmpty.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;
        BooksEmptyText.Text = _booksProblem
            ?? (search.Length > 0 && _books.Count > 0
                ? $"No shows match \u201C{search}\u201D."
                : "No shows in this folder. The Shows pane lists .mka files.");
        int index = rows.FindIndex(r => SamePath(r.Path, _bookPath));
        BookList.SelectedIndex = index;
        if (_allShows is not null)
        {
            ShowShows();
        }
    }

    private string BookDetail(Book b)
    {
        if (!b.Exists)
        {
            return "File missing";
        }
        if (SamePath(b.Path, _bookPath) && HasBook)
        {
            return "Open now";
        }
        if (b.Finished)
        {
            return "Finished";
        }
        if (b.PositionMs > 0 && b.DurationMs > 0)
        {
            double percent = Math.Floor(Math.Clamp((double)b.PositionMs / b.DurationMs, 0, 1) * 100);
            string share = percent < 1 ? "Under 1%" : $"{percent:0}%";
            return $"{share} · {Clock((b.DurationMs - b.PositionMs) / 1000.0)} left";
        }
        return "Not started";
    }

    private static bool SamePath(string a, string b) => string.Equals(a, b, StringComparison.OrdinalIgnoreCase);

    private async Task RefreshRecentAsync()
    {
        if (_core is null)
        {
            return;
        }
        AudioCore core = _core;
        IReadOnlyList<Book> recent;
        try
        {
            recent = await Task.Run(() => core.RecentBooks(10));
        }
        catch (AudioCoreException e)
        {
            AppLog.Write($"Recent shows could not be read: {e.Message}");
            return;
        }
        if (_closed)
        {
            return;
        }
        RecentMenu.Items.Clear();
        foreach (Book book in recent)
        {
            string label = book.ShowDate.Length > 0 && book.Place.Length > 0 ? $"{book.Date} · {book.Place}" : BookRow.BookHeading(book);
            var item = new MenuFlyoutItem { Text = $"{label} ({book.SourceSummary.Replace(" | ", ", ")})", Tag = book.Path, IsEnabled = book.Exists };
            ToolTipService.SetToolTip(item, $"{book.Credits}\n{book.Path}");
            item.Click += (_, _) => OpenBook(book.Path);
            RecentMenu.Items.Add(item);
        }
        if (recent.Count == 0)
        {
            RecentMenu.Items.Add(new MenuFlyoutItem { Text = "No recent shows", IsEnabled = false });
        }
    }

    private void OnBookSearchChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args) => ShowBooks();

    private void OnBookClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is BookRow row)
        {
            PlayBook(row.Path);
        }
    }

    /// <summary>
    /// A click on a show plays it. The last show reopens paused at launch, so opening
    /// alone would leave a click on it with nothing to do.
    /// </summary>
    private void PlayBook(string path)
    {
        if (_core is null)
        {
            return;
        }
        if (!SamePath(path, _bookPath) || _state is not (PlaybackState.Paused or PlaybackState.Playing or PlaybackState.Loading))
        {
            ForgetChapters();
            _core.Open(path, out _playRequest);
        }
        else if (_state == PlaybackState.Paused)
        {
            _core.Play();
        }
    }

    private void OnRefreshBooks(object sender, RoutedEventArgs e) => _ = RefreshBooksAsync();

    private async void OnChooseFolder(object sender, RoutedEventArgs e)
    {
        if (await PickFolderAsync(AppWindow.Id, LibraryFolder()) is { } folder)
        {
            _settings.LibraryFolder = folder;
            _settings.Save();
            await RefreshBooksAsync();
        }
    }

    internal static async Task<string?> PickFolderAsync(WindowId owner, string? start)
    {
        var picker = new FolderPicker(owner);
        if (start is not null && Directory.Exists(start))
        {
            picker.SuggestedFolder = start;
        }
        PickFolderResult? result = await picker.PickSingleFolderAsync();
        return result?.Path;
    }

    /// <summary>Opens a book unless it is already the open one.</summary>
    private void OpenBook(string path)
    {
        if (_core is null)
        {
            return;
        }
        if (!SamePath(path, _bookPath) || _state is PlaybackState.Missing or PlaybackState.Failed or PlaybackState.Idle)
        {
            ForgetChapters();
            _core.Open(path, out _);
        }
    }

    private void ForgetChapters()
    {
        _chapters = [];
        _chapterRows = [];
        _bookLink = null;
        _bookShow = null;
        _bookReport = "";
        PlaybackNotes.Text = "";
        ExportReportItem.IsEnabled = false;
        ShowLinkText.Text = "";
        _bookHeading = "";
        _bookPlace = "";
        ChapterList.ItemsSource = null;
    }

    /// <summary>Plays a book from the start of one chapter, opening it first when needed.</summary>
    private void PlayChapter(string path, int chapter)
    {
        if (_core is null)
        {
            return;
        }
        ShowArea("player");
        if (SamePath(path, _bookPath) && _state is PlaybackState.Paused or PlaybackState.Playing or PlaybackState.Finished)
        {
            _core.SelectChapter(chapter);
            _core.Play();
            return;
        }
        ForgetChapters();
        _playChapter = chapter;
        _core.Open(path, out _playRequest);
    }

    private async void OnOpenFile(object sender, RoutedEventArgs e)
    {
        if (_core is null)
        {
            return;
        }
        var picker = new FileOpenPicker(AppWindow.Id);
        if (LibraryFolder() is { } folder)
        {
            picker.SuggestedFolder = folder;
        }
        foreach (string type in OpenTypes)
        {
            picker.FileTypeFilter.Add(type);
        }
        PickFileResult? file = await picker.PickSingleFileAsync();
        if (file is not null && !_closed)
        {
            OpenBook(file.Path);
        }
    }

    private void OnCloseBook(object sender, RoutedEventArgs e) => _core?.CloseBook();

    /// <summary>Saves the open show's standard report as a text file.</summary>
    private async void OnExportReport(object sender, RoutedEventArgs e)
    {
        if (_bookReport.Length == 0)
        {
            return;
        }
        string report = _bookReport;
        var picker = new FileSavePicker(AppWindow.Id) { SuggestedFileName = $"{Path.GetFileNameWithoutExtension(_bookPath)} report" };
        picker.FileTypeChoices.Add("Text", [".txt"]);
        if (Path.GetDirectoryName(_bookPath) is { } folder && Directory.Exists(folder))
        {
            picker.SuggestedFolder = folder;
        }
        if (await picker.PickSaveFileAsync() is not { } file || _closed)
        {
            return;
        }
        try
        {
            await File.WriteAllTextAsync(file.Path, report);
            StatusText.Text = $"Report saved to {file.Path}";
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            _storeNotice = $"The report could not be saved. {ex.Message}";
            ShowNotice();
        }
    }

    // ------------------------------------------------------------------
    // Navigation: Player, All shows, Songs, and Settings in the footer

    private void OnNavigationChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected)
        {
            // Settings is its own window (standard section 11); the area stays as it was.
            OpenSettings();
            ShowArea(_area);
            return;
        }
        ShowArea((args.SelectedItemContainer?.Tag as string) ?? "player");
    }

    private void ShowArea(string area)
    {
        _area = area;
        Workspace.Visibility = area == "player" ? Visibility.Visible : Visibility.Collapsed;
        ShowsArea.Visibility = area == "shows" ? Visibility.Visible : Visibility.Collapsed;
        SongsArea.Visibility = area == "songs" ? Visibility.Visible : Visibility.Collapsed;
        object item = area switch { "shows" => ShowsItem, "songs" => SongsItem, _ => PlayerItem };
        if (!ReferenceEquals(Navigation.SelectedItem, item))
        {
            Navigation.SelectedItem = item;
        }
        if (area == "shows" && _allShows is null)
        {
            _ = LoadShowsAsync();
        }
        if (area == "songs" && _songs is null)
        {
            _ = RefreshSongsAsync();
        }
    }

    // The pane is compact (48) below 1440 wide, where the player needs the room for its
    // transport labels beside the Shows pane, and open (224) above. Auto mode does this
    // itself but can draw an open pane with compact items when layout settles late, as it
    // does with a real audio device, so Left mode is used and the pane is opened or closed
    // here: only once the control has loaded, and only when the width crosses the line, so
    // the pane button keeps working.
    private const double WideNavigationWidth = 1440;
    private bool? _wideNavigation;

    private void OnNavigationLoaded(object sender, RoutedEventArgs e) => FitNavigationPane();

    private void OnNavigationSizeChanged(object sender, SizeChangedEventArgs e) => FitNavigationPane();

    private void FitNavigationPane()
    {
        if (!Navigation.IsLoaded)
        {
            return;
        }
        bool wide = Navigation.ActualWidth >= WideNavigationWidth;
        if (wide == _wideNavigation)
        {
            return;
        }
        _wideNavigation = wide;
        DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () => Navigation.IsPaneOpen = wide);
    }

    private void OnShowPlayerArea(object sender, RoutedEventArgs e) => ShowArea("player");

    private void OnShowShowsArea(object sender, RoutedEventArgs e) => ShowArea("shows");

    private void OnShowSongsArea(object sender, RoutedEventArgs e) => ShowArea("songs");

    // ------------------------------------------------------------------
    // All shows

    private async Task LoadShowsAsync()
    {
        if (_core is null)
        {
            ShowsCount.Text = "The show database is unavailable while the player cannot start.";
            return;
        }
        AudioCore core = _core;
        ShowsCount.Text = "Reading the show database…";
        try
        {
            _allShows = [.. await Task.Run(core.ListShows)];
        }
        catch (AudioCoreException e)
        {
            ShowsCount.Text = $"The show database could not be read. {e.Message}";
            return;
        }
        if (_closed)
        {
            return;
        }
        var years = _allShows.Select(s => s.Date[..4]).Distinct().Order().ToList();
        YearFilter.Items.Clear();
        YearFilter.Items.Add("All years");
        foreach (string year in years)
        {
            YearFilter.Items.Add(year);
        }
        YearFilter.SelectedIndex = 0;
        ShowShows();
    }

    private void ShowShows()
    {
        if (_allShows is null)
        {
            return;
        }
        var owned = _books.Where(b => b.Exists && b.ShowId.Length > 0).GroupBy(b => b.ShowId).ToDictionary(g => g.Key, g => g.Count());
        string search = ShowSearch.Text.Trim();
        string year = YearFilter.SelectedItem as string ?? "All years";
        bool ownedOnly = OwnedShowsOnly.IsChecked == true;
        string? selected = (ShowList.SelectedItem as ShowRow)?.Id;
        var rows = _allShows
            .Where(s => year == "All years" || s.Date.StartsWith(year, StringComparison.Ordinal))
            .Where(s => !ownedOnly || owned.ContainsKey(s.Id))
            .Where(s => search.Length == 0
                || s.Date.Contains(search, StringComparison.OrdinalIgnoreCase)
                || s.Place.Contains(search, StringComparison.CurrentCultureIgnoreCase)
                || s.Id.Contains(search, StringComparison.OrdinalIgnoreCase))
            .Select(s => new ShowRow(s.Id, s.Date, s.Place, ShowDetailText(s), owned.TryGetValue(s.Id, out int n) ? (n == 1 ? "In folder" : $"{n} in folder") : ""))
            .ToList();
        ShowList.ItemsSource = rows;
        int ownedCount = _allShows.Count(s => owned.ContainsKey(s.Id));
        ShowsCount.Text = $"{rows.Count:N0} of {_allShows.Count:N0} shows · {ownedCount:N0} in the shows folder";
        ShowList.SelectedIndex = selected is null ? -1 : rows.FindIndex(r => r.Id == selected);
    }

    private static string ShowDetailText(ShowSummary s)
    {
        string songs = s.Songs == 0 ? "no setlist" : s.Songs == 1 ? "1 song" : $"{s.Songs} songs";
        string tapes = s.Recordings == 1 ? "1 listed recording" : $"{s.Recordings} listed recordings";
        return $"{songs} · {tapes} · {s.Status}";
    }

    private void OnShowSearchChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args) => ShowShows();

    private void OnShowFilterChanged(object sender, SelectionChangedEventArgs e) => ShowShows();

    private void OnShowFilterClicked(object sender, RoutedEventArgs e) => ShowShows();

    private async void OnShowSelected(object sender, SelectionChangedEventArgs e)
    {
        if (ShowList.SelectedItem is not ShowRow row || _core is null)
        {
            return;
        }
        var books = _books.Where(b => b.ShowId == row.Id && b.Exists)
            .Select(b => BookRow.From(b, BookDetail(b)))
            .ToList();
        ShowRecordings.ItemsSource = books;
        ShowRecordings.Visibility = books.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        // The reference text begins with the date and place.
        ShowDetailHint.Visibility = Visibility.Collapsed;
        AudioCore core = _core;
        try
        {
            ShowDetail detail = await Task.Run(() => core.GetShow(row.Id));
            if (!_closed && (ShowList.SelectedItem as ShowRow)?.Id == row.Id)
            {
                ShowInfo.Text = detail.Info;
            }
        }
        catch (AudioCoreException ex)
        {
            ShowInfo.Text = $"The show could not be read. {ex.Message}";
        }
    }

    private void OnShowRecordingClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is BookRow row)
        {
            ShowArea("player");
            PlayBook(row.Path);
        }
    }

    // ------------------------------------------------------------------
    // Songs

    private async Task RefreshSongsAsync()
    {
        string? folder = LibraryFolder();
        if (_core is null || folder is null)
        {
            _songs = [];
            _songsProblem = folder is null ? "Choose the folder that holds your shows to see the performances in it." : "The show database is unavailable.";
            ShowSongs();
            return;
        }
        AudioCore core = _core;
        SongsCount.Text = "Reading the shows folder…";
        try
        {
            var songs = await Task.Run(() => core.ListSongs(folder));
            if (_closed)
            {
                return;
            }
            _songs = [.. songs];
            _songsProblem = null;
        }
        catch (AudioCoreException e)
        {
            _songs = [];
            _songsProblem = $"The songs could not be read. {e.Message}";
        }
        ShowSongs();
    }

    private void ShowSongs()
    {
        if (_songs is null)
        {
            return;
        }
        string search = SongSearch.Text.Trim();
        bool ownedOnly = OwnedSongsOnly.IsChecked == true;
        string? selected = (SongList.SelectedItem as SongRow)?.Id;
        var rows = _songs
            .Where(s => !ownedOnly || s.Owned.Count > 0)
            .Where(s => search.Length == 0 || s.Title.Contains(search, StringComparison.CurrentCultureIgnoreCase))
            .Select(s => new SongRow(s.Id, s.Title, SongDetail(s)))
            .ToList();
        SongList.ItemsSource = rows;
        int ownedCount = _songs.Count(s => s.Owned.Count > 0);
        SongsCount.Text = _songsProblem ?? $"{rows.Count:N0} of {_songs.Count:N0} songs · {ownedCount:N0} in the shows folder";
        SongList.SelectedIndex = selected is null ? -1 : rows.FindIndex(r => r.Id == selected);
        if (SongList.SelectedIndex < 0)
        {
            ShowPerformances(null);
        }
    }

    private static string SongDetail(SongEntry s)
    {
        string played = s.Played == 1 ? "Played once" : $"Played {s.Played:N0} times";
        string owned = s.Owned.Count switch { 0 => "none in the folder", 1 => "1 in the folder", var n => $"{n} in the folder" };
        return $"{played} · {owned}";
    }

    private void OnSongSearchChanged(AutoSuggestBox sender, AutoSuggestBoxTextChangedEventArgs args) => ShowSongs();

    private void OnSongFilterClicked(object sender, RoutedEventArgs e) => ShowSongs();

    private void OnSongSelected(object sender, SelectionChangedEventArgs e) =>
        ShowPerformances(SongList.SelectedItem is SongRow row ? _songs?.FirstOrDefault(s => s.Id == row.Id) : null);

    private void ShowPerformances(SongEntry? song)
    {
        SongHeading.Text = song?.Title ?? "Song";
        PerformanceList.ItemsSource = song?.Owned
            .Select(o => new PerformanceRow(o.Path, (int)o.Chapter, $"{o.Date} · {o.Place}", $"{o.BookTitle} · chapter {o.Chapter + 1}"))
            .ToList();
        SongDetailHint.Text = song is null
            ? "Choose a song to see the performances in the shows folder."
            : song.Owned.Count == 0 ? "No show in the folder holds this song." : "Choose a performance to play it.";
    }

    private void OnPerformanceClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is PerformanceRow row)
        {
            PlayChapter(row.Path, row.Chapter);
        }
    }

    // ------------------------------------------------------------------
    // Transport and volume

    private void OnChapterClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ChapterRow { Index: >= 0 } row)
        {
            _core?.SelectChapter(row.Index);
        }
    }

    private void OnPlayPause(object sender, RoutedEventArgs e) => TogglePlayPause();

    private void TogglePlayPause()
    {
        if (_state == PlaybackState.Playing)
        {
            _core?.Pause();
        }
        else if (_state == PlaybackState.Paused)
        {
            _core?.Play();
        }
    }

    private void OnPrevious(object sender, RoutedEventArgs e) => _core?.PreviousChapter();

    private void OnNext(object sender, RoutedEventArgs e) => _core?.NextChapter();

    private void OnBack(object sender, RoutedEventArgs e) => _core?.SeekRelative(-_settings.SkipSeconds);

    private void OnForward(object sender, RoutedEventArgs e) => _core?.SeekRelative(_settings.SkipSeconds);

    private void OnPositionSliderChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        // Only the listener moving the thumb is a seek; updates from the core are not.
        if (_settingSlider || CurrentChapter() is not { } chapter)
        {
            return;
        }
        _sliderTouched = Environment.TickCount64;
        _core?.SeekAbsolute(chapter.Start + e.NewValue);
    }

    private void OnVolumeSliderChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        VolumeText.Text = $"{e.NewValue:0}%";
        if (!_settingSlider)
        {
            _core?.SetVolume(e.NewValue);
        }
    }

    private void OnVolumeUp(object sender, RoutedEventArgs e) => _core?.SetVolume(Math.Min(100, _volume + VolumeStep));

    private void OnVolumeDown(object sender, RoutedEventArgs e) => _core?.SetVolume(Math.Max(0, _volume - VolumeStep));

    private void OnMuteButton(object sender, RoutedEventArgs e) => ToggleMute();

    private void OnMuteItem(object sender, RoutedEventArgs e) => ToggleMute();

    private void ToggleMute()
    {
        _core?.SetMute(!_mute);
        // The core's volume event confirms it; until then show the setting as it was.
        MuteButton.IsChecked = MuteItem.IsChecked = _mute;
    }

    internal void ShowSkipLabels()
    {
        int seconds = _settings.SkipSeconds;
        BackLabel.Text = $"Back {seconds} s";
        ForwardLabel.Text = $"Forward {seconds} s";
        BackItem.Text = $"Back {seconds} seconds";
        ForwardItem.Text = $"Forward {seconds} seconds";
        ToolTipService.SetToolTip(BackButton, $"Back {seconds} seconds (Ctrl+Left)");
        ToolTipService.SetToolTip(ForwardButton, $"Forward {seconds} seconds (Ctrl+Right)");
        AutomationNameFor(BackButton, $"Back {seconds} seconds");
        AutomationNameFor(ForwardButton, $"Forward {seconds} seconds");
    }

    // Space is taken here, on the way down, so a focused button or list never
    // sees it: it is play/pause wherever the focus is, except in a text field.
    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        bool control = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control)
            .HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        bool typing = FocusManager.GetFocusedElement(Root.XamlRoot) is TextBox;
        switch (e.Key)
        {
            case VirtualKey.Space when !typing && !control:
                e.Handled = true;
                if (!e.KeyStatus.WasKeyDown)
                {
                    TogglePlayPause();  // not on auto-repeat
                }
                break;
            case VirtualKey.Add or (VirtualKey)187 when control:  // Ctrl and +
                e.Handled = true;
                StepZoom(1);
                break;
            case VirtualKey.Subtract or (VirtualKey)189 when control:  // Ctrl and -
                e.Handled = true;
                StepZoom(-1);
                break;
            case VirtualKey.Number0 or VirtualKey.NumberPad0 when control:
                e.Handled = true;
                SetZoom(AppSettings.DefaultZoom);
                break;
            case (VirtualKey)188 when control:  // Ctrl and ,
                e.Handled = true;
                OpenSettings();
                break;
        }
    }

    // ------------------------------------------------------------------
    // View: zoom, Books pane, full screen

    private void OnZoomSizeChanged(object sender, SizeChangedEventArgs e) => ApplyZoom();

    private void ApplyZoom()
    {
        if (Zoom.ActualWidth <= 0 || Zoom.ActualHeight <= 0)
        {
            return;
        }
        Scaled.Width = Zoom.ActualWidth / _settings.Zoom;
        Scaled.Height = Zoom.ActualHeight / _settings.Zoom;
        ZoomInItem.IsEnabled = _settings.Zoom < AppSettings.Zooms[^1];
        ZoomOutItem.IsEnabled = _settings.Zoom > AppSettings.Zooms[0];
    }

    private void StepZoom(int direction)
    {
        int index = Math.Clamp(Array.IndexOf(AppSettings.Zooms, _settings.Zoom) + direction, 0, AppSettings.Zooms.Length - 1);
        SetZoom(AppSettings.Zooms[index]);
    }

    private void SetZoom(double zoom)
    {
        if (zoom == _settings.Zoom)
        {
            return;
        }
        _settings.Zoom = zoom;
        _settings.Save();
        ApplyZoom();
    }

    private void OnZoomIn(object sender, RoutedEventArgs e) => StepZoom(1);

    private void OnZoomOut(object sender, RoutedEventArgs e) => StepZoom(-1);

    private void OnZoomReset(object sender, RoutedEventArgs e) => SetZoom(AppSettings.DefaultZoom);

    private void OnToggleBooksPane(object sender, RoutedEventArgs e)
    {
        if (_settings.BooksPaneVisible)
        {
            _settings.BooksPaneWidth = BooksColumn.ActualWidth;
        }
        _settings.BooksPaneVisible = !_settings.BooksPaneVisible;
        _settings.Save();
        ApplyBooksPane();
    }

    private void ApplyBooksPane()
    {
        bool visible = _settings.BooksPaneVisible;
        BooksPaneItem.IsChecked = visible;
        BooksPane.Visibility = BooksSplitter.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        BooksColumn.MinWidth = visible ? 240 : 0;
        BooksColumn.Width = new GridLength(visible ? _settings.BooksPaneWidth : 0);
    }

    private void OnFullScreen(object sender, RoutedEventArgs e) => AppWindow.SetPresenter(
        AppWindow.Presenter.Kind == AppWindowPresenterKind.FullScreen ? AppWindowPresenterKind.Overlapped : AppWindowPresenterKind.FullScreen);

    // ------------------------------------------------------------------
    // Settings, help, exit

    private void OnSettings(object sender, RoutedEventArgs e) => OpenSettings();

    private void OpenSettings()
    {
        if (_settingsWindow is null)
        {
            _settingsWindow = new SettingsWindow(this);
            _settingsWindow.Closed += (_, _) => _settingsWindow = null;
        }
        _settingsWindow.Activate();
    }

    /// <summary>Called by the Settings window after the books folder or skip interval changes.</summary>
    internal void OnSettingsChanged()
    {
        ShowSkipLabels();
        _ = RefreshBooksAsync();
    }

    internal string? CurrentLibraryFolder => LibraryFolder();

    private async void OnShortcuts(object sender, RoutedEventArgs e)
    {
        int seconds = _settings.SkipSeconds;
        var rows = new (string Key, string Action)[]
        {
            ("Space", "Play or pause"),
            ("Ctrl+Left", $"Back {seconds} seconds"),
            ("Ctrl+Right", $"Forward {seconds} seconds"),
            ("Ctrl+Shift+Left", "Previous chapter (restarts the chapter after its first 5 seconds)"),
            ("Ctrl+Shift+Right", "Next chapter"),
            ("Ctrl+Up / Ctrl+Down", "Volume up or down"),
            ("Ctrl+M", "Mute"),
            ("Ctrl+O", "Open a file"),
            ("Ctrl+W", "Close the show"),
            ("Ctrl+1 / Ctrl+2 / Ctrl+3", "Player, All shows, Songs"),
            ("Ctrl+B", "Show or hide the Shows pane"),
            ("F5", "Refresh the Shows pane"),
            ("Ctrl++ / Ctrl+- / Ctrl+0", "Zoom in, zoom out, actual size"),
            ("F11", "Full screen"),
            ("Ctrl+,", "Settings"),
        };
        var grid = new Grid { ColumnSpacing = 24, RowSpacing = 8 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (int i = 0; i < rows.Length; i++)
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var key = new TextBlock { Text = rows[i].Key, Style = Style("JupiterBodyStrongTextStyle") };
            var action = new TextBlock { Text = rows[i].Action, Style = Style("JupiterBodyTextStyle"), TextWrapping = TextWrapping.Wrap };
            Grid.SetRow(key, i);
            Grid.SetRow(action, i);
            Grid.SetColumn(action, 1);
            grid.Children.Add(key);
            grid.Children.Add(action);
        }
        await Dialog("Keyboard shortcuts", new ScrollViewer { Content = grid }).ShowAsync();
    }

    private async void OnAbout(object sender, RoutedEventArgs e)
    {
        var assembly = Assembly.GetExecutingAssembly();
        string version = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "";
        string build = version.Contains('+') ? version[(version.IndexOf('+') + 1)..] : "";
        string shortVersion = version.Split('+')[0];
        var text = new TextBlock
        {
            Style = Style("JupiterBodyTextStyle"),
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
            Text = $"Gratitude\nPart of Jupiter\nVersion {shortVersion}"
                + (build.Length > 0 ? $"\nBuild {build[..Math.Min(build.Length, 12)]}" : "")
                + "\nMIT License"
                + "\nPlayback by libmpv, under the GNU LGPL 2.1 or later; its license is in the licenses folder.",
        };
        await Dialog("About Gratitude", text).ShowAsync();
    }

    private ContentDialog Dialog(string title, object content)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = content,
            CloseButtonText = "Close",
            XamlRoot = Root.XamlRoot,
            RequestedTheme = Root.ActualTheme,
        };
        App.Current.Accent.Attach(dialog);
        return dialog;
    }

    private static Style Style(string key) => (Style)Application.Current.Resources[key];

    private void OnExit(object sender, RoutedEventArgs e) => Close();

    // ------------------------------------------------------------------
    // Window placement and closing

    private void RestoreBounds()
    {
        var display = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary);
        var area = display.WorkArea;
        double scale = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(this)) / 96.0;
        // Standard 6.1: a workspace opens at 1200 × 800 (minimum 800 × 560), here
        // in zoomed units, so the first window fits the layout at the chosen zoom.
        double zoomed = scale * _settings.Zoom;
        int width = Math.Min(_settings.WindowWidth > 0 ? _settings.WindowWidth : (int)(1200 * zoomed), area.Width);
        int height = Math.Min(_settings.WindowHeight > 0 ? _settings.WindowHeight : (int)(800 * zoomed), area.Height);
        var saved = new RectInt32(_settings.WindowX, _settings.WindowY, width, height);
        bool visible = _settings.WindowX != int.MinValue
            && saved.X >= area.X - 32 && saved.Y >= area.Y - 8
            && saved.X + 64 <= area.X + area.Width && saved.Y + 64 <= area.Y + area.Height;
        AppWindow.MoveAndResize(visible ? saved : new RectInt32(area.X + (area.Width - width) / 2, area.Y + (area.Height - height) / 2, width, height));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.PreferredMinimumWidth = (int)(800 * scale);
            presenter.PreferredMinimumHeight = (int)(560 * scale);
            if (_settings.WindowMaximized)
            {
                presenter.Maximize();
            }
        }
    }

    private void SaveLayout()
    {
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            _settings.WindowMaximized = presenter.State == OverlappedPresenterState.Maximized;
            if (presenter.State == OverlappedPresenterState.Restored)
            {
                _settings.WindowX = AppWindow.Position.X;
                _settings.WindowY = AppWindow.Position.Y;
                _settings.WindowWidth = AppWindow.Size.Width;
                _settings.WindowHeight = AppWindow.Size.Height;
            }
        }
        if (_settings.BooksPaneVisible && BooksColumn.ActualWidth > 0)
        {
            _settings.BooksPaneWidth = BooksColumn.ActualWidth;
        }
        _settings.Save();
    }

    private void OnClosed(object sender, WindowEventArgs args)
    {
        App.Current.RecordingPrioritiesChanged -= OnPriorityChange;
        _workshop?.Close();
        _closed = true;
        SaveLayout();
        _settingsWindow?.Close();
        if (_core is null)
        {
            return;
        }
        // Gone from the screen at once, even if the engine takes a moment to
        // stop (it can when closed in the middle of a long seek).
        AppWindow.Hide();
        // The core saves the position before it touches the engine, so even a
        // timeout here has not lost the listener's place.
        if (_core.Shutdown(3000) != NativeStatus.Ok)
        {
            AppLog.Write("The player did not stop within 3 seconds; exiting with the position saved.");
            Environment.Exit(1);
        }
        _core.Dispose();
    }

    [LibraryImport("user32.dll")]
    private static partial uint GetDpiForWindow(nint window);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetForegroundWindow(nint window);
}
