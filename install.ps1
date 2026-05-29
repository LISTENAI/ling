$ErrorActionPreference = "Stop"

$Repo = if ($env:LING_REPO) { $env:LING_REPO } else { "LISTENAI/ling" }
$Bin = "ling"
$InstallDir = if ($env:LING_INSTALL_DIR) { $env:LING_INSTALL_DIR } else { Join-Path $HOME "bin" }
$Token = if ($env:GH_TOKEN) { $env:GH_TOKEN } else { $env:GITHUB_TOKEN }
$ApiUrl = "https://api.github.com/repos/$Repo"
$Headers = @{
    "X-GitHub-Api-Version" = "2022-11-28"
}
if ($Token) {
    $Headers["Authorization"] = "Bearer $Token"
}

function Invoke-GitHubJson {
    param([string] $Uri)
    return Invoke-RestMethod -Uri $Uri -Headers $Headers
}

function Resolve-Target {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
    switch ($arch) {
        "x64" { return "x86_64-pc-windows-msvc" }
        "arm64" { return "aarch64-pc-windows-msvc" }
        default { throw "Unsupported CPU architecture: $arch" }
    }
}

function Get-Release {
    param([string] $Version)
    if ($Version) {
        return Invoke-GitHubJson "$ApiUrl/releases/tags/$Version"
    }
    return Invoke-GitHubJson "$ApiUrl/releases/latest"
}

function Save-ReleaseAsset {
    param(
        [object] $Release,
        [string] $Name,
        [string] $OutFile
    )

    if ($Token) {
        $asset = $Release.assets | Where-Object { $_.name -eq $Name } | Select-Object -First 1
        if (-not $asset) {
            throw "Release asset not found: $Name"
        }
        $downloadHeaders = $Headers.Clone()
        $downloadHeaders["Accept"] = "application/octet-stream"
        Invoke-WebRequest -Uri $asset.url -Headers $downloadHeaders -OutFile $OutFile
    } else {
        Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$($Release.tag_name)/$Name" -OutFile $OutFile
    }
}

function Test-CommandOnPath {
    param([string] $Directory)
    $full = [System.IO.Path]::GetFullPath($Directory).TrimEnd('\')
    foreach ($part in ($env:Path -split ';')) {
        if (-not $part) { continue }
        try {
            if ([System.IO.Path]::GetFullPath($part).TrimEnd('\') -ieq $full) {
                return $true
            }
        } catch {
            continue
        }
    }
    return $false
}

$RequestedVersion = if ($env:LING_VERSION) { $env:LING_VERSION } else { $null }
$Release = Get-Release $RequestedVersion
$Version = $Release.tag_name
if (-not $Version) {
    throw "Failed to resolve release tag for $Repo"
}

$Target = Resolve-Target
$Asset = "$Bin-$Version-$Target.zip"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ling-install-" + [System.Guid]::NewGuid().ToString("N"))

try {
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $Archive = Join-Path $TempDir $Asset
    $Checksums = Join-Path $TempDir "SHA256SUMS"

    Write-Host "Installing $Bin $Version for $Target"
    Write-Host "Downloading $Asset"
    Save-ReleaseAsset $Release $Asset $Archive

    try {
        Save-ReleaseAsset $Release "SHA256SUMS" $Checksums
        $line = Get-Content $Checksums | Where-Object { $_ -match "\s$([regex]::Escape($Asset))$" } | Select-Object -First 1
        if ($line) {
            $expected = (($line -split '\s+')[0]).ToLowerInvariant()
            $actual = (Get-FileHash -Algorithm SHA256 -Path $Archive).Hash.ToLowerInvariant()
            if ($expected -ne $actual) {
                throw "Checksum mismatch for $Asset"
            }
            Write-Host "Checksum verified"
        }
    } catch {
        Write-Warning "Skipping checksum verification: $($_.Exception.Message)"
    }

    Expand-Archive -Force -Path $Archive -DestinationPath $TempDir
    $Source = Join-Path $TempDir "$Bin-$Version-$Target\$Bin.exe"
    if (-not (Test-Path $Source)) {
        throw "Binary not found in archive: $Source"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $Destination = Join-Path $InstallDir "$Bin.exe"
    Copy-Item -Force $Source $Destination

    Write-Host "Installed to $Destination"
    & $Destination --help *> $null

    if (-not (Test-CommandOnPath $InstallDir)) {
        if ($env:LING_NO_PATH -eq "1") {
            Write-Host "Add $InstallDir to PATH if 'ling' is not found."
        } else {
            $fullInstallDir = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
            $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
            if ([string]::IsNullOrWhiteSpace($userPath)) {
                $newUserPath = $fullInstallDir
            } elseif (($userPath -split ';') -notcontains $fullInstallDir) {
                $newUserPath = $userPath.TrimEnd(';') + ";" + $fullInstallDir
            } else {
                $newUserPath = $userPath
            }
            [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
            $env:Path = $env:Path.TrimEnd(';') + ";" + $fullInstallDir
            Write-Host "Added $fullInstallDir to your user PATH. Restart the terminal if 'ling' is not found."
        }
    }

    Write-Host "Done. Try: ling --help"
} finally {
    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force $TempDir
    }
}
