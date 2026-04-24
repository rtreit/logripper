using System;
using System.Collections.Concurrent;
using System.IO;
using System.Reflection;

namespace QsoRipper.Gui.Utilities;

// Issue #318: Avalonia.Controls.DataGrid (compiled against the .NETStandard reference of
// System.ComponentModel.Annotations, PublicKeyToken=b03f5f7f11d50a3a) can fail to bind to
// the .NET shared-framework copy of that assembly in published Release builds, throwing
// FileNotFoundException the first time DataGridDataConnection.GetPropertyIsReadOnly runs
// (i.e., the moment a user clicks a DataGrid cell to begin editing). The shared framework
// always ships System.ComponentModel.Annotations under the Microsoft public key, so we can
// satisfy the request by loading it by simple name and returning that instance.
//
// This is defensive: if the runtime would have resolved the request itself, the handler is
// never invoked. If the runtime fails to bind, the handler unblocks the load and prevents
// the crash.
internal static class FrameworkAssemblyResolver
{
    private static readonly string[] FrameworkSimpleNames =
    {
        "System.ComponentModel.Annotations",
    };

    private static readonly ConcurrentDictionary<string, Assembly> Cache = new(StringComparer.OrdinalIgnoreCase);

    private static int _registered;

    public static void Register()
    {
        if (System.Threading.Interlocked.Exchange(ref _registered, 1) != 0)
        {
            return;
        }

        AppDomain.CurrentDomain.AssemblyResolve += OnAssemblyResolve;
    }

    internal static Assembly? Resolve(AssemblyName requested)
    {
        ArgumentNullException.ThrowIfNull(requested);

        var simpleName = requested.Name;
        if (string.IsNullOrEmpty(simpleName))
        {
            return null;
        }

        if (Array.IndexOf(FrameworkSimpleNames, simpleName) < 0)
        {
            return null;
        }

        return Cache.GetOrAdd(simpleName, static name =>
        {
            // Load by simple name; the runtime resolves to the shared-framework copy regardless
            // of the original request's PublicKeyToken/Version, which is exactly what we want
            // when the original strong-name binding fails.
            return Assembly.Load(new AssemblyName(name));
        });
    }

    private static Assembly? OnAssemblyResolve(object? sender, ResolveEventArgs args)
    {
        try
        {
            return Resolve(new AssemblyName(args.Name));
        }
        catch (FileNotFoundException)
        {
            return null;
        }
        catch (FileLoadException)
        {
            return null;
        }
        catch (BadImageFormatException)
        {
            return null;
        }
    }
}
