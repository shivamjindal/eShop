using System.Reflection;
using System.Runtime.InteropServices;
using eShop.Catalog.API.Infrastructure.Exceptions;

namespace eShop.Catalog.API.Model;

/// <summary>
/// Stock rules for catalog items. Delegates to the Rust <c>catalog</c> cdylib.
/// </summary>
public static class CatalogStock
{
    private const int ErrOk = 0;
    private const int ErrEmptyStock = 1;
    private const int ErrInvalidQuantity = 2;

    private static readonly object LoadLock = new();
    private static RemoveStockFn? _removeStock;
    private static AddStockFn? _addStock;

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int RemoveStockFn(ref int availableStock, int quantityDesired, out int removed);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int AddStockFn(
        ref int availableStock,
        int maxStockThreshold,
        ref byte onReorder,
        int quantity,
        out int added);

    /// <summary>Decrements available stock; same semantics as legacy <see cref="CatalogItem.RemoveStock"/>.</summary>
    public static int RemoveStock(ref int availableStock, string productName, int quantityDesired)
    {
        EnsureNativeLoaded();
        var status = _removeStock!(ref availableStock, quantityDesired, out var removed);
        return status switch
        {
            ErrOk => removed,
            ErrEmptyStock => throw new CatalogDomainException($"Empty stock, product item {productName} is sold out"),
            ErrInvalidQuantity => throw new CatalogDomainException("Item units desired should be greater than zero"),
            _ => throw new CatalogDomainException($"Unexpected catalog stock error code {status}"),
        };
    }

    /// <summary>Increments available stock up to max threshold; clears on-reorder.</summary>
    public static int AddStock(ref int availableStock, int maxStockThreshold, ref bool onReorder, int quantity)
    {
        EnsureNativeLoaded();
        byte reorderFlag = onReorder ? (byte)1 : (byte)0;
        var status = _addStock!(ref availableStock, maxStockThreshold, ref reorderFlag, quantity, out var added);
        if (status != ErrOk)
        {
            throw new CatalogDomainException($"Unexpected catalog stock error code {status}");
        }

        onReorder = reorderFlag != 0;
        return added;
    }

    private static void EnsureNativeLoaded()
    {
        if (_removeStock is not null && _addStock is not null)
        {
            return;
        }

        lock (LoadLock)
        {
            if (_removeStock is not null && _addStock is not null)
            {
                return;
            }

            var path = ResolveNativeLibraryPath()
                ?? throw new DllNotFoundException(
                    $"Could not locate native library '{GetNativeLibraryFileName()}' next to Catalog.API or under AppContext.BaseDirectory.");

            var handle = NativeLibrary.Load(path);
            _removeStock = Marshal.GetDelegateForFunctionPointer<RemoveStockFn>(
                NativeLibrary.GetExport(handle, "catalog_remove_stock"));
            _addStock = Marshal.GetDelegateForFunctionPointer<AddStockFn>(
                NativeLibrary.GetExport(handle, "catalog_add_stock"));
        }
    }

    private static string? ResolveNativeLibraryPath()
    {
        var fileName = GetNativeLibraryFileName();
        var assemblyDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
        string[] candidates =
        [
            assemblyDir is null ? fileName : Path.Combine(assemblyDir, fileName),
            Path.Combine(AppContext.BaseDirectory, fileName),
            Path.Combine(AppContext.BaseDirectory, "runtimes", RuntimeInformation.RuntimeIdentifier, "native", fileName),
        ];

        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }

        return null;
    }

    private static string GetNativeLibraryFileName()
    {
        if (OperatingSystem.IsWindows())
        {
            return "catalog.dll";
        }

        if (OperatingSystem.IsMacOS())
        {
            return "libcatalog.dylib";
        }

        return "libcatalog.so";
    }
}
