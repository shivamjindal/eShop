using System.Reflection;
using System.Runtime.InteropServices;

namespace eShop.Catalog.API.Model;

/// <summary>
/// P/Invoke boundary to <c>native/catalog_stock</c> (RemoveStock / AddStock).
/// </summary>
internal static partial class CatalogStockNative
{
    internal const int Ok = 0;
    internal const int ErrEmptyStock = 1;
    internal const int ErrNonPositiveQty = 2;

    private const string LibraryName = "catalog_stock";

    static CatalogStockNative()
    {
        NativeLibrary.SetDllImportResolver(Assembly.GetExecutingAssembly(), Resolve);
    }

    private static IntPtr Resolve(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (!string.Equals(libraryName, LibraryName, StringComparison.Ordinal))
        {
            return IntPtr.Zero;
        }

        var baseDir = AppContext.BaseDirectory;
        foreach (var candidate in CandidatePaths(baseDir))
        {
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle))
            {
                return handle;
            }
        }

        // Fall back to default probing (e.g. system path / rpath).
        return NativeLibrary.TryLoad(LibraryName, assembly, searchPath, out var fallback)
            ? fallback
            : IntPtr.Zero;
    }

    private static IEnumerable<string> CandidatePaths(string baseDir)
    {
        if (OperatingSystem.IsWindows())
        {
            yield return Path.Combine(baseDir, "catalog_stock.dll");
            yield return Path.Combine(baseDir, "runtimes", "win-x64", "native", "catalog_stock.dll");
            yield return Path.Combine(baseDir, "runtimes", "win-arm64", "native", "catalog_stock.dll");
        }
        else if (OperatingSystem.IsMacOS())
        {
            yield return Path.Combine(baseDir, "libcatalog_stock.dylib");
            yield return Path.Combine(baseDir, "runtimes", "osx-arm64", "native", "libcatalog_stock.dylib");
            yield return Path.Combine(baseDir, "runtimes", "osx-x64", "native", "libcatalog_stock.dylib");
        }
        else
        {
            yield return Path.Combine(baseDir, "libcatalog_stock.so");
            yield return Path.Combine(baseDir, "runtimes", "linux-x64", "native", "libcatalog_stock.so");
            yield return Path.Combine(baseDir, "runtimes", "linux-arm64", "native", "libcatalog_stock.so");
        }
    }

    [LibraryImport(LibraryName)]
    [UnmanagedCallConv(CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    internal static partial int catalog_stock_remove(
        ref int availableStock,
        int quantityDesired,
        out int removed);

    [LibraryImport(LibraryName)]
    [UnmanagedCallConv(CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    internal static partial int catalog_stock_add(
        ref int availableStock,
        int maxStockThreshold,
        ref byte onReorder,
        int quantity,
        out int added);
}
