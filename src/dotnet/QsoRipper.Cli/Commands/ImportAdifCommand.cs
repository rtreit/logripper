using System.Runtime.CompilerServices;
using Google.Protobuf;
using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class ImportAdifCommand
{
    private const int ChunkSize = 65536;

    public static async Task<int> RunAsync(
        GrpcChannel channel,
        string filePath,
        bool refresh,
        CancellationToken cancellationToken = default)
    {
        if (!File.Exists(filePath))
        {
            Console.Error.WriteLine($"File not found: {filePath}");
            return 1;
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ImportAdif(cancellationToken: cancellationToken);
        await using var input = File.OpenRead(filePath);

        await foreach (var request in ReadRequestsAsync(input, refresh, cancellationToken))
        {
            cancellationToken.ThrowIfCancellationRequested();
#pragma warning disable CA2016 // gRPC stream writer doesn't expose a CancellationToken overload
            await call.RequestStream.WriteAsync(request);
#pragma warning restore CA2016
        }

        await call.RequestStream.CompleteAsync().WaitAsync(cancellationToken);
        var response = await call.ResponseAsync.WaitAsync(cancellationToken);

        Console.WriteLine($"Imported:  {response.RecordsImported}");
        Console.WriteLine($"Updated:   {response.RecordsUpdated}");
        Console.WriteLine($"Skipped:   {response.RecordsSkipped}");

        foreach (var warning in response.Warnings)
        {
            Console.WriteLine($"  Warning: {warning}");
        }

        return 0;
    }

    internal static async IAsyncEnumerable<ImportAdifRequest> ReadRequestsAsync(
        Stream input,
        bool refresh = false,
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(input);

        var buffer = new byte[ChunkSize];
        var isFirst = true;

        while (true)
        {
            var bytesRead = await input.ReadAsync(buffer.AsMemory(0, buffer.Length), cancellationToken);
            if (bytesRead == 0)
            {
                yield break;
            }

            var request = new ImportAdifRequest
            {
                Chunk = new AdifChunk
                {
                    Data = ByteString.CopyFrom(buffer, 0, bytesRead)
                }
            };

            if (isFirst && refresh)
            {
                request.Refresh = true;
                isFirst = false;
            }

            yield return request;
        }
    }
}
