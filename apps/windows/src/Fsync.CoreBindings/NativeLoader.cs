using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Fsync.Core;

/// <summary>
/// Resolves <c>fsync_ffi</c> to <c>runtimes/&lt;rid&gt;/native/fsync_ffi.dll</c> under the app
/// directory for the current process architecture. Project references do not list those
/// assets in deps.json, so the default probe (app directory only) would miss them.
/// Falls back to the default search, so a DLL next to the app still wins.
/// </summary>
internal static class NativeLoader
{
    private const string LibName = "fsync_ffi";

    // A library initializer on purpose: the resolver must be in place before the first
    // generated DllImport runs, whatever the app touches first.
#pragma warning disable CA2255
    [ModuleInitializer]
#pragma warning restore CA2255
    internal static void Init() =>
        NativeLibrary.SetDllImportResolver(typeof(NativeLoader).Assembly, Resolve);

    private static IntPtr Resolve(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (name != LibName) return IntPtr.Zero;
        if (NativeLibrary.TryLoad(name, assembly, path, out var handle)) return handle;

        var rid = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "win-x64",
            Architecture.Arm64 => "win-arm64",
            _ => null,
        };
        if (rid is null) return IntPtr.Zero;
        var file = Path.Combine(AppContext.BaseDirectory, "runtimes", rid, "native", LibName + ".dll");
        return NativeLibrary.TryLoad(file, out handle) ? handle : IntPtr.Zero;
    }
}
