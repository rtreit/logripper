using System.ComponentModel.DataAnnotations;
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
    // Model deliberately uses [Editable] - the exact attribute Avalonia.Controls.DataGrid
    // reflects on in GetPropertyIsReadOnly. Reflecting on it forces a runtime load of
    // System.ComponentModel.Annotations, which is what triggered the production crash.
    private sealed class EditableSampleRow
    {
        [Editable(allowEdit: false)]
        public string Callsign { get; set; } = "K1ABC";
    }

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

    [Fact]
    public void DataGrid_style_attribute_reflection_does_not_throw_with_resolver_registered()
    {
        // This mirrors the exact reflection call Avalonia.Controls.DataGrid.DataGridDataConnection
        // makes inside GetPropertyIsReadOnly: it asks a bound property for its [Editable]
        // attributes. JIT-time resolution of typeof(EditableAttribute) plus the reflection call
        // forces a runtime load of System.ComponentModel.Annotations, which is the load that
        // failed with FileNotFoundException in issue #318.
        FrameworkAssemblyResolver.Register();

        var prop = typeof(EditableSampleRow).GetProperty(nameof(EditableSampleRow.Callsign));
        Assert.NotNull(prop);

        // The act of calling GetCustomAttributes with typeof(EditableAttribute) is what
        // triggered the crashing assembly load in production.
        var attrs = prop!.GetCustomAttributes(typeof(EditableAttribute), inherit: true);

        Assert.Single(attrs);
        var editable = Assert.IsType<EditableAttribute>(attrs[0]);
        Assert.False(editable.AllowEdit);

        // EditableAttribute lives in System.ComponentModel.Annotations - confirm the load
        // succeeded and the resolved assembly is the shared-framework copy.
        var hostingAssembly = typeof(EditableAttribute).Assembly;
        Assert.Equal("System.ComponentModel.Annotations", hostingAssembly.GetName().Name);
    }
}

