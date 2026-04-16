using System.Diagnostics.CodeAnalysis;
using Google.Protobuf;
using Grpc.Core;
using QsoRipper.Engine.Lookup;
using QsoRipper.Services;

namespace QsoRipper.Engine.DotNet;

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedEngineInfoGrpcService
    : EngineService.EngineServiceBase
{
    public override Task<GetEngineInfoResponse> GetEngineInfo(
        GetEngineInfoRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetEngineInfoResponse
        {
            Engine = ManagedEngineState.BuildEngineInfo(),
        });
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedSetupGrpcService(ManagedEngineState state)
    : SetupService.SetupServiceBase
{
    public override Task<GetSetupStatusResponse> GetSetupStatus(
        GetSetupStatusRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetSetupStatusResponse
        {
            Status = state.GetSetupStatus(),
        });
    }

    public override Task<GetSetupWizardStateResponse> GetSetupWizardState(
        GetSetupWizardStateRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.GetSetupWizardState());
    }

    public override Task<ValidateSetupStepResponse> ValidateSetupStep(
        ValidateSetupStepRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(ManagedEngineState.ValidateSetupStep(request));
    }

    public override Task<TestQrzCredentialsResponse> TestQrzCredentials(
        TestQrzCredentialsRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(ManagedEngineState.TestQrzCredentials(request.QrzXmlUsername, request.QrzXmlPassword));
    }

    public override Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentials(
        TestQrzLogbookCredentialsRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.TestQrzLogbookCredentials(request.ApiKey));
    }

    public override Task<SaveSetupResponse> SaveSetup(
        SaveSetupRequest request,
        ServerCallContext context)
    {
        try
        {
            return Task.FromResult(state.SaveSetup(request));
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedStationProfileGrpcService(ManagedEngineState state)
    : StationProfileService.StationProfileServiceBase
{
    public override Task<ListStationProfilesResponse> ListStationProfiles(
        ListStationProfilesRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.ListStationProfiles());
    }

    public override Task<GetStationProfileResponse> GetStationProfile(
        GetStationProfileRequest request,
        ServerCallContext context)
    {
        var profile = state.GetStationProfile(request.ProfileId);
        if (profile is null)
        {
            throw new RpcException(new Status(StatusCode.NotFound, $"Station profile '{request.ProfileId}' was not found."));
        }

        return Task.FromResult(new GetStationProfileResponse { Profile = profile });
    }

    public override Task<SaveStationProfileResponse> SaveStationProfile(
        SaveStationProfileRequest request,
        ServerCallContext context)
    {
        try
        {
            return Task.FromResult(state.SaveStationProfile(request));
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    public override Task<DeleteStationProfileResponse> DeleteStationProfile(
        DeleteStationProfileRequest request,
        ServerCallContext context)
    {
        var deleted = state.DeleteStationProfile(request.ProfileId);
        if (!deleted)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, $"Station profile '{request.ProfileId}' could not be deleted."));
        }

        var response = new DeleteStationProfileResponse();
        var catalog = state.ListStationProfiles();
        if (!string.IsNullOrWhiteSpace(catalog.ActiveProfileId))
        {
            response.ActiveProfileId = catalog.ActiveProfileId;
        }

        return Task.FromResult(response);
    }

    public override Task<SetActiveStationProfileResponse> SetActiveStationProfile(
        SetActiveStationProfileRequest request,
        ServerCallContext context)
    {
        var profile = state.SetActiveStationProfile(request.ProfileId);
        if (profile is null)
        {
            throw new RpcException(new Status(StatusCode.NotFound, $"Station profile '{request.ProfileId}' was not found."));
        }

        return Task.FromResult(new SetActiveStationProfileResponse { Profile = profile });
    }

    public override Task<GetActiveStationContextResponse> GetActiveStationContext(
        GetActiveStationContextRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetActiveStationContextResponse
        {
            Context = state.GetActiveStationContext(),
        });
    }

    public override Task<SetSessionStationProfileOverrideResponse> SetSessionStationProfileOverride(
        SetSessionStationProfileOverrideRequest request,
        ServerCallContext context)
    {
        if (request.Profile is null)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, "profile is required."));
        }

        return Task.FromResult(new SetSessionStationProfileOverrideResponse
        {
            Context = state.SetSessionStationProfileOverride(request.Profile),
        });
    }

    public override Task<ClearSessionStationProfileOverrideResponse> ClearSessionStationProfileOverride(
        ClearSessionStationProfileOverrideRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new ClearSessionStationProfileOverrideResponse
        {
            Context = state.ClearSessionStationProfileOverride(),
        });
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedDeveloperControlGrpcService(ManagedEngineState state)
    : DeveloperControlService.DeveloperControlServiceBase
{
    public override Task<GetRuntimeConfigResponse> GetRuntimeConfig(
        GetRuntimeConfigRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetRuntimeConfigResponse
        {
            Snapshot = state.GetRuntimeConfigSnapshot(),
        });
    }

    public override Task<ApplyRuntimeConfigResponse> ApplyRuntimeConfig(
        ApplyRuntimeConfigRequest request,
        ServerCallContext context)
    {
        try
        {
            return Task.FromResult(new ApplyRuntimeConfigResponse
            {
                Snapshot = state.ApplyRuntimeConfig(request.Mutations),
            });
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    public override Task<ResetRuntimeConfigResponse> ResetRuntimeConfig(
        ResetRuntimeConfigRequest request,
        ServerCallContext context)
    {
        try
        {
            return Task.FromResult(new ResetRuntimeConfigResponse
            {
                Snapshot = state.ResetRuntimeConfig(request.Keys),
            });
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedLogbookGrpcService(ManagedEngineState state)
    : LogbookService.LogbookServiceBase
{
    public override Task<LogQsoResponse> LogQso(LogQsoRequest request, ServerCallContext context)
    {
        try
        {
            return Task.FromResult(state.LogQso(request));
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    public override Task<UpdateQsoResponse> UpdateQso(UpdateQsoRequest request, ServerCallContext context)
    {
        try
        {
            return Task.FromResult(state.UpdateQso(request));
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    public override Task<DeleteQsoResponse> DeleteQso(DeleteQsoRequest request, ServerCallContext context)
    {
        var deleted = state.DeleteQso(request.LocalId);
        var response = new DeleteQsoResponse
        {
            Success = deleted,
            QrzDeleteSuccess = false,
        };

        if (!deleted)
        {
            response.Error = $"QSO '{request.LocalId}' was not found.";
        }

        if (request.DeleteFromQrz)
        {
            response.QrzDeleteError = "Managed engine does not delete remote QRZ records.";
        }

        return Task.FromResult(response);
    }

    public override Task<GetQsoResponse> GetQso(GetQsoRequest request, ServerCallContext context)
    {
        var qso = state.GetQso(request.LocalId);
        if (qso is null)
        {
            throw new RpcException(new Status(StatusCode.NotFound, $"QSO '{request.LocalId}' was not found."));
        }

        return Task.FromResult(new GetQsoResponse { Qso = qso });
    }

    public override async Task ListQsos(
        ListQsosRequest request,
        IServerStreamWriter<ListQsosResponse> responseStream,
        ServerCallContext context)
    {
        foreach (var qso in state.ListQsos(request))
        {
            await responseStream.WriteAsync(new ListQsosResponse { Qso = qso });
        }
    }

    public override async Task SyncWithQrz(
        SyncWithQrzRequest request,
        IServerStreamWriter<SyncWithQrzResponse> responseStream,
        ServerCallContext context)
    {
        await responseStream.WriteAsync(state.SyncWithQrz(request.FullSync));
    }

    public override Task<GetSyncStatusResponse> GetSyncStatus(
        GetSyncStatusRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.GetSyncStatus());
    }

    public override Task<ImportAdifResponse> ImportAdif(
        IAsyncStreamReader<ImportAdifRequest> requestStream,
        ServerCallContext context)
    {
        try
        {
            return ImportAdifCoreAsync(requestStream, context);
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    public override async Task ExportAdif(
        ExportAdifRequest request,
        IServerStreamWriter<ExportAdifResponse> responseStream,
        ServerCallContext context)
    {
        try
        {
            var payload = state.ExportAdif(request);
            for (var offset = 0; offset < payload.Length; offset += ManagedAdifCodec.ChunkSize)
            {
                var chunkLength = Math.Min(ManagedAdifCodec.ChunkSize, payload.Length - offset);
                await responseStream.WriteAsync(new ExportAdifResponse
                {
                    Chunk = new AdifChunk
                    {
                        Data = ByteString.CopyFrom(payload, offset, chunkLength)
                    }
                });
            }
        }
        catch (InvalidOperationException ex)
        {
            throw new RpcException(new Status(StatusCode.InvalidArgument, ex.Message));
        }
    }

    private async Task<ImportAdifResponse> ImportAdifCoreAsync(
        IAsyncStreamReader<ImportAdifRequest> requestStream,
        ServerCallContext context)
    {
        ArgumentNullException.ThrowIfNull(requestStream);
        ArgumentNullException.ThrowIfNull(context);

        using var buffer = new MemoryStream();
        var refresh = false;

        while (await requestStream.MoveNext(context.CancellationToken))
        {
            var request = requestStream.Current;
            if (request.Chunk is null)
            {
                throw new InvalidOperationException("chunk is required.");
            }

            request.Chunk.Data.WriteTo(buffer);
            refresh |= request.Refresh;
        }

        return state.ImportAdif(buffer.ToArray(), refresh);
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedLookupGrpcService(ManagedEngineState state, ILookupCoordinator coordinator)
    : LookupService.LookupServiceBase
{
    public override Task<LookupResponse> Lookup(LookupRequest request, ServerCallContext context)
    {
        return Task.FromResult(state.Lookup(request.Callsign, skipCache: request.SkipCache));
    }

    public override async Task StreamLookup(
        StreamLookupRequest request,
        IServerStreamWriter<StreamLookupResponse> responseStream,
        ServerCallContext context)
    {
        foreach (var response in state.StreamLookup(request.Callsign))
        {
            await responseStream.WriteAsync(response);
        }
    }

    public override Task<GetCachedCallsignResponse> GetCachedCallsign(
        GetCachedCallsignRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetCachedCallsignResponse
        {
            Result = state.Lookup(request.Callsign, cacheOnly: true).Result,
        });
    }

    public override Task<GetDxccEntityResponse> GetDxccEntity(
        GetDxccEntityRequest request,
        ServerCallContext context)
    {
        return request.QueryCase switch
        {
            GetDxccEntityRequest.QueryOneofCase.DxccCode
                => DxccEntityTable.TryGetByCode(request.DxccCode, out var entity)
                    ? Task.FromResult(new GetDxccEntityResponse { Entity = entity })
                    : throw new RpcException(new Status(StatusCode.NotFound, $"DXCC entity {request.DxccCode} not found.")),

            GetDxccEntityRequest.QueryOneofCase.Prefix
                => throw new RpcException(new Status(StatusCode.Unimplemented, "Prefix-based DXCC lookup is not yet supported.")),

            _ => throw new RpcException(new Status(StatusCode.InvalidArgument, "Either dxcc_code or prefix must be specified.")),
        };
    }

    public override async Task<BatchLookupResponse> BatchLookup(
        BatchLookupRequest request,
        ServerCallContext context)
    {
        var results = await BatchLookupOrchestrator.ExecuteAsync(
            coordinator,
            (IReadOnlyList<string>)request.Callsigns,
            ct: context.CancellationToken);

        var response = new BatchLookupResponse();
        response.Results.AddRange(results);
        return response;
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedRigControlGrpcService(ManagedEngineState state)
    : RigControlService.RigControlServiceBase
{
    public override Task<GetRigStatusResponse> GetRigStatus(
        GetRigStatusRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.CreateRigStatusResponse());
    }

    public override Task<GetRigSnapshotResponse> GetRigSnapshot(
        GetRigSnapshotRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetRigSnapshotResponse
        {
            Snapshot = state.BuildRigSnapshot(),
        });
    }

    public override Task<TestRigConnectionResponse> TestRigConnection(
        TestRigConnectionRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(state.TestRigConnection());
    }
}

[SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes", Justification = "Activated by ASP.NET Core gRPC.")]
internal sealed class ManagedSpaceWeatherGrpcService(ManagedEngineState state)
    : SpaceWeatherService.SpaceWeatherServiceBase
{
    public override Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeather(
        GetCurrentSpaceWeatherRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new GetCurrentSpaceWeatherResponse
        {
            Snapshot = state.BuildSpaceWeatherSnapshot(refreshed: false),
        });
    }

    public override Task<RefreshSpaceWeatherResponse> RefreshSpaceWeather(
        RefreshSpaceWeatherRequest request,
        ServerCallContext context)
    {
        return Task.FromResult(new RefreshSpaceWeatherResponse
        {
            Snapshot = state.BuildSpaceWeatherSnapshot(refreshed: true),
        });
    }
}
