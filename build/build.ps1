<#
.SYNOPSIS
    Checks, tests and publishes Show Player as a win-x64 installer, with a
    portable folder and zip as supplemental artifacts.

.DESCRIPTION
    The normal release command. It fails if a check, a test or a packaging
    prerequisite (libmpv, Inno Setup) is missing. Outputs go to dist\.

.EXAMPLE
    pwsh build/build.ps1                    # checks, tests, publish and installer
    pwsh build/build.ps1 -SkipChecks        # publish and installer only
    pwsh build/build.ps1 -Books D:\mka\a.mka,D:\mka\b.mka,D:\mka\c.mka
                                            # also runs the stage 2 and short stage 3 tests on real books
#>
param(
    [switch]$SkipChecks,
    [string[]]$Books,
    [string]$InnoCompiler
)

$ErrorActionPreference = 'Stop'
# pwsh -File passes a comma list as one string.
$Books = @($Books | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'target'
$dist = Join-Path $root 'dist'
$appDir = Join-Path $dist 'ShowPlayer'
$version = ([xml](Get-Content (Join-Path $root 'Directory.Build.props'))).Project.PropertyGroup.Version | Select-Object -First 1
$cargoVersion = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($cargoVersion -ne $version) { throw "Cargo.toml has version $cargoVersion but Directory.Build.props has $version." }

if (-not (Test-Path (Join-Path $root 'third_party/libmpv/libmpv-2.dll'))) {
    throw 'libmpv is missing. See third_party/README.md.'
}
if (-not $InnoCompiler) {
    $InnoCompiler = @(
        (Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 6/ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6/ISCC.exe')
    ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $InnoCompiler -or -not (Test-Path -LiteralPath $InnoCompiler)) {
    throw 'Install Inno Setup 6 or pass -InnoCompiler <ISCC.exe>. Every Jupiter GUI release needs its installer.'
}

function Invoke-Step([string]$Name, [scriptblock]$Command) {
    Write-Host "== $Name" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Name failed (exit $LASTEXITCODE)." }
}

Push-Location $root
try {
    $env:AB_TEST_ENGINE = Join-Path $root 'third_party/libmpv/libmpv-2.dll'
    if (-not $SkipChecks) {
        Invoke-Step 'cargo fmt' { cargo fmt --all --check }
        Invoke-Step 'cargo clippy' { cargo clippy --workspace --all-targets --locked --target-dir $target -- -D warnings }
        Invoke-Step 'cargo test' { cargo test --workspace --locked --target-dir $target }
    }
    Invoke-Step 'build harness' { cargo build -p show-player-harness --release --locked --target-dir $target }
    Invoke-Step 'build binding check' { dotnet build tests/ShowPlayer.BindingCheck -c Release --nologo -v q }
    if (-not $SkipChecks) {
        $fixture = Join-Path $root ('work/verification-' + [guid]::NewGuid().ToString('N'))
        Invoke-Step 'show workflow verification' { dotnet run --project tests/ShowPlayer.WorkflowCheck -c Release -- $fixture }
    }
    if ($Books) {
        Invoke-Step 'stage 2 chapters' { python tests/stage2_chapters.py @Books }
        Invoke-Step 'stage 3 resume (short)' { python tests/stage3_resume.py --books @Books --scale 0.1 }
        $check = Join-Path $root 'tests/ShowPlayer.BindingCheck/bin/x64/Release/net10.0/win-x64/ShowPlayer.BindingCheck.exe'
        Invoke-Step 'stage 3 through C# (short)' { python tests/stage3_resume.py --books @Books --scale 0.05 --harness $check --gc-stress }
    }

    if ([IO.Path]::GetFullPath($appDir) -ne [IO.Path]::GetFullPath((Join-Path $root 'dist/ShowPlayer'))) { throw 'Unexpected publish path.' }
    if (Test-Path $appDir) { Remove-Item -LiteralPath $appDir -Recurse -Force }
    Invoke-Step 'publish app' { dotnet publish src/ShowPlayer.App -c Release -o $appDir --nologo -v q }

    # Without the app's own .pri the published app crashes at startup.
    foreach ($file in 'ShowPlayer.exe', 'ShowPlayer.pri', 'show_player_ffi.dll', 'libmpv-2.dll', 'Assets/AppIcon.ico', 'tools/ffmpeg.exe', 'tools/ffprobe.exe', 'data/gd-shows.xml', 'data/gdshowsdb-LICENSE.txt') {
        if (-not (Test-Path (Join-Path $appDir $file))) { throw "The published folder is missing $file." }
    }

    $zip = Join-Path $dist "Gratitude-$version-win-x64-portable.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $appDir '*') -DestinationPath $zip -CompressionLevel Optimal
    Invoke-Step 'build installer' { & $InnoCompiler /Qp "/DAppVersion=$version" (Join-Path $PSScriptRoot 'installer.iss') }
    $setup = Join-Path $dist "Gratitude-$version-win-x64-setup.exe"
    Get-Item $setup, $zip | ForEach-Object { '{0}  {1:n1} MB' -f $_.FullName, ($_.Length / 1MB) }
}
finally {
    Pop-Location
}
