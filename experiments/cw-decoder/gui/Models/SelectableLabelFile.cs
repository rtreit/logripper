using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.Json;

namespace CwDecoderGui.Models;

public sealed class SelectableLabelFile : INotifyPropertyChanged
{
    private bool _isSelected;

    public SelectableLabelFile(string path, string? displayName = null)
    {
        Path = path;
        DisplayName = displayName ?? System.IO.Path.GetFileName(path);
        TruthPreview = LoadTruthPreview(path, maxChars: 80);
    }

    public string Path { get; }
    public string DisplayName { get; }
    public string TruthPreview { get; }

    public bool IsSelected
    {
        get => _isSelected;
        set
        {
            if (_isSelected == value)
            {
                return;
            }

            _isSelected = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsSelected)));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private static string LoadTruthPreview(string labelsPath, int maxChars)
    {
        try
        {
            var combined = new System.Text.StringBuilder();
            int labelCount = 0;
            foreach (var line in File.ReadLines(labelsPath))
            {
                if (string.IsNullOrWhiteSpace(line)) continue;
                try
                {
                    using var doc = JsonDocument.Parse(line);
                    if (doc.RootElement.TryGetProperty("correct_copy", out var cc) && cc.ValueKind == JsonValueKind.String)
                    {
                        var truth = cc.GetString();
                        if (!string.IsNullOrEmpty(truth))
                        {
                            if (labelCount > 0) combined.Append(" | ");
                            combined.Append(truth);
                            labelCount++;
                            if (combined.Length >= maxChars) break;
                        }
                    }
                }
                catch { /* skip malformed line */ }
            }
            var text = combined.ToString().Replace('\n', ' ').Replace('\r', ' ').Trim();
            if (string.IsNullOrEmpty(text)) return "(no truth)";
            return text.Length > maxChars ? text.Substring(0, maxChars) + "…" : text;
        }
        catch
        {
            return "(unreadable)";
        }
    }
}
