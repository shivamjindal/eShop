using Wasmtime;

namespace eShop.Catalog.API.Model;

internal static class CatalogStockWasm
{
    internal const long ErrEmpty = -1;
    internal const long ErrInvalidQty = -2;

    private static readonly object Gate = new();
    private static Engine? _engine;
    private static Module? _module;
    private static Store? _store;
    private static Func<int, int, long>? _remove;
    private static Func<int, int, int, long>? _add;

    internal static long Remove(int availableStock, int quantityDesired)
    {
        EnsureInitialized();
        lock (Gate)
        {
            return _remove!(availableStock, quantityDesired);
        }
    }

    internal static long Add(int availableStock, int maxStockThreshold, int quantity)
    {
        EnsureInitialized();
        lock (Gate)
        {
            return _add!(availableStock, maxStockThreshold, quantity);
        }
    }

    private static void EnsureInitialized()
    {
        if (_remove is not null && _add is not null)
        {
            return;
        }

        lock (Gate)
        {
            if (_remove is not null && _add is not null)
            {
                return;
            }

            var wasmPath = ResolveWasmPath();
            _engine = new Engine();
            _module = Module.FromFile(_engine, wasmPath);
            _store = new Store(_engine);
            var linker = new Linker(_engine);
            var instance = linker.Instantiate(_store, _module);

            _remove = instance.GetFunction<int, int, long>("catalog_stock_remove")
                ?? throw new InvalidOperationException("catalog_stock_remove export missing or wrong signature");
            _add = instance.GetFunction<int, int, int, long>("catalog_stock_add")
                ?? throw new InvalidOperationException("catalog_stock_add export missing or wrong signature");
        }
    }

    private static string ResolveWasmPath()
    {
        var candidate = Path.Combine(AppContext.BaseDirectory, "catalog_stock.wasm");
        if (File.Exists(candidate))
        {
            return candidate;
        }

        throw new FileNotFoundException(
            "catalog_stock.wasm not found next to the assembly. Build Catalog.API (cargo wasm) first.",
            candidate);
    }
}
