param(
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"

$workspaceRoot = "E:\360Downloads\claw-code-main"
$rustRoot = Join-Path $workspaceRoot "rust"
$tempRoot = Join-Path $env:LOCALAPPDATA "Temp"

$candidatePatterns = @(
    (Join-Path $workspaceRoot "verify-target"),
    (Join-Path $workspaceRoot "codex-target"),
    (Join-Path $rustRoot "target"),
    (Join-Path $rustRoot "target-*"),
    (Join-Path $rustRoot ".codex-target-*"),
    "E:\claw-target-*",
    "E:\claw-runtime-test-tmp",
    "E:\claw-cli-test-tmp",
    (Join-Path $tempRoot "claw-*"),
    (Join-Path $tempRoot "clawd-*")
)

$allowedRoots = @(
    $workspaceRoot,
    "E:\",
    "$tempRoot\"
)

function Resolve-CandidateDirectories {
    param(
        [string[]]$Patterns
    )

    $resolved = New-Object System.Collections.Generic.List[string]
    foreach ($pattern in $Patterns) {
        if ($pattern.IndexOfAny(@('*', '?')) -ge 0) {
            Get-ChildItem -Path $pattern -Directory -Force -ErrorAction SilentlyContinue |
                ForEach-Object {
                    if (-not $resolved.Contains($_.FullName)) {
                        $resolved.Add($_.FullName)
                    }
                }
        } elseif (Test-Path -LiteralPath $pattern -PathType Container) {
            if (-not $resolved.Contains($pattern)) {
                $resolved.Add($pattern)
            }
        }
    }

    $resolved
}

function Test-AllowedPath {
    param(
        [string]$Path
    )

    foreach ($root in $allowedRoots) {
        if ($Path.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

function Get-DirectorySizeBytes {
    param(
        [string]$Path
    )

    $files = Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue
    ($files | Measure-Object -Property Length -Sum).Sum
}

$targets = Resolve-CandidateDirectories -Patterns $candidatePatterns |
    Sort-Object -Unique |
    Where-Object { Test-Path -LiteralPath $_ } |
    Where-Object { Test-AllowedPath -Path $_ }

if (-not $targets) {
    Write-Host "No matching test artifacts found."
    exit 0
}

$summary = foreach ($target in $targets) {
    $sizeBytes = Get-DirectorySizeBytes -Path $target
    [PSCustomObject]@{
        Path = $target
        SizeBytes = [int64]$sizeBytes
        SizeGB = [math]::Round(($sizeBytes / 1GB), 2)
    }
}

$summary | Sort-Object SizeGB -Descending | Format-Table -AutoSize

if ($WhatIf) {
    Write-Host "Dry run only. Nothing was deleted."
    exit 0
}

$freedBytes = 0L
foreach ($item in $summary) {
    if (-not (Test-Path -LiteralPath $item.Path)) {
        continue
    }

    $freedBytes += $item.SizeBytes
    Remove-Item -LiteralPath $item.Path -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ("Deleted {0} directories. Estimated freed: {1} GB" -f $summary.Count, [math]::Round(($freedBytes / 1GB), 2))
