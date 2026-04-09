using System.Text.Json;
using Google.Protobuf;
using LogRipper.DebugHost.Models;

namespace LogRipper.DebugHost.Services;

public sealed class ProtoJsonService
{
    private static readonly JsonFormatter Formatter = new(new JsonFormatter.Settings(true));

    public ProtoPayloadView Describe(IMessage message)
    {
        var json = Formatter.Format(message);
        return new ProtoPayloadView(PrettyPrintJson(json), Convert.ToBase64String(message.ToByteArray()), message.CalculateSize());
    }

    private static string PrettyPrintJson(string json)
    {
        using var document = JsonDocument.Parse(json);
        return JsonSerializer.Serialize(document.RootElement, new JsonSerializerOptions { WriteIndented = true });
    }
}
