using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class ExportAdifCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string[] args)
    {
        string? outputFile = null;
        var includeHeader = false;

        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--file" when i < args.Length - 1:
                    outputFile = args[++i];
                    break;
                case "--include-header":
                    includeHeader = true;
                    break;
            }
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ExportAdif(new ExportAdifRequest { IncludeHeader = includeHeader });
        using var output = outputFile is not null
            ? new FileStream(outputFile, FileMode.Create, FileAccess.Write)
            : Console.OpenStandardOutput();

        while (await call.ResponseStream.MoveNext(CancellationToken.None))
        {
            var chunk = call.ResponseStream.Current.Chunk;
            if (chunk is not null)
            {
                await output.WriteAsync(chunk.Data.Memory);
            }
        }

        if (outputFile is not null)
        {
            Console.WriteLine($"Exported to {outputFile}");
        }

        return 0;
    }
}
