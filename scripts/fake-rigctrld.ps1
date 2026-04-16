#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Fake rigctld simulator for testing QsoRipper rig control without hardware.

.DESCRIPTION
    Listens on TCP port 4532 (the rigctld default) and responds to the two
    commands the QsoRipper engine polls:

        f  -> frequency in Hz
        m  -> mode string, then passband width

    Frequency and mode cycle through a configurable preset list every
    CycleSecs seconds so you can watch the TUI update in real time.

.PARAMETER Port
    TCP port to listen on. Default: 4532.

.PARAMETER BindAddress
    IP address to bind. Default: 127.0.0.1.

.PARAMETER CycleSecs
    Seconds between automatic frequency/mode changes. Default: 10.

.PARAMETER Presets
    Array of hashtables with keys: FrequencyHz, Mode, Passband.
    Default covers 20m FT8, 40m CW, 15m SSB, 2m FM.

.EXAMPLE
    # Run with defaults -- simulates 20m FT8 rig
    .\scripts\fake-rigctrld.ps1

.EXAMPLE
    # Custom single preset, no cycling
    .\scripts\fake-rigctrld.ps1 -CycleSecs 9999 -Presets @(@{ FrequencyHz=7074000; Mode='USB'; Passband=2400 })
#>
param(
    [int]    $Port         = 4532,
    [string] $BindAddress  = '127.0.0.1',
    [int]    $CycleSecs    = 10,
    [array]  $Presets   = @(
        @{ FrequencyHz = 14074000; Mode = 'USB';  Passband = 2400 },
        @{ FrequencyHz = 7030000;  Mode = 'CW';   Passband = 500  },
        @{ FrequencyHz = 21200000; Mode = 'USB';  Passband = 2400 },
        @{ FrequencyHz = 144200000; Mode = 'USB'; Passband = 2700 },
        @{ FrequencyHz = 3573000;  Mode = 'USB';  Passband = 3000 }
    )
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$listener = [System.Net.Sockets.TcpListener]::new(
    [System.Net.IPAddress]::Parse($BindAddress), $Port
)
$listener.Start()

Write-Host "fake-rigctrld listening on ${BindAddress}:${Port}"
Write-Host "Cycling through $($Presets.Count) presets every ${CycleSecs}s. Ctrl+C to stop."
Write-Host ""

$presetIndex = 0
$lastCycle   = [System.Diagnostics.Stopwatch]::StartNew()

function Get-CurrentPreset {
    $script:Presets[$script:presetIndex % $script:Presets.Count]
}

function Show-Preset([hashtable] $preset) {
    $freq = $preset.FrequencyHz
    $mode = $preset.Mode
    $band = if     ($freq -ge 1800000   -and $freq -le 2000000)   { '160m' }
            elseif ($freq -ge 3500000   -and $freq -le 4000000)   { '80m'  }
            elseif ($freq -ge 7000000   -and $freq -le 7300000)   { '40m'  }
            elseif ($freq -ge 10100000  -and $freq -le 10150000)  { '30m'  }
            elseif ($freq -ge 14000000  -and $freq -le 14350000)  { '20m'  }
            elseif ($freq -ge 18068000  -and $freq -le 18168000)  { '17m'  }
            elseif ($freq -ge 21000000  -and $freq -le 21450000)  { '15m'  }
            elseif ($freq -ge 24890000  -and $freq -le 24990000)  { '12m'  }
            elseif ($freq -ge 28000000  -and $freq -le 29700000)  { '10m'  }
            elseif ($freq -ge 50000000  -and $freq -le 54000000)  { '6m'   }
            elseif ($freq -ge 144000000 -and $freq -le 148000000) { '2m'   }
            else   { '?m' }
    Write-Host "  Active: $freq Hz  ($band $mode)"
}

Show-Preset (Get-CurrentPreset)

try {
    while ($true) {
        if ($lastCycle.Elapsed.TotalSeconds -ge $CycleSecs -and $Presets.Count -gt 1) {
            $presetIndex++
            $lastCycle.Restart()
            $preset = Get-CurrentPreset
            Write-Host "[cycle] switched to preset $($presetIndex % $Presets.Count)"
            Show-Preset $preset
        }

        if (-not $listener.Pending()) {
            Start-Sleep -Milliseconds 100
            continue
        }

        $client = $listener.AcceptTcpClient()
        $stream = $client.GetStream()
        $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::ASCII)
        $writer = [System.IO.StreamWriter]::new($stream, [System.Text.Encoding]::ASCII)
        $writer.AutoFlush = $true
        $writer.NewLine   = "`n"

        try {
            $preset = Get-CurrentPreset
            while ($client.Connected) {
                $line = $reader.ReadLine()
                if ($null -eq $line) { break }
                $cmd = $line.Trim()
                switch ($cmd) {
                    'f' {
                        $writer.WriteLine($preset.FrequencyHz)
                    }
                    'm' {
                        $writer.WriteLine($preset.Mode)
                        $writer.WriteLine($preset.Passband)
                    }
                    default {
                        $writer.WriteLine('RPRT -11')
                    }
                }
            }
        }
        catch {
            # client disconnected -- normal
        }
        finally {
            $reader.Dispose()
            $writer.Dispose()
            $client.Dispose()
        }
    }
}
finally {
    $listener.Stop()
    Write-Host "fake-rigctrld stopped."
}
