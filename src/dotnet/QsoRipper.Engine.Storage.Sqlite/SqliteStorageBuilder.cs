using Microsoft.Data.Sqlite;

namespace QsoRipper.Engine.Storage.Sqlite;

/// <summary>
/// Builder for <see cref="SqliteStorage"/>. Mirrors the Rust SqliteStorageBuilder pattern.
/// </summary>
public sealed class SqliteStorageBuilder
{
    private string? _path = "qsoripper.db";
    private TimeSpan _busyTimeout = TimeSpan.FromSeconds(5);

    /// <summary>Sets the file path for the SQLite database.</summary>
    public SqliteStorageBuilder Path(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        _path = path;
        return this;
    }

    /// <summary>Uses an in-memory SQLite database (no file persistence).</summary>
    public SqliteStorageBuilder InMemory()
    {
        _path = null;
        return this;
    }

    /// <summary>Sets the busy timeout for the SQLite connection.</summary>
    public SqliteStorageBuilder BusyTimeout(TimeSpan timeout)
    {
        _busyTimeout = timeout;
        return this;
    }

    /// <summary>Builds and returns the configured <see cref="SqliteStorage"/>.</summary>
    public SqliteStorage Build()
    {
        string connectionString;
        if (_path is null)
        {
            // In-memory database: use a shared cache so the connection stays alive.
            connectionString = "Data Source=InMemoryStorage;Mode=Memory;Cache=Shared";
        }
        else
        {
            var fullPath = System.IO.Path.GetFullPath(_path);
            var directory = System.IO.Path.GetDirectoryName(fullPath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            connectionString = new SqliteConnectionStringBuilder
            {
                DataSource = fullPath,
                Mode = SqliteOpenMode.ReadWriteCreate,
            }.ConnectionString;
        }

        var connection = new SqliteConnection(connectionString);
        try
        {
            connection.Open();
            var storage = new SqliteStorage(connection);
            storage.ConfigurePragmas(_busyTimeout);
            storage.RunMigrations();
            return storage;
        }
        catch
        {
            connection.Dispose();
            throw;
        }
    }
}
