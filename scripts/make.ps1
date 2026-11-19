# Build commands for Windows, with the same targets as the Makefile.
# Do not run this file directly. From the repo folder run:   .\make <target>   (for example .\make setup)
# make.cmd starts this script without needing a change to PowerShell's script policy.
# Everything that gets installed is listed in .dependencies.

param([string]$Target = "help")

# Run from the repo root, whichever folder the command was typed in.
Set-Location (Split-Path $PSScriptRoot -Parent)

# Read the items of one [section] in .dependencies, without comments.
function Get-Section([string]$Name) {
    $inSection = $false
    foreach ($line in Get-Content ".dependencies" -ErrorAction Stop) {
        if ($line -match '^\s*\[(.+)\]') { $inSection = ($Matches[1] -eq $Name); continue }
        if (-not $inSection) { continue }
        $item = ($line -replace '#.*$', '').Trim()
        if ($item) { $item }
    }
}

# Pick up PATH changes made by installers, so new tools work without reopening the terminal.
function Update-Path {
    $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                [Environment]::GetEnvironmentVariable("Path", "User") + ";" +
                (Join-Path $env:USERPROFILE ".cargo\bin")
}

# Run a program and stop if it fails. PowerShell does not do this on its own.
function Invoke-Step([string]$Exe, [string[]]$Arguments) {
    Write-Host "> $Exe $($Arguments -join ' ')" -ForegroundColor DarkGray
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

function Install-SystemDeps {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "winget is missing. Install 'App Installer' from the Microsoft Store, then run .\make setup again."
    }
    foreach ($entry in Get-Section "winget") {
        $id, $override = $entry -split '\|', 2
        $id = $id.Trim()
        winget list --id $id --exact --accept-source-agreements *> $null
        if ($LASTEXITCODE -eq 0) { Write-Host "  ok       $id"; continue }
        Write-Host "  install  $id" -ForegroundColor Cyan
        $wingetArgs = @("install", "--id", $id, "--exact", "--accept-package-agreements", "--accept-source-agreements")
        if ($override) { $wingetArgs += @("--override", $override.Trim()) }
        Invoke-Step "winget" $wingetArgs
    }
    Update-Path
}

function Install-Rust {
    Update-Path
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        throw "rustup is not on PATH. Close this terminal, open a new one, and run .\make setup again."
    }
    $toolchain = @(Get-Section "rust-toolchain")[0]
    $components = @(Get-Section "rust-components" | ForEach-Object { "--component", $_ })
    Invoke-Step "rustup" (@("toolchain", "install", $toolchain, "--profile", "minimal") + $components)
    Invoke-Step "rustup" @("default", $toolchain)
}

function Test-Deps {
    Update-Path
    $ok = $true
    foreach ($tool in "rustup", "cargo", "git", "perl") {
        if (Get-Command $tool -ErrorAction SilentlyContinue) { Write-Host "  ok       $tool" }
        else { Write-Host "  missing  $tool" -ForegroundColor Yellow; $ok = $false }
    }
    if (Get-Command rustup -ErrorAction SilentlyContinue) {
        $installed = rustup component list --installed
        foreach ($c in Get-Section "rust-components") {
            if ($installed -match "^$c") { Write-Host "  ok       $c" }
            else { Write-Host "  missing  $c" -ForegroundColor Yellow; $ok = $false }
        }
    }
    foreach ($entry in Get-Section "winget") {
        $id = ($entry -split '\|', 2)[0].Trim()
        winget list --id $id --exact --accept-source-agreements *> $null
        if ($LASTEXITCODE -eq 0) { Write-Host "  ok       $id" }
        else { Write-Host "  missing  $id" -ForegroundColor Yellow; $ok = $false }
    }
    if ($ok) { Write-Host "Everything is installed." } else { throw "Run: .\make setup" }
}

$targets = [ordered]@{
    help    = "Show this list"
    setup   = "Install everything in .dependencies"
    doctor  = "Check that everything is installed, without changing anything"
    build   = "Build a debug version"
    release = "Build an optimised version"
    run     = "Start the chat (start it in two terminals to talk to yourself)"
    test    = "Run all tests"
    fmt     = "Format all code"
    lint    = "Run clippy and treat warnings as errors"
    check   = "Run the same checks as CI: format, lint, test"
    clean   = "Delete build output"
}

Update-Path
switch ($Target) {
    "help" {
        Write-Host "Usage: .\make <target>`n"
        foreach ($t in $targets.GetEnumerator()) { Write-Host ("  {0,-12} {1}" -f $t.Key, $t.Value) }
    }
    "setup" {
        Install-SystemDeps
        Install-Rust
        Write-Host "`nSetup done. Reopen the terminal so 'cargo' is on your PATH, then run: .\make build"
    }
    "doctor"  { Test-Deps }
    "build"   { Invoke-Step "cargo" @("build") }
    "release" { Invoke-Step "cargo" @("build", "--release") }
    "run"     { Invoke-Step "cargo" @("run", "--release") }
    "test"    { Invoke-Step "cargo" @("test") }
    "fmt"     { Invoke-Step "cargo" @("fmt", "--all") }
    "lint"    { Invoke-Step "cargo" @("clippy", "--all-targets", "--", "-D", "warnings") }
    "check" {
        Invoke-Step "cargo" @("fmt", "--all", "--check")
        Invoke-Step "cargo" @("clippy", "--all-targets", "--", "-D", "warnings")
        Invoke-Step "cargo" @("test")
    }
    "clean"   { Invoke-Step "cargo" @("clean") }
    default   { Write-Host "Unknown target '$Target'. Run .\make help"; exit 1 }
}
