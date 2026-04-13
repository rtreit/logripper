using System.Runtime.CompilerServices;
using Google.Protobuf;
using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

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
        await using var input = File.OpenRead(filePath);

        await foreach (var request in ReadRequestsAsync(input))
        {
            await call.RequestStream.WriteAsync(request);
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

    internal static async IAsyncEnumerable<ImportAdifRequest> ReadRequestsAsync(
        Stream input,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(input);

        var buffer = new byte[ChunkSize];

        while (true)
        {
            var bytesRead = await input.ReadAsync(buffer.AsMemory(0, buffer.Length), cancellationToken);
            if (bytesRead == 0)
            {
                yield break;
            }

            yield return new ImportAdifRequest
            {
                Chunk = new AdifChunk
                {
                    Data = ByteString.CopyFrom(buffer, 0, bytesRead)
                }
            };
        }
    }
}
