[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = "Low")]
param(
    [string]$DataDir = (Join-Path $PSScriptRoot "data"),
    [string]$BackupRoot = (Join-Path $PSScriptRoot "backups")
)

$resolvedData = (Resolve-Path -LiteralPath $DataDir -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $resolvedData -PathType Container)) {
    throw "Data directory does not exist: $resolvedData"
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$resolvedBackupRoot = if (Test-Path -LiteralPath $BackupRoot -PathType Container) {
    (Resolve-Path -LiteralPath $BackupRoot).Path
} else {
    (New-Item -ItemType Directory -Path $BackupRoot -Force).FullName
}
$backupPath = Join-Path $resolvedBackupRoot $stamp
New-Item -ItemType Directory -Path $backupPath -Force | Out-Null

$fileNames = @(
    "camrelay.sqlite",
    "config.json",
    "brands.json",
    "cameras.json",
    "tokens.json",
    "recordings.json"
)
$manifestFiles = @()

foreach ($fileName in $fileNames) {
    $source = Join-Path $resolvedData $fileName
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { continue }
    $destination = Join-Path $backupPath $fileName
    if ($PSCmdlet.ShouldProcess($source, "Back up to $destination")) {
        Copy-Item -LiteralPath $source -Destination $destination -Force
    }
    $manifestFiles += [ordered]@{
        path = $fileName
        sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    }
}

$recordingsPath = Join-Path $resolvedData "recordings"
if (Test-Path -LiteralPath $recordingsPath -PathType Container) {
    $recordingsDestination = Join-Path $backupPath "recordings"
    if ($PSCmdlet.ShouldProcess($recordingsPath, "Back up recording segments to $recordingsDestination")) {
        Copy-Item -LiteralPath $recordingsPath -Destination $recordingsDestination -Recurse -Force
    }
    Get-ChildItem -LiteralPath $recordingsPath -File -Recurse | ForEach-Object {
        $relative = $_.FullName.Substring($resolvedData.Length).TrimStart([char]92, [char]47)
        $manifestFiles += [ordered]@{
            path = $relative.Replace([IO.Path]::DirectorySeparatorChar, "/")
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
        }
    }
}

$manifest = [ordered]@{
    created_at = (Get-Date).ToUniversalTime().ToString("o")
    source = $resolvedData
    note = "Stop camrelay before restoring. Keep CAMRELAY_SECRET_KEY separately; it is not copied by this script."
    files = $manifestFiles
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $backupPath "manifest.json") -Encoding UTF8
Write-Output "Camrelay backup created: $backupPath"
