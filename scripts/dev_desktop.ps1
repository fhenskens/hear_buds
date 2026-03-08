param(
    [string]$DxArgs = ""
)

$ErrorActionPreference = "Stop"

Write-Host "Starting desktop development build..."

if ([string]::IsNullOrWhiteSpace($DxArgs)) {
    dx serve --platform desktop --no-default-features --features desktop
} else {
    dx serve --platform desktop --no-default-features --features desktop $DxArgs
}
