param(
    [string]$Target = "arm64-v8a",
    [string]$Profile = "release",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Path $PSScriptRoot -Parent

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found on PATH."
}

if (-not (Get-Command cargo-ndk -ErrorAction SilentlyContinue)) {
    throw "cargo-ndk not found. Install with: cargo install cargo-ndk"
}

Write-Host "Building Rust DSP staticlib for target $Target ($Profile)..."
$cargoProfile = if ($Profile -eq "debug") { "dev" } else { $Profile }
if ($cargoProfile -eq "dev") {
    cargo ndk -t $Target build --lib --no-default-features
} else {
    cargo ndk -t $Target build --profile $cargoProfile --lib --no-default-features
}

if ($Profile -eq "release") {
    $profileDir = "release"
} elseif ($Profile -eq "debug") {
    $profileDir = "debug"
} else {
    $profileDir = $Profile
}

switch ($Target) {
    "arm64-v8a" { $triple = "aarch64-linux-android" }
    "armeabi-v7a" { $triple = "armv7-linux-androideabi" }
    "x86" { $triple = "i686-linux-android" }
    "x86_64" { $triple = "x86_64-linux-android" }
    default { throw "Unsupported target: $Target" }
}

$libPath = Join-Path -Path $ProjectRoot -ChildPath "target\$triple\$profileDir\libhear_buds_dsp.a"

if (-not (Test-Path $libPath)) {
    throw "Expected library not found: $libPath"
}

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    Write-Host "Built: $libPath"
    exit 0
}

if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}

$dest = Join-Path -Path $OutDir -ChildPath "libhear_buds_dsp.a"
Copy-Item -Path $libPath -Destination $dest -Force

Write-Host "Copied to: $dest"


