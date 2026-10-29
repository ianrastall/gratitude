using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace Jupiter.ShowPlayer.App;

/// <summary>Settings - Gratitude: one per app, non-modal, changes apply at once.</summary>
public sealed partial class SettingsWindow : Window
{
    private readonly MainWindow _main;
    private readonly AppSettings _settings = App.Current.Settings;
    private bool _loading = true;

    public SettingsWindow(MainWindow main)
    {
        _main = main;
        InitializeComponent();
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "AppIcon.ico"));
        App.Current.Accent.Attach(Root);
        App.Current.Theme.Attach(this);
        AccentControl.Service = App.Current.Accent;
        ThemeControl.Service = App.Current.Theme;

        foreach (int seconds in AppSettings.SkipChoices)
        {
            SkipBox.Items.Add(new ComboBoxItem { Content = $"{seconds} seconds", Tag = seconds });
        }
        SkipBox.SelectedIndex = Array.IndexOf(AppSettings.SkipChoices, _settings.SkipSeconds);
        ShowFolder();
        _loading = false;

        // Standard 6.1: 720 × 720, minimum 560 × 480, centered on the main window's display.
        bool sized = false;
        Root.Loaded += (_, _) =>
        {
            if (sized)
            {
                return;
            }
            sized = true;
            double scale = Root.XamlRoot.RasterizationScale;
            var area = DisplayArea.GetFromWindowId(_main.AppWindow.Id, DisplayAreaFallback.Primary).WorkArea;
            int width = Math.Min((int)(720 * scale), area.Width);
            int height = Math.Min((int)(720 * scale), area.Height);
            AppWindow.MoveAndResize(new RectInt32(area.X + (area.Width - width) / 2, area.Y + (area.Height - height) / 2, width, height));
            if (AppWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.PreferredMinimumWidth = (int)(560 * scale);
                presenter.PreferredMinimumHeight = (int)(480 * scale);
            }
        };
    }

    private void ShowFolder()
    {
        bool chosen = _settings.LibraryFolder is { Length: > 0 };
        FolderBox.Text = _main.CurrentLibraryFolder ?? "";
        FolderBox.PlaceholderText = "No folder chosen";
        FolderCaption.Text = chosen
            ? "The Shows pane lists the .mka files in this folder."
            : "The Shows pane lists the .mka files in the folder of the last show opened, until you choose one.";
        ResetFolderButton.Visibility = chosen ? Visibility.Visible : Visibility.Collapsed;
    }

    private async void OnBrowse(object sender, RoutedEventArgs e)
    {
        if (await MainWindow.PickFolderAsync(AppWindow.Id, _main.CurrentLibraryFolder) is { } folder)
        {
            _settings.LibraryFolder = folder;
            _settings.Save();
            ShowFolder();
            _main.OnSettingsChanged();
        }
    }

    private void OnResetFolder(object sender, RoutedEventArgs e)
    {
        _settings.LibraryFolder = null;
        _settings.Save();
        ShowFolder();
        _main.OnSettingsChanged();
    }

    private void OnSkipChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_loading || SkipBox.SelectedItem is not ComboBoxItem { Tag: int seconds })
        {
            return;
        }
        _settings.SkipSeconds = seconds;
        _settings.Save();
        _main.OnSettingsChanged();
    }
}
