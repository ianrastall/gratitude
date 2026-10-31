using System.Text.Json;
using System.Text.Json.Serialization;

namespace Jupiter.ShowPlayer.App;

/// <summary>
/// What the shell remembers: appearance, the books folder, zoom, the skip
/// interval and window layout. Playback state and saved positions belong to
/// the core's store, not to this file.
/// </summary>
/// <remarks>
/// Kept in ui.json in the data folder, the file 0.1 to 0.3 used for the size
/// setting; that version's <c>Scale</c> is read as the zoom. Its cached book
/// titles are no longer needed (the store keeps titles) and are dropped.
/// </remarks>
public sealed class AppSettings : Jupiter.UI.IAccentPreferences, Jupiter.UI.IThemePreferences
{
    public const int CurrentVersion = 2;
    public static readonly double[] Zooms = [1.0, 1.25, 1.5, 1.75, 2.0];
    public const double DefaultZoom = 1.0;
    public static readonly int[] SkipChoices = [10, 15, 30, 60];
    public const int DefaultSkipSeconds = 30;
    private const string NeutralAccent = "#2E7D9A";

    private string _file = "";

    public int Version { get; set; }

    /// <summary>Version 1 (Gratitude 0.1 to 0.3) only: the size setting, read as <see cref="Zoom"/>.</summary>
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingDefault)]
    public double Scale { get; set; }

    /// <summary>How much larger than standard the workspace is drawn.</summary>
    public double Zoom { get; set; } = DefaultZoom;

    public string? Theme { get; set; }

    public bool UseCustomAccent { get; set; }

    public string? CustomAccent { get; set; }

    /// <summary>The red accent check below has run; a later return to the Windows accent stands.</summary>
    public bool AccentChecked { get; set; }

    /// <summary>The folder the Books pane lists. Null means the folder of the last book opened.</summary>
    public string? LibraryFolder { get; set; }
    public string? DownloadFolder { get; set; }

    public int SkipSeconds { get; set; } = DefaultSkipSeconds;

    public bool BooksPaneVisible { get; set; } = true;

    public double BooksPaneWidth { get; set; } = 320;

    public int WindowX { get; set; } = int.MinValue;

    public int WindowY { get; set; } = int.MinValue;

    public int WindowWidth { get; set; }

    public int WindowHeight { get; set; }

    public bool WindowMaximized { get; set; }

    public static AppSettings Load(string directory)
    {
        string file = Path.Combine(directory, "ui.json");
        AppSettings settings = new();
        try
        {
            if (File.Exists(file))
            {
                settings = JsonSerializer.Deserialize(File.ReadAllText(file), SettingsJson.Default.AppSettings) ?? new();
            }
        }
        catch (Exception e) when (e is IOException or JsonException or UnauthorizedAccessException)
        {
            settings = new();
        }
        settings._file = file;
        if (settings.Version < 2 && settings.Scale > 0)
        {
            settings.Zoom = settings.Scale;
        }
        settings.Scale = 0;
        settings.Version = CurrentVersion;
        if (!Zooms.Contains(settings.Zoom))
        {
            settings.Zoom = DefaultZoom;
        }
        if (!SkipChoices.Contains(settings.SkipSeconds))
        {
            settings.SkipSeconds = DefaultSkipSeconds;
        }
        settings.BooksPaneWidth = Math.Clamp(settings.BooksPaneWidth, 240, 800);
        return settings;
    }

    /// <summary>
    /// Selection and primary actions are tinted with the accent, and a red tint here reads
    /// as an error. A red-family Windows accent is replaced once; Settings can restore it.
    /// See docs/jupiter-exceptions.md.
    /// </summary>
    public void AvoidRedAccent(Windows.UI.Color windows)
    {
        if (AccentChecked)
        {
            return;
        }
        AccentChecked = true;
        int max = Math.Max(windows.R, Math.Max(windows.G, windows.B));
        int spread = max - Math.Min(windows.R, Math.Min(windows.G, windows.B));
        if (!UseCustomAccent && spread * 10 >= max * 3)
        {
            double sector = max == windows.R ? (double)(windows.G - windows.B) / spread
                : max == windows.G ? 2 + (double)(windows.B - windows.R) / spread
                : 4 + (double)(windows.R - windows.G) / spread;
            double hue = (sector * 60 + 360) % 360;
            if (hue is >= 285 or <= 40)
            {
                UseCustomAccent = true;
                CustomAccent = NeutralAccent;
            }
        }
        Save();
    }

    /// <summary>Writes through a temporary file, so a failed write leaves the old settings whole.</summary>
    public void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_file)!);
            string temporary = _file + ".tmp";
            File.WriteAllText(temporary, JsonSerializer.Serialize(this, SettingsJson.Default.AppSettings));
            File.Move(temporary, _file, overwrite: true);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            AppLog.Write($"Settings could not be saved: {e.Message}");
        }
    }
}

[JsonSourceGenerationOptions(WriteIndented = true)]
[JsonSerializable(typeof(AppSettings))]
internal sealed partial class SettingsJson : JsonSerializerContext;
