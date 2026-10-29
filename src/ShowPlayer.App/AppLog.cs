namespace Jupiter.ShowPlayer.App;

/// <summary>
/// A plain-text log in the data folder for the core's diagnostics and the
/// shell's own failures. It holds paths and timings, never book contents.
/// </summary>
internal static class AppLog
{
    private const long MaxBytes = 1024 * 1024;
    private static readonly Lock Gate = new();
    private static string? s_file;

    public static void Open(string directory) => s_file = Path.Combine(directory, "show-player.log");

    public static void Write(string message)
    {
        if (s_file is not { } file)
        {
            return;
        }
        lock (Gate)
        {
            try
            {
                // One earlier log is kept beside the current one.
                if (File.Exists(file) && new FileInfo(file).Length > MaxBytes)
                {
                    File.Move(file, file + ".old", overwrite: true);
                }
                File.AppendAllText(file, $"{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}  {message}{Environment.NewLine}");
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
            }
        }
    }
}
