using System.Reflection;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods

// Regression test for issue #318:
// In published Release builds, Avalonia.Controls.DataGrid's editing path can fail to bind
// the System.ComponentModel.Annotations identity it was compiled against
// (Version=10.0.0.0, PublicKeyToken=b03f5f7f11d50a3a). FrameworkAssemblyResolver is the
// defensive bridge that satisfies that load by returning the shared-framework copy.
public sealed class FrameworkAssemblyResolverTests
{
    [Fact]
    public void Resolver_returns_framework_copy_for_crash_assembly_identity()
    {
        // The exact identity from the crash log in issue #318.
        var requested = new AssemblyName(
            "System.ComponentModel.Annotations, Version=10.0.0.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a");

        var resolved = FrameworkAssemblyResolver.Resolve(requested);

        Assert.NotNull(resolved);
        Assert.Equal("System.ComponentModel.Annotations", resolved!.GetName().Name);

        // Sanity: the resolved assembly must actually contain the EditableAttribute type
        // that DataGrid reflects on. If this is missing, the resolver returned the wrong
        // assembly and the underlying crash would still occur.
        var editable = resolved.GetType("System.ComponentModel.DataAnnotations.EditableAttribute");
        Assert.NotNull(editable);
    }

    [Fact]
    public void Resolver_returns_null_for_unrelated_assembly_requests()
    {
        var requested = new AssemblyName("Some.Unrelated.Assembly, Version=1.0.0.0, Culture=neutral, PublicKeyToken=null");

        var resolved = FrameworkAssemblyResolver.Resolve(requested);

        Assert.Null(resolved);
    }

    [Fact]
    public void Register_is_idempotent_and_safe_to_call_repeatedly()
    {
        // Should not throw or double-register. Verifying via repeated invocation; the resolver
        // itself uses Interlocked to guard the AppDomain hookup.
        FrameworkAssemblyResolver.Register();
        FrameworkAssemblyResolver.Register();
        FrameworkAssemblyResolver.Register();
    }
}
