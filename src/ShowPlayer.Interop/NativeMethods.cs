using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

// The app directory only (and System32 for the OS): a stray copy elsewhere on the system is never loaded.
[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.AssemblyDirectory | DllImportSearchPath.System32)]
[assembly: DisableRuntimeMarshalling]

namespace Jupiter.ShowPlayer.Interop;

/// <summary>Declarations for the Rust C ABI in core/ffi (ABI version 3).</summary>
internal static unsafe partial class NativeMethods
{
    private const string Library = "show_player_ffi";

    public const uint AbiVersion = 3;

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial uint ab_abi_version();

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial NativeStatus ab_create(byte* config, nuint configLength, out CoreSafeHandle core, out ResultSafeHandle error);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial NativeStatus ab_call(CoreSafeHandle core, byte* request, nuint requestLength, out ResultSafeHandle result);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial NativeStatus ab_wait_events(CoreSafeHandle core, uint timeoutMs, out ResultSafeHandle result);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial NativeStatus ab_shutdown(CoreSafeHandle core, int timeoutMs);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial NativeStatus ab_result_bytes(ResultSafeHandle result, out byte* bytes, out nuint length);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial void ab_result_free(nint result);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    public static partial void ab_destroy(nint core);
}

/// <summary>Transport-level outcomes of an ABI call. Domain failures arrive as Response.error.</summary>
public enum NativeStatus
{
    Ok = 0,
    InvalidArgument = 1,
    MalformedRequest = 2,
    Panic = 3,
    Unusable = 4,
    CreateFailed = 5,
    Timeout = 6,
    ShutDown = 7,
}

internal sealed class CoreSafeHandle() : SafeHandle(nint.Zero, ownsHandle: true)
{
    public override bool IsInvalid => handle == nint.Zero;

    // The handle's reference count keeps this from running while any call,
    // including a waiting ab_wait_events, is still inside the native core.
    // ab_destroy shuts the core down first if that has not happened.
    protected override bool ReleaseHandle()
    {
        NativeMethods.ab_destroy(handle);
        return true;
    }
}

internal sealed class ResultSafeHandle() : SafeHandle(nint.Zero, ownsHandle: true)
{
    public override bool IsInvalid => handle == nint.Zero;

    protected override bool ReleaseHandle()
    {
        NativeMethods.ab_result_free(handle);
        return true;
    }

    /// <summary>Copies the result bytes into managed memory before the handle is freed.</summary>
    public unsafe byte[] CopyBytes()
    {
        var status = NativeMethods.ab_result_bytes(this, out var ptr, out var length);
        if (status != NativeStatus.Ok)
        {
            throw new AudioCoreException($"Reading a result from the playback core failed ({status}).");
        }
        if (length > int.MaxValue)
        {
            throw new AudioCoreException("A result from the playback core was too large.");
        }
        return length == 0 ? [] : new ReadOnlySpan<byte>(ptr, checked((int)length)).ToArray();
    }
}
