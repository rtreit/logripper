using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class ImportAdifCommand
{
    private const int ChunkSize = 65536;

    public static async Task<int> RunAsync(GrpcChannel channel, string filePath)
    {
        if (!File.Exists(filePath))
        {
            Console.Error.WriteLine($"File not found: {filePath}");
            return 1;
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ImportAdif();
        var fileBytes = await File.ReadAllBytesAsync(filePath);

        for (var offset = 0; offset < fileBytes.Length; offset += ChunkSize)
        {
            var length = Math.Min(ChunkSize, fileBytes.Length - offset);
            var chunk = new AdifChunk { Data = Google.Protobuf.ByteString.CopyFrom(fileBytes, offset, length) };
            await call.RequestStream.WriteAsync(new ImportAdifRequest { Chunk = chunk });
        }

        await call.RequestStream.CompleteAsync();
        var response = await call.ResponseAsync;

        Console.WriteLine($"Imported:  {response.RecordsImported}");
        Console.WriteLine($"Skipped:   {response.RecordsSkipped}");

        foreach (var warning in response.Warnings)
        {
            Console.WriteLine($"  Warning: {warning}");
        }

        return 0;
    }
}
