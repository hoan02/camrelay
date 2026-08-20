[CmdletBinding()]
param(
    [string]$DataDir = (Join-Path $PSScriptRoot "data")
)

$resolvedData = (Resolve-Path -LiteralPath $DataDir -ErrorAction Stop).Path
$errors = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

function Read-LegacyJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [object]$Fallback
    )

    $path = Join-Path $resolvedData $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        return $Fallback
    }
    try {
        return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    } catch {
        $errors.Add("Could not parse ${Name}: $($_.Exception.Message)")
        return $Fallback
    }
}

function Test-DuplicateValues {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Items,
        [Parameter(Mandatory = $true)]
        [string]$Property,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    $duplicates = $Items |
        Where-Object { $null -ne $_.$Property -and "$($_.$Property)".Trim() -ne "" } |
        Group-Object -Property $Property |
        Where-Object Count -gt 1
    foreach ($duplicate in $duplicates) {
        $errors.Add("Duplicate ${Label}: $($duplicate.Name)")
    }
}

$config = Read-LegacyJson -Name "config.json" -Fallback ([pscustomobject]@{})
$brands = @(Read-LegacyJson -Name "brands.json" -Fallback @())
$cameras = @(Read-LegacyJson -Name "cameras.json" -Fallback @())
$tokens = @(Read-LegacyJson -Name "tokens.json" -Fallback @())

Test-DuplicateValues -Items $brands -Property "id" -Label "provider id"
Test-DuplicateValues -Items $brands -Property "name" -Label "provider name"
Test-DuplicateValues -Items $cameras -Property "id" -Label "camera id"
Test-DuplicateValues -Items $cameras -Property "local_port" -Label "local port"
Test-DuplicateValues -Items $tokens -Property "id" -Label "token id"

$providerNames = @($brands | ForEach-Object { "$($_.name)" })
foreach ($camera in $cameras) {
    if ([string]::IsNullOrWhiteSpace("$($camera.id)") -or [string]::IsNullOrWhiteSpace("$($camera.name)")) {
        $errors.Add("Camera is missing id or name")
    }
    if (-not $providerNames -contains "$($camera.brand)") {
        $errors.Add("Camera '$($camera.id)' references missing provider '$($camera.brand)'")
    }
    if ([int]$camera.local_port -lt 1 -or [int]$camera.local_port -gt 65535) {
        $errors.Add("Camera '$($camera.id)' has an invalid local port")
    }
}

if (($brands.Count -gt 0 -or $cameras.Count -gt 0 -or $tokens.Count -gt 0) -and
    [string]::IsNullOrWhiteSpace($env:CAMRELAY_SECRET_KEY)) {
    $warnings.Add("CAMRELAY_SECRET_KEY is not set; SQLite import of provider, camera, or token secrets will fail")
}

$files = @("config.json", "brands.json", "cameras.json", "tokens.json") |
    ForEach-Object {
        [ordered]@{
            name = $_
            present = Test-Path -LiteralPath (Join-Path $resolvedData $_) -PathType Leaf
        }
    }

$result = [ordered]@{
    status = if ($errors.Count -eq 0) { "ready" } else { "blocked" }
    data_dir = $resolvedData
    database_enabled = [bool]$config.database_enabled
    counts = [ordered]@{
        providers = $brands.Count
        cameras = $cameras.Count
        tokens = $tokens.Count
    }
    files = $files
    errors = @($errors)
    warnings = @($warnings)
}

$result | ConvertTo-Json -Depth 6
if ($errors.Count -gt 0) {
    exit 2
}
