using System.Diagnostics;
using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Windows.Storage.Pickers;
using Windows.Graphics;

namespace Jupiter.ShowPlayer.App;

public sealed record ImportTrackRow(int Index, string Label, string Detail);
public sealed partial class WorkshopWindow : Window
{
    private readonly AudioCore _core;
    private readonly MainWindow _main;
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(600) };
    private ShowDraft? _draft;
    private ShowJob? _job;
    private bool _editing, _closed, _inspecting;
    private int _trackIndex = -1;

    public WorkshopWindow(MainWindow main, AudioCore core, string page)
    {
        _main = main;
        _core = core;
        InitializeComponent();
        PrioritiesControl.Configure(core);
        App.Current.Accent.Attach(Root);
        App.Current.Theme.Attach(this);
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "AppIcon.ico"));
        _timer.Tick += (_, _) => PollJob();
        _timer.Start();
        Closed += (_, _) => { _closed = true; _timer.Stop(); };
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

    private void OnNavigation(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.IsSettingsSelected) { _main.OpenAppearanceSettings(); return; }
        if (args.SelectedItem is not NavigationViewItem { Tag: string page }) return;
        ImportPage.Visibility = page == "import" ? Visibility.Visible : Visibility.Collapsed;
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
        ImportButton.IsEnabled = NotesButton.IsEnabled = CreateButton.IsEnabled = TagButton.IsEnabled = false;
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
            AlbumBox.Text = draft.Album;
            DateBox.Text = draft.Date;
            CommentBox.Text = draft.Comment;
            NotesText.Text = draft.Notes;
            ImportSourceInfo.Labels = draft.SourceSummary;
            ImportCredits.Text = draft.Credits;
            ReferenceText.Text = string.IsNullOrEmpty(draft.ReferenceInfo) ? "No matching show reference in the offline catalogue." : draft.ReferenceInfo;
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
                NotesButton.IsEnabled = CreateButton.IsEnabled = TagButton.IsEnabled = _draft is not null;
                ImportProgress.Visibility = Visibility.Collapsed;
            }
        }
    }

    private void RenderTracks(int selected)
    {
        if (_draft is null) return;
        int digits = _draft.Tracks.Count.ToString().Length;
        TracksList.ItemsSource = _draft.Tracks.Select((t, i) => new ImportTrackRow(i, $"{(i + 1).ToString().PadLeft(digits, '0')} {t.Title}", $"{t.Set} · {Path.GetFileName(t.Path)} · {t.Duration / 60:F1} min · {t.Codec} {t.SampleRate} Hz / {t.BitDepth} bit / {t.Channels} ch")).ToList();
        TracksList.SelectedIndex = selected;
    }

    private void OnTrackSelected(object sender, SelectionChangedEventArgs e)
    {
        _editing = true;
        _trackIndex = TracksList.SelectedIndex;
        bool valid = _draft is not null && _trackIndex >= 0 && _trackIndex < _draft.Tracks.Count;
        TrackTitleBox.IsEnabled = valid;
        TrackTitleBox.Text = valid ? _draft!.Tracks[_trackIndex].Title : "";
        _editing = false;
    }

    private void OnTrackEdited(object sender, TextChangedEventArgs e)
    {
        if (_editing || _draft is null || _trackIndex < 0) return;
        _draft.Tracks[_trackIndex].Title = TrackTitleBox.Text;
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

    private bool ReadReview()
    {
        if (_draft is null) return false;
        if (ReviewedBox.IsChecked != true) { Error(new InvalidOperationException("Review the song titles, order and tags, then check the review box.")); return false; }
        _draft.Date = DateBox.Text.Trim(); _draft.Album = AlbumBox.Text.Trim(); _draft.Comment = CommentBox.Text.Trim();
        return true;
    }

    private async void OnCreate(object sender, RoutedEventArgs e)
    {
        if (!ReadReview()) return;
        try
        {
            var picker = new FileSavePicker(AppWindow.Id) { SuggestedFileName = _draft!.Album };
            picker.FileTypeChoices.Add("Matroska audio", new List<string> { ".mka" });
            var result = await picker.PickSaveFileAsync();
            if (result is null || _closed) return;
            _job = _core.PackageShow(_draft, result.Path, CopyAudioBox.IsChecked == true);
            ShowJob(); Navigate("job");
        }
        catch (Exception ex) { Error(ex); }
    }

    private async void OnWriteTags(object sender, RoutedEventArgs e)
    {
        if (!ReadReview()) return;
        try
        {
            string? parent = await MainWindow.PickFolderAsync(AppWindow.Id, Path.GetDirectoryName(_draft!.Folder));
            if (parent is null || _closed) return;
            _job = _core.TagShow(_draft, parent);
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
        if (JobLog.Text != _job.Log)
        {
            // Follows new output unless the person has scrolled up to read.
            bool following = JobLogScroller.VerticalOffset >= JobLogScroller.ScrollableHeight - 1;
            JobLog.Text = _job.Log;
            if (following)
            {
                JobLogScroller.UpdateLayout();
                JobLogScroller.ChangeView(null, JobLogScroller.ScrollableHeight, null, true);
            }
        }
        JobProgress.Visibility = running ? Visibility.Visible : Visibility.Collapsed;
        CancelButton.IsEnabled = running;
        UseOutputButton.IsEnabled = _job.State == "Completed";
        CreateButton.IsEnabled = TagButton.IsEnabled = !running && !_inspecting && _draft is not null;
    }
    private void OnCancel(object sender, RoutedEventArgs e)
    {
        try { _core.CancelJob(); JobState.Text = "Cancelling"; CancelButton.IsEnabled = false; }
        catch (Exception ex) { Error(ex); }
    }
    private void OnUseOutput(object sender, RoutedEventArgs e)
    {
        if (_job is null || _job.State != "Completed") return;
        if (Directory.Exists(_job.Output)) Process.Start(new ProcessStartInfo(_job.Output) { UseShellExecute = true });
        else if (File.Exists(_job.Output)) { _main.OpenShow(_job.Output); _main.ShowAndActivate(); }
    }
}
