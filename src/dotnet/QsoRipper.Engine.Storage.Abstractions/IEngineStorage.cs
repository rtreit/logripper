namespace QsoRipper.Engine.Storage;

/// <summary>
/// Root abstraction for engine storage backends.
/// Each backend provides logbook and lookup snapshot stores.
/// </summary>
public interface IEngineStorage
{
    /// <summary>Gets the logbook store for QSO records and sync metadata.</summary>
    ILogbookStore Logbook { get; }

    /// <summary>Gets the lookup snapshot store for cached callsign lookups.</summary>
    ILookupSnapshotStore LookupSnapshots { get; }

    /// <summary>Gets the backend identifier (e.g. "memory", "sqlite").</summary>
    string BackendName { get; }
}
