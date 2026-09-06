param(
    [string] $TargetRoot = "H:/zeogit/one/A08-xingshu/.staging/real-mini-001",
    [string] $SourceRoot = "S:/zeogit-ref",
    [string[]] $Repository = @("drizzle-team/drizzle-orm","vitejs/vite","cline/cline","Aider-AI/aider","shadcn-ui/ui","2dust/v2rayN"),
    [switch] $WithEdgeCases
)
$ErrorActionPreference = "Stop"
if (Test-Path -LiteralPath $TargetRoot) { throw "Refusing to overwrite existing staging root: $TargetRoot" }
if ([IO.Path]::GetFullPath($TargetRoot).StartsWith([IO.Path]::GetFullPath($SourceRoot), [StringComparison]::OrdinalIgnoreCase)) { throw "Target must not be inside source" }

Write-Host "Creating real-test staging from S:/zeogit-ref -> $TargetRoot"
Write-Host "Repositories: $($Repository -join ', ')"

# stage-sample handles robocopy with /L dry-run guard; use -Execute for real copy
& "$PSScriptRoot/stage-sample.ps1" -SourceRoot $SourceRoot -TargetRoot $TargetRoot -Repository $Repository -Execute
if ($LASTEXITCODE -gt 7) { throw "stage-sample failed with $LASTEXITCODE" }

# Verify .git exists
foreach ($rel in $Repository) {
    $full = Join-Path $TargetRoot $rel
    if (-not (Test-Path (Join-Path $full ".git"))) { Write-Warning "Missing .git for $rel at $full" }
}

if ($WithEdgeCases) {
    Write-Host "Injecting edge cases (bare, nested, broken, bak, dirty)..."
    $nested = Join-Path $TargetRoot "drizzle-team/drizzle-orm/embedded/inner"
    if (-not (Test-Path $nested)) {
        New-Item -ItemType Directory -Path $nested -Force | Out-Null
        & git -C $nested init -q -b main
        & git -C $nested config user.email "xingshu-test@example.invalid"
        & git -C $nested config user.name "Xingshu Edge"
        Set-Content -LiteralPath (Join-Path $nested "README.md") -Value "nested fixture`n" -Encoding utf8
        & git -C $nested add README.md
        & git -C $nested commit -q -m "nested fixture"
    }
    $bare = Join-Path $TargetRoot "archives/bare-demo.git"
    if (-not (Test-Path $bare)) { & git init --bare -q -- $bare }

    $broken = Join-Path $TargetRoot "broken/demo"
    if (-not (Test-Path $broken)) {
        New-Item -ItemType Directory -Path (Join-Path $broken ".git") -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $broken ".git/config") -Value "[core]`n`trepositoryformatversion = 0`n" -Encoding utf8
    }
    $bakSrc = Join-Path $TargetRoot "vitejs/vite/.git"
    $bakDst = Join-Path $TargetRoot "vitejs/vite.bak.20250101/.git"
    if ((Test-Path $bakSrc) -and -not (Test-Path $bakDst)) {
        New-Item -ItemType Directory -Path (Split-Path $bakDst) -Force | Out-Null
        Copy-Item -LiteralPath $bakSrc -Destination $bakDst -Recurse -Force
    }
    $dirty = Join-Path $TargetRoot "shadcn-ui/ui/local-dirty.txt"
    if (-not (Test-Path $dirty)) { Set-Content -LiteralPath $dirty -Value "dirty for pull test`n" -Encoding utf8 }
    Write-Host "Edge cases injected"
}

# Initialize DB and scan
$db = Join-Path $TargetRoot "xingshu-dev.db"
Write-Host "Registering root and scanning -> $db"
& cargo run --manifest-path "$PSScriptRoot/../Cargo.toml" -p xingshu-cli -- --db $db roots add $TargetRoot | Out-Null
& cargo run --manifest-path "$PSScriptRoot/../Cargo.toml" -p xingshu-cli -- --db $db scan | Out-String -Width 300 | Write-Host
Write-Host "List stats:"
& cargo run --manifest-path "$PSScriptRoot/../Cargo.toml" -p xingshu-cli -- --db $db stats | Write-Host
Write-Host "Staging ready: $TargetRoot"
Write-Host "  DB: $db"
Write-Host "  Use: `$env:XINGSHU_DB='$db'; cargo run -p xingshu-server  # API 12681"
Write-Host "       cargo run -p xingshu-cli -- --db `$env:XINGSHU_DB list"
