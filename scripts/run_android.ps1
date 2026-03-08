param(
    [string]$Target = "arm64-v8a",
    [string]$Profile = "debug"
)

$ErrorActionPreference = "Stop"

& "$PSScriptRoot\\deploy_android.ps1" -Target $Target -Profile $Profile
