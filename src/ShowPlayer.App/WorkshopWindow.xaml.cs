using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Windows.Storage.Pickers;
using Windows.Graphics;

namespace Jupiter.ShowPlayer.App;

public sealed record ImportTrackRow(int Index, string Label, string Detail);
public sealed record RecordingRow(Recording Recording)
{
    public string Title => Recording.Title;
    public string Detail => $"{Recording.Source} · {Recording.Detail}";
    public string SourceInfo => Recording.SourceSummary + (Recording.Inferred ? " · type inferred from identifier" : "");
    public string ShowHeading => $"{Recording.Date} · {Recording.Venue}";
    public string Credits => string.IsNullOrWhiteSpace(Recording.Credits) ? "People: not identified in this snapshot" : Recording.Credits;
    public override string ToString() => Title;
}

public sealed partial class WorkshopWindow : Window
{
    private readonly AudioCore _core;
    private readonly MainWindow _main;
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(600) };
    private ShowDraft? _draft;
    private ShowJob? _job;
    private bool _editing, _closed, _inspecting, _searching;
    private int _trackIndex = -1;

    public WorkshopWindow(MainWindow main, AudioCore core, string page)
    {
        _main = main;
        _core = core;
        InitializeComponent();
        PrioritiesControl.Configure(core);
        App.Current.RecordingPrioritiesChanged += OnPriorityChange;
        App.Current.Accent.Attach(Root);
        App.Current.Theme.Attach(this);
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "AppIcon.ico"));
        DestinationBox.Text = App.Current.Settings.DownloadFolder ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyMusic), "Show Player", "Sources");
        _timer.Tick += (_, _) => PollJob();
        _timer.Start();
        Closed += (_, _) => { _closed = true; _timer.Stop(); App.Current.RecordingPrioritiesChanged -= OnPriorityChange; };
        Root.Loaded += (_, _) =>
        {
            double scale = Root.XamlRoot.RasterizationScale;
            var area = DisplayArea.GetFromWindowId(main.AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
            int w = Math.Min((int)(1200 * scale), area.Width), h = Math.Min((int)(800 * scale), area.Height);
            AppWindow.MoveAndResize(new RectInt32(area.X + (area.Width-w)/2, area.Y + (area.Height-h)/2, w, h));
            if (AppWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.PreferredMinimumWidth = (int)(1000 * scale);
                presenter.PreferredMinimumHeight = (int)(640 * scale);
            }
        };
        Navigate(page);
    }

    public void Navigate(string page)
    {
        Navigation.SelectedItem = Navigation.MenuItems.OfType<NavigationViewItem>().First(i => (string)i.Tag == page);
        Activate();
    }
    private void OnPriorityChange() { if (RecordingsList.ItemsSource is not null) _ = Search(); }

    private void OnNavigation(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected) { _main.OpenAppearanceSettings(); return; }
        if (args.SelectedItem is not NavigationViewItem { Tag: string page }) return;
        ImportPage.Visibility = page == "import" ? Visibility.Visible : Visibility.Collapsed;
        FindPage.Visibility = page == "find" ? Visibility.Visible : Visibility.Collapsed;
        JobPage.Visibility = page == "job" ? Visibility.Visible : Visibility.Collapsed;
    }

    private void Error(Exception e)
    {
        if (_closed) return;
        Notice.Title = "Operation failed";
        Notice.Message = e.Message;
        Notice.Severity = InfoBarSeverity.Error;
        Notice.IsOpen = true;
    }

    private async void OnImport(object sender, RoutedEventArgs e)
    {
        try
        {
            string? folder = await MainWindow.PickFolderAsync(AppWindow.Id, _draft?.Folder);
            if (folder is not null) await Inspect(folder);
        }
        catch (Exception ex) { Error(ex); }
    }

    private async void OnChooseNotes(object sender, RoutedEventArgs e)
    {
        if (_draft is null) return;
        try
        {
            var picker = new FileOpenPicker(AppWindow.Id);
            picker.FileTypeFilter.Add(".txt");
            var file = await picker.PickSingleFileAsync();
            if (file is not null) await Inspect(_draft.Folder, file.Path);
        }
        catch (Exception ex) { Error(ex); }
    }

    private async Task Inspect(string folder, string notes = "")
    {
        if (_inspecting) return;
        _inspecting = true;
        ImportButton.IsEnabled = NotesButton.IsEnabled = CreateButton.IsEnabled = false;
        ImportProgress.Visibility = Visibility.Visible;
        ImportSummary.Text = "Reading notes and inspecting audio files…";
        Notice.IsOpen = false;
        try
        {
            var draft = await Task.Run(() => _core.InspectShow(folder, notes));
            if (_closed) return;
            _draft = draft;
            FolderBox.Text = draft.Folder;
            NotesPathBox.Text = draft.NotesPath;
            ArtistBox.Text = draft.Artist;
            DateBox.Text = draft.Date;
            VenueBox.Text = draft.Venue;
            NotesText.Text = draft.Notes;
            WarningsText.Text = string.Join("\n\n", draft.Warnings);
            ReviewedBox.IsChecked = false;
            ImportSummary.Text = $"{draft.Tracks.Count} files · {TimeSpan.FromSeconds(draft.Tracks.Sum(t => t.Duration)):h\\:mm\\:ss} · review the proposed order and titles";
            RenderTracks(0);
        }
        catch (Exception ex) { Error(ex); ImportSummary.Text = "Import failed. Choose another folder or TXT file."; }
        finally
        {
            _inspecting = false;
            if (!_closed)
            {
                ImportButton.IsEnabled = true;
                NotesButton.IsEnabled = CreateButton.IsEnabled = _draft is not null;
                ImportProgress.Visibility = Visibility.Collapsed;
            }
        }
    }

    private void RenderTracks(int selected)
    {
        if (_draft is null) return;
        TracksList.ItemsSource = _draft.Tracks.Select((t, i) => new ImportTrackRow(i, $"{i+1}. {t.Title}", $"{t.Set} · {Path.GetFileName(t.Path)} · {t.Duration / 60:F1} min · {t.Codec} {t.SampleRate} Hz / {t.BitDepth} bit / {t.Channels} ch")).ToList();
        TracksList.SelectedIndex = selected;
    }

    private void OnTrackSelected(object sender, SelectionChangedEventArgs e)
    {
        _editing = true;
        _trackIndex = TracksList.SelectedIndex;
        bool valid = _draft is not null && _trackIndex >= 0 && _trackIndex < _draft.Tracks.Count;
        TrackTitleBox.IsEnabled = TrackSetBox.IsEnabled = valid;
        TrackTitleBox.Text = valid ? _draft!.Tracks[_trackIndex].Title : "";
        TrackSetBox.Text = valid ? _draft!.Tracks[_trackIndex].Set : "";
        _editing = false;
    }

    private void OnTrackEdited(object sender, TextChangedEventArgs e)
    {
        if (_editing || _draft is null || _trackIndex < 0) return;
        _draft.Tracks[_trackIndex].Title = TrackTitleBox.Text;
        _draft.Tracks[_trackIndex].Set = TrackSetBox.Text;
        ReviewedBox.IsChecked = false;
    }

    private void Move(int delta)
    {
        if (_draft is null || _trackIndex < 0) return;
        int target = _trackIndex + delta;
        if (target < 0 || target >= _draft.Tracks.Count) return;
        var track = _draft.Tracks[_trackIndex];
        _draft.Tracks.RemoveAt(_trackIndex);
        _draft.Tracks.Insert(target, track);
        RenderTracks(target);
        ReviewedBox.IsChecked = false;
    }
    private void OnMoveUp(object sender, RoutedEventArgs e) => Move(-1);
    private void OnMoveDown(object sender, RoutedEventArgs e) => Move(1);
    private void OnTrackEditFinished(object sender, RoutedEventArgs e) { if (_draft is not null && _trackIndex >= 0) RenderTracks(_trackIndex); }

    private async void OnCreate(object sender, RoutedEventArgs e)
    {
        if (_draft is null) return;
        if (ReviewedBox.IsChecked != true) { Error(new InvalidOperationException("Review the song titles and order, then check the review box.")); return; }
        try
        {
            _draft.Artist = ArtistBox.Text.Trim(); _draft.Date = DateBox.Text.Trim(); _draft.Venue = VenueBox.Text.Trim();
            var picker = new FileSavePicker(AppWindow.Id) { SuggestedFileName = "show" };
            picker.FileTypeChoices.Add("Matroska audio", new List<string> { ".mka" });
            var result = await picker.PickSaveFileAsync();
            if (result is null || _closed) return;
            _job = _core.PackageShow(_draft, result.Path, CopyAudioBox.IsChecked == true);
            ShowJob(); Navigate("job");
        }
        catch (Exception ex) { Error(ex); }
    }

    private async Task Search()
    {
        if (_searching) return;
        _searching = true;
        SearchSummary.Text = "Searching saved listings…";
        string query = SearchBox.Text, source = SourceBox.SelectedItem?.ToString() ?? "All sources";
        string? selected = (RecordingsList.SelectedItem as RecordingRow)?.Recording.Key;
        try
        {
            var rows = await Task.Run(() => _core.SearchRecordings(query, source));
            if (_closed) return;
            var results = rows.Select(r => new RecordingRow(r)).ToList();
            RecordingsList.ItemsSource = results;
            RecordingsList.SelectedIndex = results.FindIndex(r => r.Recording.Key == selected);
            SearchSummary.Text = $"{rows.Count} matches{(rows.Count == 500 ? " (first 500; narrow your search)" : "")} · ranked by your priorities · attributed offline XML snapshot";
        }
        catch (Exception ex) { Error(ex); }
        finally { _searching = false; }
    }
    private async void OnSearch(object sender, RoutedEventArgs e) => await Search();
    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args) => await Search();
    private void OnRecordingSelected(object sender, SelectionChangedEventArgs e)
    {
        var row = RecordingsList.SelectedItem as RecordingRow;
        RecordingTitle.Text = row?.Title ?? "Select a recording.";
        RecordingSourceInfo.Text = row?.SourceInfo ?? "";
        RecordingCredits.Text = row?.Credits ?? "";
        ShowInfoText.Text = row?.Recording.ShowInfo ?? "";
        DetailsLink.IsEnabled = DownloadButton.IsEnabled = row is not null;
        DetailsLink.NavigateUri = row is null ? null : new Uri(row.Recording.Url);
    }
    private async void OnDestination(object sender, RoutedEventArgs e)
    {
        try
        {
            if (await MainWindow.PickFolderAsync(AppWindow.Id, DestinationBox.Text) is { } folder)
            {
                DestinationBox.Text = folder;
                App.Current.Settings.DownloadFolder = folder;
                App.Current.Settings.Save();
            }
        }
        catch (Exception ex) { Error(ex); }
    }
    private void OnDownload(object sender, RoutedEventArgs e)
    {
        if (RecordingsList.SelectedItem is not RecordingRow row) return;
        try
        {
            uint seed = SeedBox.SelectedIndex switch { 1 => 30, 2 => 120, _ => 0 };
            _job = _core.DownloadRecording(row.Recording, DestinationBox.Text, seed);
            ShowJob(); Navigate("job");
        }
        catch (Exception ex) { Error(ex); }
    }
    private void PollJob()
    {
        if (_closed) return;
        try { _job = _core.GetJob(); ShowJob(); }
        catch (Exception ex) { _timer.Stop(); Error(ex); }
    }
    private void ShowJob()
    {
        if (_job is null) return;
        bool running = _job.State == "Running";
        JobState.Text = string.IsNullOrEmpty(_job.State) ? "No job running" : _job.State;
        JobPhase.Text = _job.Phase;
        JobOutput.Text = _job.Output;
        if (JobLog.Text != _job.Log) JobLog.Text = _job.Log;
        JobProgress.Visibility = running ? Visibility.Visible : Visibility.Collapsed;
        CancelButton.IsEnabled = running;
        UseOutputButton.IsEnabled = _job.State == "Completed";
        DownloadButton.IsEnabled = !running && RecordingsList.SelectedItem is RecordingRow;
        CreateButton.IsEnabled = !running && !_inspecting && _draft is not null;
    }
    private void OnCancel(object sender, RoutedEventArgs e)
    {
        try { _core.CancelJob(); JobState.Text = "Cancelling"; CancelButton.IsEnabled = false; }
        catch (Exception ex) { Error(ex); }
    }
    private async void OnUseOutput(object sender, RoutedEventArgs e)
    {
        if (_job is null || _job.State != "Completed") return;
        if (File.Exists(_job.Output)) { _main.OpenShow(_job.Output); _main.ShowAndActivate(); }
        else if (Directory.Exists(_job.Output)) { Navigate("import"); await Inspect(_job.Output); }
    }
}
