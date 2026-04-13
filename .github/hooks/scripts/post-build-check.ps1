# Post-tool hook scaffold.
# Intentionally minimal until project build/test commands are finalized.

param(
    [string]$Command = $env:COPILOT_TOOL_INPUT
)

if (-not $Command) {
    exit 0
}

if ($env:QSORIPPER_HOOK_VERBOSE -eq "1") {
    Write-Host "post-build-check observed command: $Command"
}

exit 0

