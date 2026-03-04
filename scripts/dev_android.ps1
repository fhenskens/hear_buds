param(
    [string]$Target = "arm64-v8a",
    [string]$Profile = "debug",
    [string]$DxArgs = "",
    [switch]$NoLogcat
)

$ErrorActionPreference = "Stop"

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

function Ensure-AndroidEnv {
    $sdkRoot = Resolve-SdkRoot
    if ($null -ne $sdkRoot -and [string]::IsNullOrWhiteSpace($env:ANDROID_HOME)) {
        $env:ANDROID_HOME = $sdkRoot
    }
    if ($null -ne $sdkRoot -and [string]::IsNullOrWhiteSpace($env:ANDROID_SDK_ROOT)) {
        $env:ANDROID_SDK_ROOT = $sdkRoot
    }
}

Ensure-AndroidEnv

Write-Host "Building + deploying Android app..."
& "$PSScriptRoot\\deploy_android.ps1" -Target $Target -Profile $Profile -DxArgs $DxArgs
if ($LASTEXITCODE -ne 0) {
    throw "deploy_android.ps1 failed with exit code $LASTEXITCODE"
}

if (-not $NoLogcat) {
    $sdkRoot = Resolve-SdkRoot
    if ($null -eq $sdkRoot) {
        Write-Warning "ANDROID_HOME/ANDROID_SDK_ROOT not set. Skipping logcat."
        exit 0
    }
    $adb = Join-Path -Path $sdkRoot -ChildPath "platform-tools\\adb.exe"
    if (-not (Test-Path $adb)) {
        Write-Warning "adb not found at $adb. Skipping logcat."
        exit 0
    }
    Write-Host "Tailing logcat (filter: HearBuds). Press Ctrl+C to stop."
    & $adb logcat -v time | Select-String HearBuds
}

