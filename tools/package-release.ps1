param(
    [string]$Version,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$repositoryDirectory = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $repositoryDirectory
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read Cargo metadata.' }
    $package = $metadata.packages | Where-Object { $_.name -eq 'magic-language-switcher' } | Select-Object -First 1
    if (-not $Version) { $Version = $package.version }
    if ($Version -ne $package.version) { throw "Release version $Version does not match Cargo version $($package.version)." }
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Use a stable semantic version such as 0.1.0.' }
    if (-not $SkipBuild) {
        cargo build --release --locked --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    }
    $outputDirectory = Join-Path $repositoryDirectory "artifacts/v$Version"
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
    $filename = "MagicLanguageSwitcher-v$Version-windows-x64.exe"
    $executable = Join-Path $outputDirectory $filename
    Copy-Item -LiteralPath 'target/x86_64-pc-windows-msvc/release/magic-language-switcher.exe' -Destination $executable -Force
    python tools/verify-portable.py $executable
    if ($LASTEXITCODE -ne 0) { throw 'Portable EXE validation failed.' }
    $checksum = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksumPath = Join-Path $outputDirectory 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllText($checksumPath, "$checksum  $filename`n", [System.Text.UTF8Encoding]::new($false))
    Write-Output "Release assets: $outputDirectory"
} finally {
    Pop-Location
}
