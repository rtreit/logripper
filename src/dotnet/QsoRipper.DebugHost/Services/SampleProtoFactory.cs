using System.Collections;
using Google.Protobuf;
using Google.Protobuf.Reflection;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.DebugHost.Models;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class SampleProtoFactory
{
    private const int MaxPopulationDepth = 4;
    private const string SampleProfileName = "Home";
    private const string SampleLocalStationCallsign = "K7DBG";
    private const string SampleOperatorName = "Taylor Operator";
    private const string SampleGrid = "CN87up";
    private const string SampleCounty = "KING";
    private const string SampleState = "WA";
    private const string SampleCountry = "United States";
    private const string SampleContinent = "NA";
    private const string SampleQrzBaseUrl = "https://xmldata.qrz.com/xml/current/";
    private const int SampleDxcc = 291;
    private const int SampleCqZone = 3;
    private const int SampleItuZone = 6;
    private const double SampleLatitude = 47.6062;
    private const double SampleLongitude = -122.3321;

    private static readonly Dictionary<System.Type, Func<SampleProtoFactory, SampleGenerationContext, IMessage>> CustomBuilders =
        new Dictionary<System.Type, Func<SampleProtoFactory, SampleGenerationContext, IMessage>>
        {
            [typeof(LookupRequest)] = static (_, context) => CreateSampleLookupRequest(context),
            [typeof(LookupResult)] = static (_, context) => CreateSampleLookupResult(context),
            [typeof(CallsignRecord)] = static (_, context) => CreateSampleCallsignRecord(context),
            [typeof(StationProfile)] = static (_, context) => CreateSampleStationProfile(context),
            [typeof(StationSnapshot)] = static (_, context) => CreateSampleStationSnapshot(context),
            [typeof(QsoRecord)] = static (_, context) => CreateSampleQsoRecord(context),
            [typeof(DxccEntity)] = static (_, context) => CreateSampleDxccEntity(context)
        };

#pragma warning disable CA1822 // Mark members as static
    public LookupRequest CreateLookupRequest(string callsign, bool skipCache = false)
    {
        ArgumentNullException.ThrowIfNull(callsign);

        return new LookupRequest
        {
            Callsign = NormalizeCallsign(callsign),
            SkipCache = skipCache
        };
    }

    public CallsignRecord CreateCallsignRecord(string callsign)
    {
        ArgumentNullException.ThrowIfNull(callsign);

        var normalizedCallsign = NormalizeCallsign(callsign);
        return new CallsignRecord
        {
            Callsign = normalizedCallsign,
            CrossRef = normalizedCallsign,
            FirstName = "Taylor",
            LastName = "Operator",
            FormattedName = SampleOperatorName,
            Addr2 = "Seattle",
            State = SampleState,
            Country = SampleCountry,
            CountryCode = SampleDxcc,
            GridSquare = SampleGrid,
            GeoSource = (GeoSource)3,
            LicenseClass = "Extra",
            Eqsl = (QslPreference)1,
            Lotw = (QslPreference)1,
            PaperQsl = (QslPreference)2,
            CqZone = SampleCqZone,
            ItuZone = SampleItuZone,
            DxccCountryName = SampleCountry,
            DxccContinent = SampleContinent,
            QrzSerial = 1234567,
            LastModified = Timestamp.FromDateTime(DateTime.SpecifyKind(DateTime.UtcNow.AddDays(-2), DateTimeKind.Utc))
        };
    }

    public LookupResult CreateLookupResult(string callsign, bool cacheHit = true)
    {
        ArgumentNullException.ThrowIfNull(callsign);

        var normalizedCallsign = NormalizeCallsign(callsign);
        return new LookupResult
        {
            State = LookupState.Found,
            Record = CreateCallsignRecord(normalizedCallsign),
            CacheHit = cacheHit,
            LookupLatencyMs = 42,
            QueriedCallsign = normalizedCallsign
        };
    }

    public QsoRecord CreateQsoRecord(string workedCallsign)
    {
        ArgumentNullException.ThrowIfNull(workedCallsign);

        return CreateQsoRecord(workedCallsign, new QsoSampleOptions());
    }

    public QsoRecord CreateQsoRecord(string workedCallsign, QsoSampleOptions options)
    {
        ArgumentNullException.ThrowIfNull(workedCallsign);
        ArgumentNullException.ThrowIfNull(options);

        var utcNow = DateTime.SpecifyKind(DateTime.UtcNow, DateTimeKind.Utc);
        var normalizedCallsign = NormalizeCallsign(workedCallsign);
        var sampleRst = CreateSampleRst(options.Mode);
        var frequencyKhz = GetSampleFrequencyKhz(options.Band);
        var submode = GetSampleSubmode(options.Band, options.Mode);
        var record = new QsoRecord
        {
            LocalId = Guid.NewGuid().ToString("D"),
            StationCallsign = SampleLocalStationCallsign,
            WorkedCallsign = normalizedCallsign,
            UtcTimestamp = Timestamp.FromDateTime(utcNow),
            Band = options.Band,
            Mode = options.Mode,
            FrequencyKhz = frequencyKhz,
            RstSent = sampleRst.Sent,
            RstReceived = sampleRst.Received,
            TxPower = "100",
            WorkedOperatorName = SampleOperatorName,
            WorkedGrid = SampleGrid,
            WorkedCountry = SampleCountry,
            WorkedDxcc = SampleDxcc,
            WorkedContinent = SampleContinent,
            StationSnapshot = CreateStationSnapshot(),
            Notes = "Generated by the debug workbench.",
            Comment = "Sample QSO payload",
            CreatedAt = Timestamp.FromDateTime(utcNow),
            UpdatedAt = Timestamp.FromDateTime(utcNow),
            SyncStatus = (SyncStatus)0
        };

        if (!string.IsNullOrWhiteSpace(submode))
        {
            record.Submode = submode;
        }

        return record;
    }

    public StationProfile CreateStationProfile()
    {
        return new StationProfile
        {
            ProfileName = SampleProfileName,
            StationCallsign = SampleLocalStationCallsign,
            OperatorCallsign = SampleLocalStationCallsign,
            OperatorName = SampleOperatorName,
            Grid = SampleGrid,
            County = SampleCounty,
            State = SampleState,
            Country = SampleCountry,
            Dxcc = SampleDxcc,
            CqZone = SampleCqZone,
            ItuZone = SampleItuZone,
            Latitude = SampleLatitude,
            Longitude = SampleLongitude
        };
    }

    public StationSnapshot CreateStationSnapshot()
    {
        return new StationSnapshot
        {
            ProfileName = SampleProfileName,
            StationCallsign = SampleLocalStationCallsign,
            OperatorCallsign = SampleLocalStationCallsign,
            OperatorName = SampleOperatorName,
            Grid = SampleGrid,
            County = SampleCounty,
            State = SampleState,
            Country = SampleCountry,
            Dxcc = SampleDxcc,
            CqZone = SampleCqZone,
            ItuZone = SampleItuZone,
            Latitude = SampleLatitude,
            Longitude = SampleLongitude
        };
    }

    public DxccEntity CreateDxccEntity()
    {
        return new DxccEntity
        {
            DxccCode = SampleDxcc,
            CountryName = SampleCountry,
            Continent = SampleContinent,
            ItuZone = SampleItuZone,
            CqZone = SampleCqZone,
            UtcOffset = -8,
            Latitude = SampleLatitude,
            Longitude = SampleLongitude,
            Notes = "Sample DXCC entity for the debug workbench."
        };
    }

    public IMessage CreateSampleMessage(System.Type messageType, string callsign, QsoSampleOptions? qsoOptions = null)
    {
        ArgumentNullException.ThrowIfNull(messageType);
        ArgumentNullException.ThrowIfNull(callsign);

        var normalizedCallsign = NormalizeCallsign(callsign);
        var options = qsoOptions ?? new QsoSampleOptions();
        var context = SampleGenerationContext.Create(normalizedCallsign, options);
        return CreateSampleMessageCore(messageType, context, [], 0);
    }

    private IMessage CreateSampleMessageCore(
        System.Type messageType,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        if (!typeof(IMessage).IsAssignableFrom(messageType))
        {
            throw new ArgumentException($"'{messageType.FullName}' is not a protobuf message type.", nameof(messageType));
        }

        if (CustomBuilders.TryGetValue(messageType, out var builder))
        {
            var built = builder(this, context);
            EnsureMessageSerializesNonDefault(built, context, activeTypes, depth);
            return built;
        }

        if (messageType == typeof(Timestamp))
        {
            return Timestamp.FromDateTime(DateTime.SpecifyKind(DateTime.UtcNow.AddMinutes(-5), DateTimeKind.Utc));
        }

        if (messageType == typeof(Duration))
        {
            return Duration.FromTimeSpan(TimeSpan.FromSeconds(42));
        }

        if (Activator.CreateInstance(messageType) is IMessage message)
        {
            if (depth >= MaxPopulationDepth || !activeTypes.Add(messageType))
            {
                return message;
            }

            try
            {
                PopulateMessage(message, context, activeTypes, depth + 1);
                EnsureMessageSerializesNonDefault(message, context, activeTypes, depth + 1);
            }
            finally
            {
                activeTypes.Remove(messageType);
            }

            return message;
        }

        throw new InvalidOperationException($"Could not create sample message for '{messageType.FullName}'.");
    }
#pragma warning restore CA1822 // Mark members as static

    private void PopulateMessage(
        IMessage message,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        var selectedOneofFields = message.Descriptor.Oneofs
            .ToDictionary(static oneof => oneof, SampleGenerationContext.PickOneofField);
        foreach (var field in message.Descriptor.Fields.InFieldNumberOrder())
        {
            if (field.ContainingOneof is not null
                && selectedOneofFields.TryGetValue(field.ContainingOneof, out var selectedField)
                && !ReferenceEquals(field, selectedField))
            {
                continue;
            }

            PopulateField(message, field, context, activeTypes, depth);
        }
    }

    private void PopulateField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        if (field.IsMap)
        {
            PopulateMapField(message, field, context, activeTypes, depth);
            return;
        }

        if (field.IsRepeated)
        {
            PopulateRepeatedField(message, field, context, activeTypes, depth);
            return;
        }

        var value = CreateSampleValue(message, field, context, activeTypes, depth);
        if (value is not null)
        {
            field.Accessor.SetValue(message, value);
        }
    }

    private void EnsureMessageSerializesNonDefault(
        IMessage message,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        if (message.CalculateSize() > 0 || message.Descriptor.Fields.InFieldNumberOrder().Count == 0)
        {
            return;
        }

        foreach (var field in message.Descriptor.Fields.InFieldNumberOrder())
        {
            ForcePopulateField(message, field, context, activeTypes, depth);
            if (message.CalculateSize() > 0)
            {
                return;
            }
        }
    }

    private void ForcePopulateField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        if (field.IsMap)
        {
            ForcePopulateMapField(message, field, context, activeTypes, depth);
            return;
        }

        if (field.IsRepeated)
        {
            ForcePopulateRepeatedField(message, field, context, activeTypes, depth);
            return;
        }

        var value = CreateGuaranteedNonDefaultValue(message, field, context, activeTypes, depth);
        if (value is not null)
        {
            field.Accessor.SetValue(message, value);
        }
    }

    private void ForcePopulateRepeatedField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        var collection = field.Accessor.GetValue(message);
        var item = CreateGuaranteedNonDefaultValue(message, field, context, activeTypes, depth);
        if (collection is null || item is null)
        {
            return;
        }

        var addMethod = collection.GetType()
            .GetMethods()
            .FirstOrDefault(static method => method.Name == "Add" && method.GetParameters().Length == 1)
            ?? throw new InvalidOperationException($"Could not find Add(T) on repeated field '{field.FullName}'.");
        addMethod.Invoke(collection, [item]);
    }

    private void ForcePopulateMapField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        var map = field.Accessor.GetValue(message);
        var keyField = field.MessageType.FindFieldByNumber(1)
            ?? throw new InvalidOperationException($"Map field '{field.FullName}' is missing a key descriptor.");
        var valueField = field.MessageType.FindFieldByNumber(2)
            ?? throw new InvalidOperationException($"Map field '{field.FullName}' is missing a value descriptor.");
        var key = CreateGuaranteedNonDefaultValue(message, keyField, context, activeTypes, depth);
        var value = CreateGuaranteedNonDefaultValue(message, valueField, context, activeTypes, depth);
        if (map is null || key is null || value is null)
        {
            return;
        }

        var addMethod = map.GetType()
            .GetMethods()
            .FirstOrDefault(static method => method.Name == "Add" && method.GetParameters().Length == 2)
            ?? throw new InvalidOperationException($"Could not find Add(TKey, TValue) on map field '{field.FullName}'.");
        addMethod.Invoke(map, [key, value]);
    }

    private void PopulateRepeatedField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        var collection = field.Accessor.GetValue(message);
        var item = CreateSampleValue(message, field, context, activeTypes, depth);
        if (collection is null || item is null)
        {
            return;
        }

        var addMethod = collection.GetType()
            .GetMethods()
            .FirstOrDefault(static method => method.Name == "Add" && method.GetParameters().Length == 1)
            ?? throw new InvalidOperationException($"Could not find Add(T) on repeated field '{field.FullName}'.");
        addMethod.Invoke(collection, [item]);
    }

    private void PopulateMapField(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        var map = field.Accessor.GetValue(message);
        var keyField = field.MessageType.FindFieldByNumber(1)
            ?? throw new InvalidOperationException($"Map field '{field.FullName}' is missing a key descriptor.");
        var valueField = field.MessageType.FindFieldByNumber(2)
            ?? throw new InvalidOperationException($"Map field '{field.FullName}' is missing a value descriptor.");
        var key = CreateSampleMapValue(message, keyField, context, activeTypes, depth);
        var value = CreateSampleMapValue(message, valueField, context, activeTypes, depth);
        if (map is null || key is null || value is null)
        {
            return;
        }

        var addMethod = map.GetType()
            .GetMethods()
            .FirstOrDefault(static method => method.Name == "Add" && method.GetParameters().Length == 2)
            ?? throw new InvalidOperationException($"Could not find Add(TKey, TValue) on map field '{field.FullName}'.");
        addMethod.Invoke(map, [key, value]);
    }

    private object? CreateSampleMapValue(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        return CreateSampleValue(message, field, context, activeTypes, depth);
    }

    private object? CreateSampleValue(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        return field.FieldType switch
        {
            FieldType.String => CreateSampleString(field, context),
            FieldType.Bool => CreateSampleBool(field, context),
            FieldType.Int32 or FieldType.SInt32 or FieldType.SFixed32 => CreateSampleInt32(field, context),
            FieldType.UInt32 or FieldType.Fixed32 => unchecked((uint)CreateSamplePositiveInt(field, context)),
            FieldType.Int64 or FieldType.SInt64 or FieldType.SFixed64 => CreateSampleInt64(field, context),
            FieldType.UInt64 or FieldType.Fixed64 => unchecked((ulong)CreateSamplePositiveInt64(field, context)),
            FieldType.Float => (float)CreateSampleDouble(field, context),
            FieldType.Double => CreateSampleDouble(field, context),
            FieldType.Bytes => ByteString.CopyFromUtf8($"sample-{field.JsonName}-{context.GenerationToken[..6]}"),
            FieldType.Enum => CreateSampleEnum(message, field, context),
            FieldType.Message => CreateSampleNestedMessage(field, context, activeTypes, depth),
            _ => null
        };
    }

    private object? CreateGuaranteedNonDefaultValue(
        IMessage message,
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        return field.FieldType switch
        {
            FieldType.String => $"sample-{field.JsonName}-{context.GenerationToken[..6]}",
            FieldType.Bool => true,
            FieldType.Int32 or FieldType.SInt32 or FieldType.SFixed32 => 1,
            FieldType.UInt32 or FieldType.Fixed32 => 1U,
            FieldType.Int64 or FieldType.SInt64 or FieldType.SFixed64 => 1L,
            FieldType.UInt64 or FieldType.Fixed64 => 1UL,
            FieldType.Float => 1F,
            FieldType.Double => 1D,
            FieldType.Bytes => ByteString.CopyFromUtf8($"sample-{field.JsonName}-{context.GenerationToken[..6]}"),
            FieldType.Enum => CreateGuaranteedEnumValue(message, field),
            FieldType.Message => CreateSampleNestedMessage(field, context, activeTypes, depth),
            _ => null
        };
    }

    private static object CreateGuaranteedEnumValue(IMessage message, FieldDescriptor field)
    {
        var enumValue = field.EnumType.Values.FirstOrDefault(static value => value.Number != 0)
            ?? field.EnumType.Values[0];
        var enumType = field.Accessor.GetValue(message).GetType();
        return System.Enum.ToObject(enumType, enumValue.Number);
    }

    private IMessage? CreateSampleNestedMessage(
        FieldDescriptor field,
        SampleGenerationContext context,
        HashSet<System.Type> activeTypes,
        int depth)
    {
        if (depth >= MaxPopulationDepth)
        {
            return null;
        }

        var messageType = field.MessageType.ClrType;
        if (messageType == typeof(Empty))
        {
            return new Empty();
        }

        if (messageType == typeof(Any))
        {
            return Any.Pack(CreateSampleLookupRequest(context));
        }

        return CreateSampleMessageCore(messageType, context, activeTypes, depth);
    }

    private static object CreateSampleEnum(IMessage message, FieldDescriptor field, SampleGenerationContext context)
    {
        var preferredValue = field.Name switch
        {
            "storage_backend" => field.EnumType.Values.FirstOrDefault(value =>
                value.Name.Contains(context.PreferSqlite ? "SQLITE" : "MEMORY", StringComparison.Ordinal)),
            "state" => field.EnumType.Values.FirstOrDefault(static value => value.Name.Contains("FOUND", StringComparison.Ordinal)),
            "band" => field.EnumType.Values.FirstOrDefault(value => value.Number == (int)context.Options.Band),
            "mode" => field.EnumType.Values.FirstOrDefault(value => value.Number == (int)context.Options.Mode),
            _ => null
        };
        var enumValue = preferredValue
            ?? field.EnumType.Values.FirstOrDefault(static value => value.Number != 0 && !value.Name.Contains("UNSPECIFIED", StringComparison.Ordinal))
            ?? field.EnumType.Values[0];
        var enumType = field.Accessor.GetValue(message).GetType();
        return System.Enum.ToObject(enumType, enumValue.Number);
    }

    private static string CreateSampleString(FieldDescriptor field, SampleGenerationContext context)
    {
        return field.Name switch
        {
            "profile_id" or "active_profile_id" or "persisted_active_profile_id" => context.ProfileId,
            "profile_name" => context.ProfileName,
            "station_callsign" => context.Station.StationCallsign,
            "operator_callsign" => context.Station.OperatorCallsign,
            "worked_callsign" or "callsign" or "queried_callsign" => context.InputCallsign,
            "operator_name" or "formatted_name" => context.OperatorName,
            "first_name" => context.FirstName,
            "last_name" => context.LastName,
            "grid" or "grid_square" or "worked_grid" => context.Station.Grid,
            "county" => context.Station.County,
            "state" => context.Station.State,
            "country" or "country_name" or "worked_country" or "dxcc_country_name" => context.Station.Country,
            "continent" or "worked_continent" or "dxcc_continent" => context.Station.Continent,
            "sqlite_path" => context.SqlitePath,
            "username" => context.Station.OperatorCallsign,
            "password" => $"demo-{context.GenerationToken[..8]}",
            "user_agent" => context.UserAgent,
            "base_url" => SampleQrzBaseUrl,
            "session_key" => context.SessionKey,
            "local_id" => context.LocalId,
            "tx_power" => context.TxPower,
            "notes" => context.Note,
            "comment" => context.Comment,
            "message" or "error" => context.ErrorMessage,
            "summary" => context.Summary,
            "warning" => context.Warning,
            "addr2" => context.Addr2,
            "license_class" => context.LicenseClass,
            _ when field.Name.Contains("callsign", StringComparison.Ordinal) => context.InputCallsign,
            _ when field.Name.Contains("grid", StringComparison.Ordinal) => context.Station.Grid,
            _ when field.Name.Contains("country", StringComparison.Ordinal) => context.Station.Country,
            _ when field.Name.Contains("state", StringComparison.Ordinal) => context.Station.State,
            _ when field.Name.Contains("county", StringComparison.Ordinal) => context.Station.County,
            _ when field.Name.Contains("continent", StringComparison.Ordinal) => context.Station.Continent,
            _ when field.Name.Contains("username", StringComparison.Ordinal) => context.Station.OperatorCallsign,
            _ when field.Name.Contains("password", StringComparison.Ordinal) => $"demo-{context.GenerationToken[..8]}",
            _ when field.Name.Contains("base_url", StringComparison.Ordinal) => SampleQrzBaseUrl,
            _ when field.Name.Contains("sqlite", StringComparison.Ordinal) && field.Name.Contains("path", StringComparison.Ordinal) => context.SqlitePath,
            _ when field.Name.Contains("agent", StringComparison.Ordinal) => context.UserAgent,
            _ when field.Name.Contains("session", StringComparison.Ordinal) && field.Name.Contains("key", StringComparison.Ordinal) => context.SessionKey,
            _ when field.Name.Contains("query", StringComparison.Ordinal) => context.InputCallsign,
            _ when field.Name.Contains("warning", StringComparison.Ordinal) => context.Warning,
            _ when field.Name.Contains("error", StringComparison.Ordinal) => context.ErrorMessage,
            _ when field.Name.Contains("summary", StringComparison.Ordinal) => context.Summary,
            _ when field.Name.Contains("note", StringComparison.Ordinal) => context.Note,
            _ when field.Name.Contains("comment", StringComparison.Ordinal) => context.Comment,
            _ when field.Name.Contains("mode", StringComparison.Ordinal) => GetSampleSubmode(context.Options.Band, context.Options.Mode) ?? "USB",
            _ => $"sample-{field.JsonName}-{context.GenerationToken[..6]}"
        };
    }

    private static bool CreateSampleBool(FieldDescriptor field, SampleGenerationContext context)
    {
        return field.Name switch
        {
            "cache_hit" => context.CacheHit,
            "skip_cache" => context.SkipCache,
            "setup_complete" => context.SetupComplete,
            "has_session_override" => context.HasSessionOverride,
            "make_active" or "is_active" => context.MakeActive,
            _ when field.Name.Contains("active", StringComparison.Ordinal) => SampleGenerationContext.NextBool(),
            _ when field.Name.StartsWith("has_", StringComparison.Ordinal) => SampleGenerationContext.NextBool(),
            _ when field.Name.StartsWith("is_", StringComparison.Ordinal) => SampleGenerationContext.NextBool(),
            _ when field.Name.Contains("enabled", StringComparison.Ordinal) => true,
            _ when field.Name.Contains("complete", StringComparison.Ordinal) => true,
            _ => SampleGenerationContext.NextBool()
        };
    }

    private static int CreateSampleInt32(FieldDescriptor field, SampleGenerationContext context)
    {
        return field.Name switch
        {
            "dxcc" or "country_code" or "worked_dxcc" or "dxcc_code" => context.Station.Dxcc,
            "cq_zone" => context.Station.CqZone,
            "itu_zone" => context.Station.ItuZone,
            "lookup_latency_ms" => context.LookupLatencyMs,
            "http_timeout_seconds" => context.HttpTimeoutSeconds,
            "max_retries" => context.MaxRetries,
            "qrz_serial" => context.QrzSerial,
            _ when field.Name.Contains("count", StringComparison.Ordinal) => context.Count,
            _ when field.Name.Contains("offset", StringComparison.Ordinal) => context.NegativeOffsetHours,
            _ when field.Name.Contains("port", StringComparison.Ordinal) => 50051,
            _ => SampleGenerationContext.NextInt(1, 10)
        };
    }

    private static int CreateSamplePositiveInt(FieldDescriptor field, SampleGenerationContext context)
    {
        return Math.Abs(CreateSampleInt32(field, context));
    }

    private static long CreateSampleInt64(FieldDescriptor field, SampleGenerationContext context)
    {
        return field.Name switch
        {
            "frequency_khz" => checked((long)(GetSampleFrequencyKhz(context.Options.Band) + (ulong)SampleGenerationContext.NextInt(0, 25))),
            _ => CreateSampleInt32(field, context)
        };
    }

    private static long CreateSamplePositiveInt64(FieldDescriptor field, SampleGenerationContext context)
    {
        return Math.Abs(CreateSampleInt64(field, context));
    }

    private static double CreateSampleDouble(FieldDescriptor field, SampleGenerationContext context)
    {
        return field.Name switch
        {
            "latitude" => context.Latitude,
            "longitude" => context.Longitude,
            "utc_offset" => context.NegativeOffsetHours,
            _ => SampleGenerationContext.NextDouble(1, 100)
        };
    }

    private static LookupRequest CreateSampleLookupRequest(SampleGenerationContext context)
    {
        return new LookupRequest
        {
            Callsign = context.InputCallsign,
            SkipCache = context.SkipCache
        };
    }

    private static CallsignRecord CreateSampleCallsignRecord(SampleGenerationContext context)
    {
        return new CallsignRecord
        {
            Callsign = context.InputCallsign,
            CrossRef = context.InputCallsign,
            Aliases = { $"{context.InputCallsign}/P" },
            PreviousCall = $"{context.InputCallsign[..Math.Min(2, context.InputCallsign.Length)]}1OLD",
            DxccEntityId = checked((uint)context.Station.Dxcc),
            FirstName = context.FirstName,
            LastName = context.LastName,
            FormattedName = context.OperatorName,
            Addr2 = context.Addr2,
            State = context.Station.State,
            Country = context.Station.Country,
            CountryCode = checked((uint)context.Station.Dxcc),
            Latitude = context.Latitude,
            Longitude = context.Longitude,
            GridSquare = context.Station.Grid,
            County = context.Station.County,
            GeoSource = (GeoSource)3,
            LicenseClass = context.LicenseClass,
            EffectiveDate = Timestamp.FromDateTime(context.GeneratedAtUtc.AddYears(-6)),
            ExpirationDate = Timestamp.FromDateTime(context.GeneratedAtUtc.AddYears(4)),
            Eqsl = (QslPreference)1,
            Lotw = (QslPreference)1,
            PaperQsl = (QslPreference)2,
            CqZone = checked((uint)context.Station.CqZone),
            ItuZone = checked((uint)context.Station.ItuZone),
            DxccCountryName = context.Station.Country,
            DxccContinent = context.Station.Continent,
            BirthYear = 1984,
            QrzSerial = checked((uint)context.QrzSerial),
            LastModified = Timestamp.FromDateTime(context.LastModifiedUtc),
            TimeZone = $"UTC{context.NegativeOffsetHours}",
            GmtOffset = context.NegativeOffsetHours,
            ProfileViews = 1200
        };
    }

    private static LookupResult CreateSampleLookupResult(SampleGenerationContext context)
    {
        return new LookupResult
        {
            State = LookupState.Found,
            Record = CreateSampleCallsignRecord(context),
            CacheHit = context.CacheHit,
            LookupLatencyMs = checked((uint)context.LookupLatencyMs),
            QueriedCallsign = context.InputCallsign
        };
    }

    private static QsoRecord CreateSampleQsoRecord(SampleGenerationContext context)
    {
        var sampleRst = CreateSampleRst(context.Options.Mode);
        var frequencyKhz = GetSampleFrequencyKhz(context.Options.Band) + (ulong)SampleGenerationContext.NextInt(0, 25);
        var submode = GetSampleSubmode(context.Options.Band, context.Options.Mode);
        var record = new QsoRecord
        {
            LocalId = context.LocalId,
            StationCallsign = context.Station.StationCallsign,
            WorkedCallsign = context.InputCallsign,
            UtcTimestamp = Timestamp.FromDateTime(context.GeneratedAtUtc),
            Band = context.Options.Band,
            Mode = context.Options.Mode,
            FrequencyKhz = frequencyKhz,
            RstSent = sampleRst.Sent,
            RstReceived = sampleRst.Received,
            TxPower = context.TxPower,
            WorkedOperatorName = context.OperatorName,
            WorkedGrid = context.Station.Grid,
            WorkedCountry = context.Station.Country,
            WorkedDxcc = checked((uint)context.Station.Dxcc),
            WorkedContinent = context.Station.Continent,
            StationSnapshot = CreateSampleStationSnapshot(context),
            Notes = context.Note,
            Comment = context.Comment,
            CreatedAt = Timestamp.FromDateTime(context.GeneratedAtUtc),
            UpdatedAt = Timestamp.FromDateTime(context.UpdatedAtUtc),
            SyncStatus = SyncStatus.LocalOnly
        };

        if (!string.IsNullOrWhiteSpace(submode))
        {
            record.Submode = submode;
        }

        return record;
    }

    private static StationProfile CreateSampleStationProfile(SampleGenerationContext context)
    {
        return new StationProfile
        {
            ProfileName = context.ProfileName,
            StationCallsign = context.Station.StationCallsign,
            OperatorCallsign = context.Station.OperatorCallsign,
            OperatorName = context.OperatorName,
            Grid = context.Station.Grid,
            County = context.Station.County,
            State = context.Station.State,
            Country = context.Station.Country,
            Dxcc = checked((uint)context.Station.Dxcc),
            CqZone = checked((uint)context.Station.CqZone),
            ItuZone = checked((uint)context.Station.ItuZone),
            Latitude = context.Latitude,
            Longitude = context.Longitude
        };
    }

    private static StationSnapshot CreateSampleStationSnapshot(SampleGenerationContext context)
    {
        return new StationSnapshot
        {
            ProfileName = context.ProfileName,
            StationCallsign = context.Station.StationCallsign,
            OperatorCallsign = context.Station.OperatorCallsign,
            OperatorName = context.OperatorName,
            Grid = context.Station.Grid,
            County = context.Station.County,
            State = context.Station.State,
            Country = context.Station.Country,
            Dxcc = checked((uint)context.Station.Dxcc),
            CqZone = checked((uint)context.Station.CqZone),
            ItuZone = checked((uint)context.Station.ItuZone),
            Latitude = context.Latitude,
            Longitude = context.Longitude
        };
    }

    private static DxccEntity CreateSampleDxccEntity(SampleGenerationContext context)
    {
        return new DxccEntity
        {
            DxccCode = checked((uint)context.Station.Dxcc),
            CountryName = context.Station.Country,
            Continent = context.Station.Continent,
            ItuZone = checked((uint)context.Station.ItuZone),
            CqZone = checked((uint)context.Station.CqZone),
            UtcOffset = context.NegativeOffsetHours,
            Latitude = context.Latitude,
            Longitude = context.Longitude,
            Notes = context.Note
        };
    }

    private static (RstReport Sent, RstReport Received) CreateSampleRst(Mode mode)
    {
        return mode switch
        {
            Mode.Cw => (
                new RstReport { Raw = "599", Readability = 5, Strength = 9, Tone = 9 },
                new RstReport { Raw = "579", Readability = 5, Strength = 7, Tone = 9 }),
            Mode.Ft8 or Mode.Rtty or Mode.Psk or Mode.Mfsk => (
                new RstReport { Raw = "-08" },
                new RstReport { Raw = "-12" }),
            _ => (
                new RstReport { Raw = "59", Readability = 5, Strength = 9 },
                new RstReport { Raw = "57", Readability = 5, Strength = 7 })
        };
    }

    private static ulong GetSampleFrequencyKhz(Band band)
    {
        return band switch
        {
            Band._160M => 1900,
            Band._80M => 3900,
            Band._40M => 7100,
            Band._30M => 10136,
            Band._20M => 14250,
            Band._17M => 18100,
            Band._15M => 21250,
            Band._12M => 24940,
            Band._10M => 28400,
            Band._6M => 50125,
            Band._2M => 146520,
            Band._70Cm => 446000,
            _ => 14250
        };
    }

    private static string? GetSampleSubmode(Band band, Mode mode)
    {
        return mode switch
        {
            Mode.Ssb => band is Band._160M or Band._80M or Band._40M ? "LSB" : "USB",
            Mode.Ft8 => "FT8",
            _ => null
        };
    }

    private static string NormalizeCallsign(string? callsign)
    {
        return string.IsNullOrWhiteSpace(callsign)
            ? "K7DBG"
            : callsign.Trim().ToUpperInvariant();
    }
}
