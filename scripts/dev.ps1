# Start a full local Aether session from the repo root.
# Usage: .\scripts\dev.ps1
#        .\scripts\dev.ps1 -Watch

param(
    [string]$Package = "examples/minimal_explorer/package.json",
    [string]$Plane = "",
    [switch]$Watch,
    [switch]$NoSpawnPlane,
    [switch]$NoOpenStudio
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$devArgs = @("run", "-p", "aether_cli", "--", "dev", $Package)
if ($Plane) { $devArgs += @("--plane", $Plane) }
if ($Watch) { $devArgs += "--watch" }
if ($NoSpawnPlane) { $devArgs += "--no-spawn-plane" }
if ($NoOpenStudio) { $devArgs += "--no-open-studio" }

cargo @devArgs
