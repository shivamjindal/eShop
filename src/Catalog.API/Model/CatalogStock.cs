using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using eShop.Catalog.API.Infrastructure.Exceptions;

namespace eShop.Catalog.API.Model;

/// <summary>
/// Stock mutation island — live path delegates to the Rust <c>catalog</c> cdylib.
/// </summary>
internal static partial class CatalogStock
{
    public static int RemoveStock(ref int availableStock, int quantityDesired, string productName)
    {
        var status = Native.Remove(availableStock, quantityDesired, out var newAvailable, out var removed);
        switch (status)
        {
            case 0:
                availableStock = newAvailable;
                return removed;
            case 1:
                throw new CatalogDomainException($"Empty stock, product item {productName} is sold out");
            case 2:
                throw new CatalogDomainException("Item units desired should be greater than zero");
            default:
                throw new CatalogDomainException($"Unexpected catalog stock status code {status}");
        }
    }

    public static int AddStock(ref int availableStock, int maxStockThreshold, ref bool onReorder, int quantity)
    {
        var status = Native.Add(availableStock, maxStockThreshold, quantity, out var newAvailable, out var added);
        if (status != 0)
        {
            throw new CatalogDomainException($"Unexpected catalog stock status code {status}");
        }

        availableStock = newAvailable;
        onReorder = false;
        return added;
    }

    private static partial class Native
    {
        private const string LibraryName = "catalog";

        static Native()
        {
            NativeLibrary.SetDllImportResolver(typeof(CatalogStock).Assembly, static (name, _, _) =>
            {
                if (name != LibraryName)
                {
                    return IntPtr.Zero;
                }

                foreach (var candidate in CandidatePaths())
                {
                    if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle))
                    {
                        return handle;
                    }
                }

                return IntPtr.Zero;
            });
        }

        private static IEnumerable<string> CandidatePaths()
        {
            var fileName = OperatingSystem.IsWindows()
                ? "catalog.dll"
                : OperatingSystem.IsMacOS()
                    ? "libcatalog.dylib"
                    : "libcatalog.so";

            yield return Path.Combine(AppContext.BaseDirectory, fileName);

            // Dev fallback: native/target/release next to the repo layout.
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir is not null)
            {
                var release = Path.Combine(dir.FullName, "native", "target", "release", fileName);
                if (File.Exists(release))
                {
                    yield return release;
                }

                dir = dir.Parent;
            }
        }

        [LibraryImport(LibraryName, EntryPoint = "catalog_stock_remove")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        public static partial int Remove(int availableStock, int quantityDesired, out int outAvailable, out int outRemoved);

        [LibraryImport(LibraryName, EntryPoint = "catalog_stock_add")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        public static partial int Add(int availableStock, int maxStockThreshold, int quantity, out int outAvailable, out int outAdded);
    }
}
