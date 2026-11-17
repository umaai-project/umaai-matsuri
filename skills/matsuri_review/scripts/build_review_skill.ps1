# Build the review binary and place it into this skill's bin/.
# cargo is only needed at build time; runtime uses bin/review.exe directly.
#
# Usage (from any directory):
#   powershell -File skills/matsuri_review/scripts/build_review_skill.ps1
#   powershell -File skills/matsuri_review/scripts/build_review_skill.ps1 -WithData
param(
    [switch]$WithData,
    [string]$TargetDir = ""
)
$ErrorActionPreference = "Continue"
$skill = Resolve-Path (Join-Path $PSScriptRoot "..")
$repo = Resolve-Path (Join-Path $skill "..\..")
if (-not $TargetDir) { $TargetDir = Join-Path $repo ".cargo-adv" }
$env:CARGO_TARGET_DIR = $TargetDir
Push-Location $repo
try {
    cargo build -p matsuri-tools --release --offline --bin review 2>&1 | Write-Host
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $exe = Join-Path $TargetDir "release/review.exe"
    if (-not (Test-Path $exe)) { throw "missing $exe" }
    $bin = Join-Path $skill "bin"
    New-Item -ItemType Directory -Force -Path $bin | Out-Null
    Copy-Item $exe (Join-Path $bin "review.exe") -Force
    "updated: " + (Join-Path $bin "review.exe")
    if ($WithData) {
        $src = Join-Path $repo "data"
        $dst = Join-Path $skill "data"
        if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
        Copy-Item $src $dst -Recurse
        "copied: $dst"
    }
    "run: " + (Join-Path $bin "review.exe") + " open <game.zip> --out <cwd>/game{id}"
} finally {
    Pop-Location
}
