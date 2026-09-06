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

# diverged-demo：上游 force-push 重写历史后本地与上游分叉
$divergedRemote = Join-Path $remoteRoot 'diverged-demo.git'
& git init --bare -q -- $divergedRemote
if ($LASTEXITCODE -ne 0) { throw "failed to create diverged bare fixture remote" }
$divergedSeed = Join-Path $seed 'diverged-demo'
New-WorkingRepository $divergedSeed $divergedRemote
Invoke-Git $divergedSeed @('push', '-q', '-u', 'origin', 'main')
Invoke-Git $divergedRemote @('symbolic-ref', 'HEAD', 'refs/heads/main')
# 上游追加 base 提交，本地同步到同一点
Set-Content -LiteralPath (Join-Path $divergedSeed 'base.txt') -Value "shared base`n" -Encoding utf8
Invoke-Git $divergedSeed @('add', 'base.txt')
Invoke-Git $divergedSeed @('commit', '-q', '-m', 'shared base commit')
Invoke-Git $divergedSeed @('push', '-q')
$diverged = Join-Path $rootB 'network/diverged-demo'
New-Item -ItemType Directory -Path (Split-Path -Parent $diverged) -Force | Out-Null
& git clone --quiet -- $divergedRemote $diverged
if ($LASTEXITCODE -ne 0) { throw "failed to clone diverged fixture" }
# 本地提交 rewritten.txt（与上游即将重做的提交同文件同内容 → patch 等价，模拟回退后重做）
Set-Content -LiteralPath (Join-Path $diverged 'rewritten.txt') -Value "rewritten upstream commit`n" -Encoding utf8
Invoke-Git $diverged @('add', 'rewritten.txt')
Invoke-Git $diverged @('commit', '-q', '-m', 'rewritten upstream commit')
# 上游 reset 回 base 前一个提交（丢弃 shared base），重新提交同内容 base 重写历史后 force-push
Invoke-Git $divergedSeed @('reset', '-q', '--hard', 'HEAD~1')
Set-Content -LiteralPath (Join-Path $divergedSeed 'rewritten.txt') -Value "rewritten upstream commit`n" -Encoding utf8
Invoke-Git $divergedSeed @('add', 'rewritten.txt')
Invoke-Git $divergedSeed @('commit', '-q', '-m', 'rewritten upstream commit')
Invoke-Git $divergedSeed @('push', '-q', '--force')

$manifest = [ordered]@{
    source = 'synthetic; never copied from S:\\zeogit-ref'
    rootA = $rootA
    rootB = $rootB
    repositories = @(
        'root-a/network/third-party-demo',
        'root-a/network/dirty-demo',
        'root-a/self/own-demo',
        'root-b/agents/fork-demo',
        'root-b/archives/bare-demo.git',
        'root-b/network/diverged-demo'
    )
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $TargetRoot 'manifest.json') -Encoding utf8
Write-Host "Created isolated Xingshu staging fixture: $TargetRoot"
