using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Jupiter.ShowPlayer.App;

public static partial class Program
{
    // Two processes must never write the same resume store. The installer
    // checks the same mutex to ask for the app to be closed first.
    private const string InstanceMutexName = @"Local\ShowPlayer.SingleInstance";
    private const string ShowEventName = @"Local\ShowPlayer.Show";

    /// <summary>
    /// The identity the taskbar groups and pins this app under, unchanged since
    /// 0.1 so existing pins keep working. The installer puts the same value on
    /// the Start shortcut.
    /// </summary>
    public const string AppUserModelId = "IanRastall.ShowPlayer";

    private static EventWaitHandle? s_showSignal;

    [STAThread]
    public static int Main(string[] args)
    {
        // Created before the instance mutex: a second launch can signal even during WinUI startup.
        s_showSignal = new EventWaitHandle(false, EventResetMode.AutoReset, ShowEventName);

        // One instance per user session. A second launch asks the first to show its window.
        using var instance = new Mutex(initiallyOwned: true, InstanceMutexName, out bool createdNew);
        if (!createdNew)
        {
            // This process was started by the user, so it may hand the foreground to the first one.
            AllowSetForegroundWindow(AnyProcess);
            s_showSignal.Set();
            return 0;
        }

        Marshal.ThrowExceptionForHR(SetCurrentProcessExplicitAppUserModelID(AppUserModelId));
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(callback =>
        {
            var context = new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            _ = new App();
        });
        GC.KeepAlive(instance);
        return 0;
    }

    /// <summary>Brings the window forward whenever a second launch signals.</summary>
    internal static void ListenForShowRequests(MainWindow window)
    {
        var signal = s_showSignal!;
        var dispatcher = window.DispatcherQueue;
        var thread = new Thread(() =>
        {
            while (signal.WaitOne())
            {
                if (!dispatcher.TryEnqueue(window.ShowAndActivate))
                {
                    return;
                }
            }
        })
        { IsBackground = true, Name = "show-requests" };
        thread.Start();
    }

    private const uint AnyProcess = unchecked((uint)-1);

    [LibraryImport("shell32.dll", StringMarshalling = StringMarshalling.Utf16)]
    private static partial int SetCurrentProcessExplicitAppUserModelID(string appId);

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool AllowSetForegroundWindow(uint processId);
}
