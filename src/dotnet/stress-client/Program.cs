using System.Diagnostics;
using System.Text;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

var serverAddress = args.Length > 0 ? args[0] : "http://localhost:50051";
var parallelism = args.Length > 1 ? int.Parse(args[1]) : 100;
var durationSeconds = args.Length > 2 ? int.Parse(args[2]) : 10;

Console.WriteLine($"LogRipper gRPC Stress Client");
Console.WriteLine($"  Target:      {serverAddress}");
Console.WriteLine($"  Parallelism: {parallelism}");
Console.WriteLine($"  Duration:    {durationSeconds}s");
Console.WriteLine();

using var channel = GrpcChannel.ForAddress(serverAddress);
var logbook = new LogbookService.LogbookServiceClient(channel);
var lookup = new LookupService.LookupServiceClient(channel);

var panics = new System.Collections.Concurrent.ConcurrentBag<string>();
var errors = new System.Collections.Concurrent.ConcurrentBag<string>();
var totalCalls = 0;
var internalErrors = 0;
var cts = new CancellationTokenSource(TimeSpan.FromSeconds(durationSeconds));

string[] adversarialStrings =
[
    "",
    " ",
    "\0",
    "\0\0\0\0\0\0\0\0",
    "\t\n\r",
    "\uFFFD\uFFFD",
    "\u200F\u202E",
    "\uFEFF",
    "\U0001F4A9\U0001F4A9",
    "A\u0301",
    new('X', 100_000),
    new('\u00FC', 50_000),
    "W1AW",
    "K7DBG/P",
    "VE3/W1AW/QRP",
    "-1",
    "NaN",
    "Infinity",
    "-Infinity",
    "20250230",
    "99991399",
];

byte[][] adversarialAdifPayloads =
[
    [],
    [0xFF, 0xFF, 0xFF],
    Enumerable.Range(0, 256).Select(i => (byte)i).ToArray(),
    Encoding.UTF8.GetBytes("<CALL:4>W1AW<BAND:3>20M<MODE:2>CW<QSO_DATE:8>20250115<TIME_ON:4>1200<STATION_CALLSIGN:4>TEST<eor>"),
    Encoding.UTF8.GetBytes($"<CALL:4>W1AW<QSO_DATE:8>202\u00fc123<TIME_ON:4>1200<BAND:3>20M<MODE:2>CW<STATION_CALLSIGN:4>TEST<eor>"),
    Encoding.UTF8.GetBytes("<CALL:-1>W1AW<eor>"),
    Encoding.UTF8.GetBytes("<CALL:999999999>W1AW<eor>"),
    Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat("<CALL:4>W1AW<BAND:3>20M<MODE:2>CW<QSO_DATE:8>20250115<TIME_ON:4>1200<STATION_CALLSIGN:4>TEST<eor>", 500))),
    new byte[1_000_000],
    Encoding.UTF8.GetBytes("<CALL:4>\U0001F4A9<eor>"),
];

async Task RunVector(string name, Func<CancellationToken, Task> action)
{
    var tasks = new List<Task>();
    for (int i = 0; i < parallelism; i++)
    {
        tasks.Add(Task.Run(async () =>
        {
            while (!cts.Token.IsCancellationRequested)
            {
                try
                {
                    await action(cts.Token);
                    Interlocked.Increment(ref totalCalls);
                }
                catch (RpcException ex) when (ex.StatusCode == StatusCode.Internal)
                {
                    Interlocked.Increment(ref internalErrors);
                    Interlocked.Increment(ref totalCalls);
                    panics.Add($"[{name}] INTERNAL: {ex.Status.Detail}");
                }
                catch (RpcException ex) when (ex.StatusCode == StatusCode.Unknown)
                {
                    Interlocked.Increment(ref internalErrors);
                    Interlocked.Increment(ref totalCalls);
                    panics.Add($"[{name}] UNKNOWN: {ex.Status.Detail}");
                }
                catch (RpcException)
                {
                    Interlocked.Increment(ref totalCalls);
                }
                catch (OperationCanceledException)
                {
                    break;
                }
                catch (Exception ex)
                {
                    errors.Add($"[{name}] {ex.GetType().Name}: {ex.Message}");
                    Interlocked.Increment(ref totalCalls);
                }
            }
        }));
    }
    await Task.WhenAll(tasks);
}

var random = new Random(42);

