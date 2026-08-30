param(
    [Parameter(Mandatory = $true)] [string] $SourceRoot,
    [Parameter(Mandatory = $true)] [string] $TargetRoot,
    [string[]] $Repository = @(),
    [switch] $Execute
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) {
    throw "Source root does not exist: $SourceRoot"
}
if ([IO.Path]::GetFullPath($TargetRoot).StartsWith([IO.Path]::GetFullPath($SourceRoot), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Target root must not be inside the source gitclone root.'
}

$items = if ($Repository.Count -gt 0) { $Repository } else {
    $sourcePrefix = [IO.Path]::GetFullPath($SourceRoot).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    Get-ChildItem -LiteralPath $SourceRoot -Directory -Recurse -Depth 4 |
        Where-Object {
            (Test-Path -LiteralPath (Join-Path $_.FullName '.git')) -or
            ((Test-Path -LiteralPath (Join-Path $_.FullName 'HEAD')) -and
             (Test-Path -LiteralPath (Join-Path $_.FullName 'objects')) -and
             (Test-Path -LiteralPath (Join-Path $_.FullName 'refs')))
        } |
        Select-Object -First 10 -ExpandProperty FullName |
        ForEach-Object { $_.Substring($sourcePrefix.Length) }
}
foreach ($item in $items) {
    $source = Join-Path $SourceRoot $item
    $target = Join-Path $TargetRoot $item
    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        Write-Warning "Skipping missing sample: $source"
        continue
    }
    $arguments = @($source, $target, '/E', '/COPY:DAT', '/DCOPY:DAT', '/XJ', '/R:1', '/W:1', '/L')
    if ($Execute) { $arguments = $arguments | Where-Object { $_ -ne '/L' } }
    Write-Host "robocopy $($arguments -join ' ')"
    if ($Execute) { & robocopy @arguments; if ($LASTEXITCODE -gt 7) { throw "robocopy failed: $LASTEXITCODE" } }
}
