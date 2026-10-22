$ErrorActionPreference = 'Stop'
$adapters = @(Get-NetAdapter -IncludeHidden | Where-Object { $_.InterfaceDescription -like 'TAP-Windows Adapter V9*' })
if ($adapters.Count -gt 0) { Write-Host 'Existing TAP adapter retained'; exit 0 }
& (Join-Path $PSScriptRoot 'driver/tapinstall.exe') install (Join-Path $PSScriptRoot 'driver/OemVista.inf') tap0901
if ($LASTEXITCODE -notin @(0,1)) { throw "TAP installation failed (exit $LASTEXITCODE). Run install-tap.ps1 as Administrator to retry." }
if ($LASTEXITCODE -eq 1) { Write-Host 'Restart Windows to complete TAP driver installation.' }