QsoRecord MakeAdversarialQso()
{
    var s = adversarialStrings[random.Next(adversarialStrings.Length)];
    var qso = new QsoRecord
    {
        StationCallsign = adversarialStrings[random.Next(adversarialStrings.Length)],
        WorkedCallsign = adversarialStrings[random.Next(adversarialStrings.Length)],
        Band = (Band)random.Next(-5, 40),
        Mode = (Mode)random.Next(-5, 100),
        UtcTimestamp = new Timestamp
        {
            Seconds = random.NextInt64(long.MinValue / 2, long.MaxValue / 2),
            Nanos = random.Next(int.MinValue, int.MaxValue),
        },
        Comment = s,
        Notes = s,
    };

    for (int i = 0; i < random.Next(0, 50); i++)
    {
        qso.ExtraFields[$"FIELD_{i}"] = adversarialStrings[random.Next(adversarialStrings.Length)];
    }

    return qso;
}

Console.WriteLine("Starting stress vectors...");
var sw = Stopwatch.StartNew();

var vectors = new List<Task>
{
    RunVector("LogQso-adversarial", async ct =>
    {
        await logbook.LogQsoAsync(new LogQsoRequest { Qso = MakeAdversarialQso() }, cancellationToken: ct);
    }),

    RunVector("LogQso-missing-fields", async ct =>
    {
        await logbook.LogQsoAsync(new LogQsoRequest { Qso = new QsoRecord() }, cancellationToken: ct);
    }),

    RunVector("UpdateQso-garbage", async ct =>
    {
        var qso = MakeAdversarialQso();
        qso.LocalId = adversarialStrings[random.Next(adversarialStrings.Length)];
        await logbook.UpdateQsoAsync(new UpdateQsoRequest { Qso = qso }, cancellationToken: ct);
    }),

    RunVector("DeleteQso-garbage", async ct =>
    {
        await logbook.DeleteQsoAsync(new DeleteQsoRequest
        {
            LocalId = adversarialStrings[random.Next(adversarialStrings.Length)]
        }, cancellationToken: ct);
    }),

    RunVector("GetQso-garbage", async ct =>
    {
        await logbook.GetQsoAsync(new GetQsoRequest
        {
            LocalId = adversarialStrings[random.Next(adversarialStrings.Length)]
        }, cancellationToken: ct);
    }),

    RunVector("ListQsos-chaos", async ct =>
    {
        await logbook.ListQsosAsync(new ListQsosRequest
        {
            CallsignFilter = adversarialStrings[random.Next(adversarialStrings.Length)],
            BandFilter = (Band)random.Next(-5, 40),
            ModeFilter = (Mode)random.Next(-5, 100),
            Limit = (uint)random.Next(0, 10000),
            Offset = (uint)random.Next(0, 10000),
        }, cancellationToken: ct);
    }),

    RunVector("Lookup-adversarial", async ct =>
    {
        await lookup.LookupAsync(new LookupRequest
        {
            Callsign = adversarialStrings[random.Next(adversarialStrings.Length)],
            SkipCache = random.Next(2) == 0,
        }, cancellationToken: ct);
    }),

    RunVector("StreamLookup-adversarial", async ct =>
    {
        using var stream = lookup.StreamLookup(new StreamLookupRequest
        {
            Callsign = adversarialStrings[random.Next(adversarialStrings.Length)],
            SkipCache = random.Next(2) == 0,
        }, cancellationToken: ct);
        while (await stream.ResponseStream.MoveNext(ct)) { }
    }),

    RunVector("ImportAdif-garbage", async ct =>
    {
        var payload = adversarialAdifPayloads[random.Next(adversarialAdifPayloads.Length)];
        await logbook.ImportAdifAsync(new ImportAdifRequest
        {
            Chunk = new AdifChunk { Data = ByteString.CopyFrom(payload) }
        }, cancellationToken: ct);
    }),

    RunVector("ExportAdif-chaos", async ct =>
    {
        await logbook.ExportAdifAsync(new ExportAdifRequest
        {
            IncludeHeader = random.Next(2) == 0,
            After = new Timestamp { Seconds = random.NextInt64(-1000, long.MaxValue / 2) },
        }, cancellationToken: ct);
    }),
};

await Task.WhenAll(vectors);
sw.Stop();

Console.WriteLine();
Console.WriteLine("============================================================");
Console.WriteLine("STRESS TEST RESULTS");
Console.WriteLine("============================================================");
Console.WriteLine($"  Duration:        {sw.Elapsed}");
Console.WriteLine($"  Total calls:     {totalCalls:N0}");
Console.WriteLine($"  INTERNAL errors: {internalErrors} (potential server panics)");
Console.WriteLine($"  Other errors:    {errors.Count}");
Console.WriteLine();

if (panics.IsEmpty)
{
    Console.WriteLine("No server panics detected (no INTERNAL/UNKNOWN gRPC status codes).");
}
else
{
    Console.WriteLine($"Found {panics.Count} potential server panic(s):");
    foreach (var p in panics.Distinct().Take(50))
    {
        Console.WriteLine($"  {p}");
    }
}

Console.WriteLine("============================================================");

return panics.IsEmpty ? 0 : 1;
