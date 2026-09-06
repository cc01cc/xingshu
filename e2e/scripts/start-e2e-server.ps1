param()
$ErrorActionPreference = "Stop"
$ProjectRoot = (Resolve-Path "$PSScriptRoot/../..").Path
$Staging = Join-Path $ProjectRoot ".staging/e2e-playwright"
$Db = Join-Path $Staging "xingshu.db"

# 端口预检：残留的上一轮 server 会让本轮 409（pull task already running），必须先清场
$portListeners = Get-NetTCPConnection -LocalPort 12681 -State Listen -ErrorAction SilentlyContinue
foreach ($listener in $portListeners) {
    Write-Host "[e2e] killing stale server pid=$($listener.OwningProcess) on port 12681"
    Stop-Process -Id $listener.OwningProcess -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

Write-Host "[e2e] staging=$Staging db=$Db"

# Clean previous staging if exists (idempotent for webServer reuse)
if (Test-Path -LiteralPath $Staging) {
  Write-Host "[e2e] cleaning previous staging"
  Remove-Item -LiteralPath $Staging -Recurse -Force
}

# Create synthetic staging (5 repos: own/fork/third-party/bare/dirty)
Write-Host "[e2e] creating synthetic staging"
pwsh -NoProfile -File (Join-Path $ProjectRoot "scripts/create-test-staging.ps1") -TargetRoot $Staging
if ($LASTEXITCODE -ne 0) { throw "create-test-staging failed" }

# Register roots and scan via xingshu-cli
# Roots are $Staging/root-a and $Staging/root-b
$rootA = Join-Path $Staging "root-a"
$rootB = Join-Path $Staging "root-b"
Write-Host "[e2e] registering roots $rootA , $rootB"
& cargo run --manifest-path (Join-Path $ProjectRoot "Cargo.toml") -p xingshu-cli -- --db $Db roots add $rootA | Out-String | Write-Host
if ($LASTEXITCODE -ne 0) { throw "roots add root-a failed" }
& cargo run --manifest-path (Join-Path $ProjectRoot "Cargo.toml") -p xingshu-cli -- --db $Db roots add $rootB | Out-String | Write-Host
if ($LASTEXITCODE -ne 0) { throw "roots add root-b failed" }

Write-Host "[e2e] scanning"
& cargo run --manifest-path (Join-Path $ProjectRoot "Cargo.toml") -p xingshu-cli -- --db $Db scan | Out-String | Write-Host
if ($LASTEXITCODE -ne 0) { throw "scan failed" }

Write-Host "[e2e] stats"
& cargo run --manifest-path (Join-Path $ProjectRoot "Cargo.toml") -p xingshu-cli -- --db $Db stats | Out-String | Write-Host

# Ensure webui/dist exists
$dist = Join-Path $ProjectRoot "webui/dist/index.html"
if (-not (Test-Path -LiteralPath $dist)) {
  Write-Host "[e2e] building webui"
  Push-Location (Join-Path $ProjectRoot "webui")
  pnpm --ignore-workspace run build | Out-String | Write-Host
  Pop-Location
  if ($LASTEXITCODE -ne 0) { throw "webui build failed" }
}

Write-Host "[e2e] starting xingshu-server on 127.0.0.1:12681 with DB=$Db"
$env:XINGSHU_DB = $Db
$env:XINGSHU_PORT = "12681"
$env:XINGSHU_HOST = "127.0.0.1"
# Exec server (webServer will wait for /health)
& cargo run --manifest-path (Join-Path $ProjectRoot "Cargo.toml") -p xingshu-server
