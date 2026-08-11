[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = "High")]
param(
    [Parameter(Mandatory = $true)]
    [string]$BackupPath,
    [string]$DataDir = (Join-Path $PSScriptRoot "data")
)

$resolvedBackup = (Resolve-Path -LiteralPath $BackupPath -ErrorAction Stop).Path
$resolvedData = (Resolve-Path -LiteralPath $DataDir -ErrorAction SilentlyContinue).Path
if (-not $resolvedData) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    $resolvedData = (Resolve-Path -LiteralPath $DataDir).Path
}
$manifestPath = Join-Path $resolvedBackup "manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Backup manifest is missing: $manifestPath"
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json

Write-Warning "Stop the Camrelay service before restoring. The current data files will be overwritten."
if (-not $PSCmdlet.ShouldProcess($resolvedData, "Restore backup $resolvedBackup")) { return }

foreach ($entry in $manifest.files) {
    $source = Join-Path $resolvedBackup ($entry.path -replace "/", [IO.Path]::DirectorySeparatorChar)
    $destination = Join-Path $resolvedData ($entry.path -replace "/", [IO.Path]::DirectorySeparatorChar)
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Backup file is missing: $source"
    }
    $destinationDirectory = Split-Path -Parent $destination
    New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination -Force
    $actualHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
    if ($actualHash -ne $entry.sha256) {
        throw "Restored file hash does not match the manifest: $destination"
    }
}

Write-Output "Camrelay backup restored: $resolvedBackup"
Write-Output "Restore CAMRELAY_SECRET_KEY from the deployment secret store before starting the service."
