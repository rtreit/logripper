using System.Text.Json;
using Google.Protobuf;
using QsoRipper.DebugHost.Models;

namespace QsoRipper.DebugHost.Services;

internal sealed class ProtoJsonService
{
    private static readonly JsonFormatter Formatter = new(new JsonFormatter.Settings(true));
    private static readonly JsonSerializerOptions PrettyPrintOptions = new() { WriteIndented = true };

#pragma warning disable CA1822 // Mark members as static
    public ProtoPayloadView Describe(IMessage message)
    {
        ArgumentNullException.ThrowIfNull(message);

        var json = Formatter.Format(message);
        return new ProtoPayloadView(PrettyPrintJson(json), Convert.ToBase64String(message.ToByteArray()), message.CalculateSize());
    }
#pragma warning restore CA1822 // Mark members as static

    private static string PrettyPrintJson(string json)
    {
        ArgumentNullException.ThrowIfNull(json);

        using var document = JsonDocument.Parse(json);
        return JsonSerializer.Serialize(document.RootElement, PrettyPrintOptions);
    }
}
