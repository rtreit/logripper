namespace QsoRipper.Engine.Storage;

/// <summary>
/// Represents an error originating from a storage backend.
/// </summary>
public sealed class StorageException : Exception
{
    /// <summary>Gets the kind of storage error.</summary>
    public StorageErrorKind Kind { get; }

    public StorageException()
        : this(StorageErrorKind.Backend, "An unspecified storage error occurred.")
    {
    }

    public StorageException(string message)
        : this(StorageErrorKind.Backend, message)
    {
    }

    public StorageException(string message, Exception innerException)
        : this(StorageErrorKind.Backend, message, innerException)
    {
    }

    public StorageException(StorageErrorKind kind, string message)
        : base(message)
    {
        Kind = kind;
    }

    public StorageException(StorageErrorKind kind, string message, Exception inner)
        : base(message, inner)
    {
        Kind = kind;
    }

    /// <summary>Creates a <see cref="StorageErrorKind.Duplicate"/> exception.</summary>
    public static StorageException Duplicate(string entity, string key)
        => new(StorageErrorKind.Duplicate, $"Duplicate {entity}: {key}");

    /// <summary>Creates a <see cref="StorageErrorKind.Backend"/> exception.</summary>
    public static StorageException Backend(string message)
        => new(StorageErrorKind.Backend, message);
}

/// <summary>Classifies storage errors.</summary>
public enum StorageErrorKind
{
    /// <summary>An insert attempted to create a record with a duplicate key.</summary>
    Duplicate,

    /// <summary>Stored data could not be deserialized or is structurally invalid.</summary>
    CorruptData,

    /// <summary>The operation is not supported by this backend.</summary>
    Unsupported,

    /// <summary>A generic backend-level failure (I/O, connection, etc.).</summary>
    Backend,
}
