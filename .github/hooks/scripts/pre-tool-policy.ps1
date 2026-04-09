# Pre-tool policy hook
# Blocks obvious secret leaks and warns when Python appears in core runtime paths.

param(
    [string]$File = $env:COPILOT_FILE
)

function Test-ForSecrets {
    param([string]$FilePath)

    if (-not (Test-Path $FilePath)) {
        return
    }

    $secretPattern = '(?i)(api[_-]?key|secret|password|token|connection[_-]?string)\s*[:=]\s*[''"][^''"]+[''"]'
    if (Select-String -Path $FilePath -Pattern $secretPattern -Quiet) {
        Write-Error "POLICY VIOLATION: Potential hardcoded secret detected in $FilePath"
        Write-Error "Use environment variables or a secure secrets provider."
        exit 1
    }
}

function Test-ForRuntimeLanguageDrift {
    param([string]$FilePath)

    if ($FilePath -match '(?i)[\\/](src|core|engine)[\\/].*\.py$') {
        Write-Warning "Performance guidance: prefer Rust or C# for core runtime paths."
    }
}

if ($File) {
    Test-ForSecrets -FilePath $File
    Test-ForRuntimeLanguageDrift -FilePath $File
}

