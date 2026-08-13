param (
    [string]$Tag = ""
)

$ErrorActionPreference = "Stop"

$ManifestUrl = "https://storage.galfus.com/cli/manifest.json" # Correct storage URL
$GalfusDir = Join-Path $env:LOCALAPPDATA "galfus"
$BinDir = Join-Path $GalfusDir "bin"

Write-Host "=> Fetching manifest from $ManifestUrl..."
try {
    $ManifestJson = Invoke-RestMethod -Uri $ManifestUrl -UseBasicParsing
} catch {
    Write-Error "Failed to download manifest. Please check your internet connection."
    exit 1
}

if ([string]::IsNullOrWhiteSpace($Tag)) {
    if (-not $ManifestJson.latest_tag) {
        Write-Error "Error: Could not determine latest_tag from manifest."
        exit 1
    }
    $Tag = $ManifestJson.latest_tag
    Write-Host "=> Selected tag: $Tag (latest)"
} else {
    Write-Host "=> Selected tag: $Tag"
}

$Version = $ManifestJson.tags.$Tag

if ([string]::IsNullOrWhiteSpace($Version)) {
    Write-Error "Error: Tag '$Tag' not found in the manifest."
    exit 1
}

Write-Host "=> Version: $Version"

# Architecture
$Arch = "x64"
if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    $Arch = "arm64"
}

$DownloadUrl = "https://storage.galfus.com/cli/$Tag/$Version/windows/$Arch/galfus-cli-windows-$Arch.exe"
Write-Host "=> Downloading from $DownloadUrl..."

if (-not (Test-Path -Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
}

$DestPath = Join-Path $BinDir "galfus.exe"
$TmpPath = Join-Path $env:TEMP "galfus-cli.exe"

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TmpPath -UseBasicParsing
} catch {
    Write-Error "Failed to download binary from CDN."
    exit 1
}

# Verify it's actually an executable (starts with MZ)
$Bytes = Get-Content -Path $TmpPath -Encoding Byte -TotalCount 2 -ErrorAction SilentlyContinue
if ($Bytes -ne $null -and ($Bytes[0] -ne 77 -or $Bytes[1] -ne 90)) {
    Remove-Item $TmpPath -Force
    Write-Error "Downloaded file is not a valid executable."
    exit 1
}

Move-Item -Path $TmpPath -Destination $DestPath -Force

Write-Host "=> Installed galfus to $DestPath"

# Add to PATH
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notmatch [regex]::Escape($BinDir)) {
    $NewPath = $UserPath
    if (-not $NewPath.EndsWith(";")) {
        $NewPath += ";"
    }
    $NewPath += $BinDir
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    Write-Host "=> Added $BinDir to your PATH."
    Write-Host "=> Please restart your terminal to use Galfus."
} else {
    Write-Host "=> $BinDir is already in your PATH."
}

Write-Host "=> Galfus installation complete!"
