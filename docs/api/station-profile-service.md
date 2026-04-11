# StationProfileService Reference

`StationProfileService` owns persisted station-profile lifecycle after the initial setup/bootstrap phase.

Proto definition: [`proto/services/station_profile_service.proto`](../../proto/services/station_profile_service.proto)

## Purpose

Use this service to:

- list and inspect persisted station profiles
- create or update additional profiles after first-run setup
- switch the persisted active profile used for new QSO saves
- inspect the effective active station context
- apply or clear a bounded in-memory session override without mutating persisted profiles

Historical QSOs remain stable because saved records already carry their own `station_snapshot`.

## Current behavior

| RPC | Status | Notes |
|---|---|---|
| `ListStationProfiles` | ✅ Implemented | Returns every persisted profile plus the active id |
| `GetStationProfile` | ✅ Implemented | Reads a single profile by id |
| `SaveStationProfile` | ✅ Implemented | Creates or updates a profile and optionally makes it active |
| `DeleteStationProfile` | ✅ Implemented | Deletes an inactive profile |
| `SetActiveStationProfile` | ✅ Implemented | Switches the persisted active profile |
| `GetActiveStationContext` | ✅ Implemented | Returns persisted active, effective active, and session override state |
| `SetSessionStationProfileOverride` | ✅ Implemented | Applies a process-session override for new QSO saves |
| `ClearSessionStationProfileOverride` | ✅ Implemented | Clears the process-session override |

## Contract shape

This service follows the same protobuf 1-1-1 rule as the rest of the engine surface:

- every RPC has a unique request/response envelope
- shared payloads such as `StationProfileRecord` and `ActiveStationContext` live in their own support files under `proto/services/`
- response envelopes wrap those payloads instead of being reused as nested models elsewhere

### Reusable payloads

| Payload | Purpose |
|---|---|
| `StationProfileRecord` | A persisted profile plus its `profile_id` and `is_active` status |
| `ActiveStationContext` | Persisted active profile, effective active profile, optional session override, and warnings |

### Response envelopes

| RPC | Response envelope | Key payload fields |
|---|---|---|
| `ListStationProfiles` | `ListStationProfilesResponse` | `profiles`, `active_profile_id` |
| `GetStationProfile` | `GetStationProfileResponse` | `profile` |
| `SaveStationProfile` | `SaveStationProfileResponse` | `profile`, `active_profile_id` |
| `DeleteStationProfile` | `DeleteStationProfileResponse` | `active_profile_id` |
| `SetActiveStationProfile` | `SetActiveStationProfileResponse` | `profile` |
| `GetActiveStationContext` | `GetActiveStationContextResponse` | `context` |
| `SetSessionStationProfileOverride` | `SetSessionStationProfileOverrideResponse` | `context` |
| `ClearSessionStationProfileOverride` | `ClearSessionStationProfileOverrideResponse` | `context` |

## Notes

- `SaveStationProfile`, `DeleteStationProfile`, and `SetActiveStationProfile` require persisted setup to already exist. Run `SetupService.SaveSetup` first.
- When `profile_id` is omitted on save, the server generates one from the profile metadata.
- The active profile cannot be deleted; activate another profile first.
- Session overrides are process-local and in-memory only. Restarting the engine clears them.
