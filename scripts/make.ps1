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

# Visual Studio's installer puts vswhere here. It lists every installed edition and its components.
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"

# True when any Visual Studio edition (Community, Professional, Build Tools, ...) has the C++ compiler.
function Test-MsvcTools {
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}

# These count as installed however they got there, not only when winget installed them.
# Without this, setup installs a second copy next to one installed by hand.
$customChecks = @{
    "Git.Git"                                = { [bool](Get-Command git -ErrorAction SilentlyContinue) }
    "Rustlang.Rustup"                        = { [bool](Get-Command rustup -ErrorAction SilentlyContinue) }
    "StrawberryPerl.StrawberryPerl"          = { [bool](Get-Command perl -ErrorAction SilentlyContinue) }
    "Microsoft.VisualStudio.2022.BuildTools" = { Test-MsvcTools }
}

function Test-Installed([string]$Id) {
    if ($customChecks.ContainsKey($Id)) { return [bool](& $customChecks[$Id]) }
    winget list --id $Id --exact --accept-source-agreements *> $null
    return ($LASTEXITCODE -eq 0)
}

function Install-SystemDeps {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "winget is missing. Install 'App Installer' from the Microsoft Store, then run .\make setup again."
    }
    foreach ($entry in Get-Section "winget") {
        $id, $override = $entry -split '\|', 2
        $id = $id.Trim()
        if (Test-Installed $id) { Write-Host "  ok       $id"; continue }
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
    foreach ($tool in "rustup", "cargo") {
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
        if (Test-Installed $id) { Write-Host "  ok       $id" }
        else { Write-Host "  missing  $id" -ForegroundColor Yellow; $ok = $false }
    }
    if ($ok) { Write-Host "Everything is installed." } else { throw "Run: .\make setup" }
}

function Write-Step([string]$Text) { Write-Host "`n== $Text" -ForegroundColor Cyan }
function Write-Note([string]$Text) { Write-Host $Text -ForegroundColor Yellow }

# Start-of-day routine, the same as scripts/recap.sh on Linux.
# It only fast-forwards, so it cannot create merge commits or conflicts.
function Invoke-Recap {
    Write-Step "Code"
    $branch = git branch --show-current
    Write-Host "You are on: $branch"
    git fetch --prune --quiet origin
    if ($LASTEXITCODE -ne 0) { Write-Note "Could not reach GitHub. Continuing with the code you have." }

    $before = git rev-parse HEAD
    git rev-parse --abbrev-ref "@{u}" *> $null
    if ($LASTEXITCODE -eq 0) {
        git pull --ff-only --quiet
        if ($LASTEXITCODE -ne 0) { Write-Note "Could not update ${branch}: it has commits that GitHub does not have, and the reverse. Merge or rebase it yourself." }
    } else {
        Write-Note "$branch is not on GitHub yet. Push it with: git push -u origin $branch"
    }

    if ($before -ne (git rev-parse HEAD)) {
        Write-Host "New commits on ${branch}:"
        git log --oneline "$before..HEAD"
    } else {
        Write-Host "$branch is up to date."
    }

    if ($branch -ne "main") {
        git rev-parse -q --verify origin/main *> $null
        if ($LASTEXITCODE -eq 0) {
            $behind = [int](git rev-list --count HEAD..origin/main)
            if ($behind -gt 0) { Write-Note "main has $behind commit(s) that $branch does not have. To bring them in: git merge origin/main" }
        }
    }

    $changes = @(git status --porcelain).Count
    if ($changes -gt 0) { Write-Note "You have $changes file(s) with uncommitted changes." }

    git diff --quiet $before HEAD -- .dependencies
    if ($LASTEXITCODE -ne 0) { Write-Note ".dependencies changed in the new commits. Run: .\make setup" }

    Write-Step "Rust"
    rustup update --no-self-update 2>&1 | Select-String "updated|unchanged|error"

    Write-Step "Tools"
    try { Test-Deps *> $null; Write-Host "Everything in .dependencies is installed." }
    catch { Write-Note "Something in .dependencies is missing. Run: .\make doctor, then .\make setup" }

    Write-Step "Build"
    cargo build --quiet
    if ($LASTEXITCODE -eq 0) { Write-Host "Build OK." } else { Write-Note "The build failed. Read the errors above." }

    Write-Step "Pull requests"
    $gh = Get-Command gh -ErrorAction SilentlyContinue
    if ($gh) { gh auth status *> $null }
    if ($gh -and $LASTEXITCODE -eq 0) { gh pr status }
    else { Write-Host "Install and log in to the GitHub CLI (gh) to see pull requests waiting for your review here." }
}

$targets = [ordered]@{
    help    = "Show this list"
    setup   = "Install everything in .dependencies"
    recap   = "Start of the day: get the latest code, update Rust, check tools, build"
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
try {
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
    "recap"   { Invoke-Recap }
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
} catch {
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}