# LierSync Mobile — Build Rust core (.so) into Android jniLibs
# Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

$ErrorActionPreference = "Stop"

if (-not $env:ANDROID_HOME) {
    $env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
}
if (-not $env:ANDROID_NDK_HOME) {
    $ndkRoot = Join-Path $env:ANDROID_HOME "ndk"
    if (Test-Path $ndkRoot) {
        $latestNdk = Get-ChildItem $ndkRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
        if ($latestNdk) {
            $env:ANDROID_NDK_HOME = $latestNdk.FullName
        }
    }
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
$JniLibsDir = Join-Path $PSScriptRoot "app\src\main\jniLibs"

Push-Location $RepoRoot
try {
    cargo ndk `
        -t arm64-v8a `
        -t x86_64 `
        -o $JniLibsDir `
        build -p lier-sync-android --release
} finally {
    Pop-Location
}
Write-Host "Rust Android .so libraries built successfully!" -ForegroundColor Green
