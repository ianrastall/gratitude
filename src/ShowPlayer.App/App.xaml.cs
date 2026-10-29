using Jupiter.UI;
using Microsoft.UI.Xaml;

namespace Jupiter.ShowPlayer.App;

/// <summary>What the command line asked for.</summary>
/// <param name="DataDirectory">Development and tests: a data folder in place of %LOCALAPPDATA%\show-player.</param>
/// <param name="AudioOutput">Tests: the engine's audio output driver; "null" is silent.</param>
/// <param name="OpenPath">A book to open at startup: the first argument that is not an option.</param>
/// <param name="AutoPlay">Test hook: start playing once the book has loaded.</param>
/// <param name="SeekTo">Test hook: seek here once the book has loaded.</param>
public sealed record StartupOptions(string? DataDirectory, string? AudioOutput, string? OpenPath, bool AutoPlay, double? SeekTo)
{
    public static StartupOptions Parse(string[] args)
    {
        string? data = null, audio = null, open = null;
        bool play = false;
        double? seek = null;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--data" when i + 1 < args.Length: data = args[++i]; break;
                case "--ao" when i + 1 < args.Length: audio = args[++i]; break;
                case "--seek" when i + 1 < args.Length:
                    seek = double.TryParse(args[++i], System.Globalization.NumberStyles.Float,
                        System.Globalization.CultureInfo.InvariantCulture, out double s) ? s : null;
                    break;
                case "--play": play = true; break;
                default: open ??= args[i]; break;
            }
        }
        return new StartupOptions(data, audio, open, play, seek);
    }
}

public partial class App : Application
{
    public event Action? RecordingPrioritiesChanged;
    public void NotifyRecordingPrioritiesChanged() => RecordingPrioritiesChanged?.Invoke();
    private MainWindow? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) => AppLog.Write($"Unhandled exception: {e.Exception}");
    }

    public static new App Current => (App)Application.Current;

    public static StartupOptions Options { get; } = StartupOptions.Parse(Environment.GetCommandLineArgs()[1..]);

    /// <summary>The folder every release has used for saved positions and settings.</summary>
    public static string DataDirectory { get; } = Options.DataDirectory ?? Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "show-player");

    public AppSettings Settings { get; private set; } = null!;

    public AccentService Accent { get; private set; } = null!;

    public ThemeService Theme { get; private set; } = null!;

    public MainWindow Window => _window!;

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        Directory.CreateDirectory(DataDirectory);
        AppLog.Open(DataDirectory);
        Settings = AppSettings.Load(DataDirectory);
        Accent = new AccentService(Settings);
        Theme = new ThemeService(Settings);
        _window = new MainWindow(Options);
        _window.Activate();
        Program.ListenForShowRequests(_window);
    }
}
