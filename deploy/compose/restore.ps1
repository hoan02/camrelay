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

function Resolve-ManifestChildPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Root,
        [Parameter(Mandatory = $true)]
        [string]$RelativePath,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    if ([string]::IsNullOrWhiteSpace($RelativePath)) {
        throw "$Label path is empty"
    }
    $normalized = $RelativePath.Replace('/', [IO.Path]::DirectorySeparatorChar)
    if ([IO.Path]::IsPathRooted($normalized) -or $normalized -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "$Label path is outside the restore root: $RelativePath"
    }
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $candidate = [IO.Path]::GetFullPath((Join-Path $Root $normalized))
    if (-not $candidate.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label path is outside the restore root: $RelativePath"
    }
    return $candidate
}

$restoreEntries = @()
foreach ($entry in @($manifest.files)) {
    if ($entry.path -notmatch '^[^:*?"<>|]+$' -or $entry.path -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "Backup manifest contains an unsafe relative path: $($entry.path)"
    }
    if ($entry.sha256 -notmatch '^[0-9a-fA-F]{64}$') {
        throw "Backup manifest contains an invalid SHA-256 value for: $($entry.path)"
    }
    $source = Resolve-ManifestChildPath -Root $resolvedBackup -RelativePath $entry.path -Label "Backup"
    $destination = Resolve-ManifestChildPath -Root $resolvedData -RelativePath $entry.path -Label "Data"
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Backup file is missing: $source"
    }
    $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    if ($sourceHash -ne $entry.sha256) {
        throw "Backup file hash does not match the manifest: $source"
    }
    $restoreEntries += [pscustomobject]@{
        entry = $entry
        source = $source
        destination = $destination
    }
}

Write-Warning "Stop the Camrelay service before restoring. The current data files will be overwritten."
if (-not $PSCmdlet.ShouldProcess($resolvedData, "Restore backup $resolvedBackup")) { return }

foreach ($restoreEntry in $restoreEntries) {
    $entry = $restoreEntry.entry
    $source = $restoreEntry.source
    $destination = $restoreEntry.destination
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
