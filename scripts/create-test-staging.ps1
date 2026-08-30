param(
    [string] $TargetRoot = (Join-Path $PSScriptRoot '..\.staging')
)

$ErrorActionPreference = 'Stop'
$TargetRoot = [IO.Path]::GetFullPath($TargetRoot)
if (Test-Path -LiteralPath $TargetRoot) {
    throw "Refusing to overwrite existing staging root: $TargetRoot"
}

function Invoke-Git([string] $WorkingDirectory, [string[]] $Arguments) {
    & git -C $WorkingDirectory @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git failed in ${WorkingDirectory}: git $($Arguments -join ' ')"
    }
}

function New-WorkingRepository([string] $Path, [string] $RemoteUrl) {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    Invoke-Git $Path @('init', '-q', '-b', 'main')
    Invoke-Git $Path @('config', 'user.email', 'xingshu-test@example.invalid')
    Invoke-Git $Path @('config', 'user.name', 'Xingshu Staging')
    Set-Content -LiteralPath (Join-Path $Path 'README.md') -Value "staging fixture`n" -Encoding utf8
    Invoke-Git $Path @('add', 'README.md')
    Invoke-Git $Path @('commit', '-q', '-m', 'staging fixture')
    if ($RemoteUrl) { Invoke-Git $Path @('remote', 'add', 'origin', $RemoteUrl) }
}

$remoteRoot = Join-Path $TargetRoot 'remotes'
$rootA = Join-Path $TargetRoot 'root-a'
$rootB = Join-Path $TargetRoot 'root-b'
$seed = Join-Path $TargetRoot 'seed'
New-Item -ItemType Directory -Path $remoteRoot, $rootA, $rootB, $seed -Force | Out-Null

$thirdPartyRemote = Join-Path $remoteRoot 'third-party-demo.git'
& git init --bare -q -- $thirdPartyRemote
if ($LASTEXITCODE -ne 0) { throw "failed to create bare fixture remote" }
$seedRepo = Join-Path $seed 'third-party-demo'
New-WorkingRepository $seedRepo $thirdPartyRemote
Invoke-Git $seedRepo @('push', '-q', '-u', 'origin', 'main')
Invoke-Git $thirdPartyRemote @('symbolic-ref', 'HEAD', 'refs/heads/main')

$thirdParty = Join-Path $rootA 'network/third-party-demo'
New-Item -ItemType Directory -Path (Split-Path -Parent $thirdParty) -Force | Out-Null
& git clone --quiet -- $thirdPartyRemote $thirdParty
if ($LASTEXITCODE -ne 0) { throw "failed to clone third-party fixture" }

$dirty = Join-Path $rootA 'network/dirty-demo'
& git clone --quiet -- $thirdPartyRemote $dirty
if ($LASTEXITCODE -ne 0) { throw "failed to clone dirty fixture" }
Set-Content -LiteralPath (Join-Path $dirty 'local-change.txt') -Value "uncommitted staging change`n" -Encoding utf8

$own = Join-Path $rootA 'self/own-demo'
New-Item -ItemType Directory -Path (Split-Path -Parent $own) -Force | Out-Null
New-WorkingRepository $own 'https://github.com/cc01cc/own-demo.git'

$fork = Join-Path $rootB 'agents/fork-demo'
New-Item -ItemType Directory -Path (Split-Path -Parent $fork) -Force | Out-Null
New-WorkingRepository $fork 'https://github.com/cc01cc/fork-demo.git'
Invoke-Git $fork @('remote', 'add', 'upstream', 'https://github.com/example/fork-demo.git')

$bare = Join-Path $rootB 'archives/bare-demo.git'
& git init --bare -q -- $bare
if ($LASTEXITCODE -ne 0) { throw "failed to create bare fixture" }

$manifest = [ordered]@{
    source = 'synthetic; never copied from S:\\zeogit-ref'
    rootA = $rootA
    rootB = $rootB
    repositories = @(
        'root-a/network/third-party-demo',
        'root-a/network/dirty-demo',
        'root-a/self/own-demo',
        'root-b/agents/fork-demo',
        'root-b/archives/bare-demo.git'
    )
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $TargetRoot 'manifest.json') -Encoding utf8
Write-Host "Created isolated Xingshu staging fixture: $TargetRoot"
