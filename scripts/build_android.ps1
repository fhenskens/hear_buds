param(
    [string]$Target = "arm64-v8a",
    [string]$Profile = "release",
    [string]$DxArgs = ""
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Path $PSScriptRoot -Parent

if ($Profile -notin @("debug", "release", "minsize")) {
    throw "Unsupported -Profile '$Profile'. Use debug, release, or minsize."
}

function Resolve-Triple([string]$target) {
    switch ($target) {
        "arm64-v8a" { return "aarch64-linux-android" }
        "armeabi-v7a" { return "armv7-linux-androideabi" }
        "x86" { return "i686-linux-android" }
        "x86_64" { return "x86_64-linux-android" }
        default { throw "Unsupported target: $target" }
    }
}

function Resolve-SdkRoot {
    if (-not [string]::IsNullOrWhiteSpace($env:ANDROID_HOME)) {
        return $env:ANDROID_HOME
    }
    if (-not [string]::IsNullOrWhiteSpace($env:ANDROID_SDK_ROOT)) {
        return $env:ANDROID_SDK_ROOT
    }
    $default = Join-Path -Path $env:LOCALAPPDATA -ChildPath "Android\\Sdk"
    if (Test-Path $default) {
        return $default
    }
    return $null
}

function Resolve-LatestNdk([string]$sdkRoot) {
    $ndkRoot = Join-Path -Path $sdkRoot -ChildPath "ndk"
    if (-not (Test-Path $ndkRoot)) {
        return $null
    }
    $candidates = Get-ChildItem -Path $ndkRoot -Directory | Sort-Object Name -Descending
    if ($candidates.Count -gt 0) {
        return $candidates[0].FullName
    }
    return $null
}

function Resolve-JavaHome {
    if (-not [string]::IsNullOrWhiteSpace($env:JAVA_HOME)) {
        return $env:JAVA_HOME
    }
    $candidates = @(
        (Join-Path -Path $env:LOCALAPPDATA -ChildPath "Programs\\Android Studio\\jbr"),
        "C:\\Program Files\\Android\\Android Studio\\jbr",
        "C:\\Program Files (x86)\\Android\\Android Studio\\jbr"
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    return $null
}

function Ensure-AndroidEnv {
    $sdkRoot = Resolve-SdkRoot
    if ($null -ne $sdkRoot -and [string]::IsNullOrWhiteSpace($env:ANDROID_HOME)) {
        $env:ANDROID_HOME = $sdkRoot
    }
    if ($null -ne $sdkRoot -and [string]::IsNullOrWhiteSpace($env:ANDROID_SDK_ROOT)) {
        $env:ANDROID_SDK_ROOT = $sdkRoot
    }
    if ([string]::IsNullOrWhiteSpace($env:ANDROID_NDK_HOME) -and $null -ne $sdkRoot) {
        $ndk = Resolve-LatestNdk $sdkRoot
        if ($null -ne $ndk) {
            $env:ANDROID_NDK_HOME = $ndk
        }
    }
    if ([string]::IsNullOrWhiteSpace($env:ANDROID_NDK_HOME)) {
        throw "ANDROID_NDK_HOME is not set and no NDK could be found. Set ANDROID_NDK_HOME to your NDK root."
    }
    if ([string]::IsNullOrWhiteSpace($env:JAVA_HOME)) {
        $javaHome = Resolve-JavaHome
        if ($null -ne $javaHome) {
            $env:JAVA_HOME = $javaHome
        } else {
            Write-Warning "JAVA_HOME is not set. Android builds will fail without a JDK. Set JAVA_HOME to your JDK or Android Studio JBR."
        }
    }
}

function Find-GradleRoot {
    $dxRoot = Join-Path -Path $ProjectRoot -ChildPath "target\\dx"
    if (-not (Test-Path $dxRoot)) {
        return $null
    }
    $gradlew = Get-ChildItem -Path $dxRoot -Recurse -Filter gradlew.bat -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -eq $gradlew) {
        return $null
    }
    return (Split-Path -Path $gradlew.FullName -Parent)
}

function Ensure-ManifestPermission([string]$gradleRoot) {
    $manifest = Join-Path -Path $gradleRoot -ChildPath "app\\src\\main\\AndroidManifest.xml"
    if (-not (Test-Path $manifest)) {
        return $false
    }
    $content = Get-Content -Path $manifest -Raw
    if ($content -match "android.permission.RECORD_AUDIO") {
        return $false
    }
    $insertion = "    <uses-permission android:name=`"android.permission.RECORD_AUDIO`" />"
    if ($content -match "<manifest[^>]*>") {
        $content = $content -replace "(<manifest[^>]*>)", "`$1`r`n$insertion"
    } else {
        return $false
    }
    Set-Content -Path $manifest -Value $content
    return $true
}

function Ensure-LocalProperties([string]$gradleRoot) {
    $sdkRoot = $env:ANDROID_HOME
    if ([string]::IsNullOrWhiteSpace($sdkRoot)) {
        $sdkRoot = $env:ANDROID_SDK_ROOT
    }
    if ([string]::IsNullOrWhiteSpace($sdkRoot)) {
        return
    }
    $propsPath = Join-Path -Path $gradleRoot -ChildPath "local.properties"
    $escaped = $sdkRoot -replace "\\", "\\\\"
    $content = "sdk.dir=$escaped"
    Set-Content -Path $propsPath -Value $content
}

Ensure-AndroidEnv

$triple = Resolve-Triple $Target

$dxProfileArgs = @()
if ($Profile -eq "release" -or $Profile -eq "minsize") {
    $dxProfileArgs += "--release"
}
if ($Profile -eq "minsize") {
    if ([string]::IsNullOrWhiteSpace($env:CARGO_PROFILE_RELEASE_OPT_LEVEL)) { $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "s" }
    if ([string]::IsNullOrWhiteSpace($env:CARGO_PROFILE_RELEASE_LTO)) { $env:CARGO_PROFILE_RELEASE_LTO = "thin" }
    if ([string]::IsNullOrWhiteSpace($env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS)) { $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1" }
    if ([string]::IsNullOrWhiteSpace($env:CARGO_PROFILE_RELEASE_STRIP)) { $env:CARGO_PROFILE_RELEASE_STRIP = "symbols" }
    if ([string]::IsNullOrWhiteSpace($env:CARGO_PROFILE_RELEASE_PANIC)) { $env:CARGO_PROFILE_RELEASE_PANIC = "abort" }
}

Write-Host "Running: dx build --platform android $($dxProfileArgs -join ' ') --no-default-features --features mobile $DxArgs"
$baseArgs = @("build", "--platform", "android", "--target", $triple) + $dxProfileArgs + @("--no-default-features", "--features", "mobile")
if ([string]::IsNullOrWhiteSpace($DxArgs)) {
    & dx @baseArgs
} else {
    & dx @baseArgs $DxArgs
}

$gradleRoot = Find-GradleRoot
if ($null -ne $gradleRoot) {
    $changed = Ensure-ManifestPermission $gradleRoot
    Ensure-LocalProperties $gradleRoot
    if ($changed) {
        Write-Host "Added RECORD_AUDIO permission. Rebuilding APK with Gradle..."
        $gradlew = Join-Path -Path $gradleRoot -ChildPath "gradlew.bat"
        $task = if ($Profile -eq "release") { "assembleRelease" } else { "assembleDebug" }
        Push-Location $gradleRoot
        try {
            & $gradlew $task
        } finally {
            Pop-Location
        }
        if ($LASTEXITCODE -ne 0) {
            throw "Gradle build failed with exit code $LASTEXITCODE"
        }
    }
}


