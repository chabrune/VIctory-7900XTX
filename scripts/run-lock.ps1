# One run of the title at a time on this computer, whoever starts it (any
# session, any worktree of this repository): scripts\play.ps1 takes the claim
# before it starts the title and lets it go when the title has ended. The
# claim lives in the title's cache directory, which every build on this
# computer shares, so a run started from another worktree is seen here too.
#
#   scripts\run-lock.ps1 -Take <exe> -Holder <pid> [-Console series-s]
#       Prints the claim's path, or nothing (and says who) if a run is
#       already going. A claim whose holder is gone is taken over.
#   scripts\run-lock.ps1 -Release
#       Lets the claim go.
#
# A run of the title is also refused while any victory.exe or the title's
# own process is alive: a claim left behind by a killed window is not
# trusted on its own.
param(
    [string]$Take,
    [int]$Holder = 0,
    [string]$Console = 'series-s',
    [switch]$Release
)
$dir = Join-Path $env:LOCALAPPDATA 'victory-7900xtx'
$claim = Join-Path $dir 'run.lock'
if ($Release) {
    Remove-Item $claim -ErrorAction SilentlyContinue
    return
}
$going = Get-Process victory, game_scarlett_master -ErrorAction SilentlyContinue
if ($going) {
    $who = ($going | ForEach-Object { "$($_.ProcessName) $($_.Id)" }) -join ', '
    Write-Host "A run of the title is already going ($who): not starting another."
    return
}
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$body = [pscustomobject]@{
    holder = $Holder
    started = (Get-Date).ToString('o')
    worktree = $PSScriptRoot
    exe = $Take
    console = $Console
} | ConvertTo-Json
try {
    $stream = [System.IO.File]::Open($claim, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write)
} catch [System.IO.IOException] {
    $held = $null
    try { $held = Get-Content $claim -Raw | ConvertFrom-Json } catch { }
    if ($held -and (Get-Process -Id $held.holder -ErrorAction SilentlyContinue)) {
        Write-Host "A run of the title is already going (pid $($held.holder), started $($held.started), from $($held.worktree)): not starting another."
        return
    }
    Remove-Item $claim -ErrorAction SilentlyContinue
    $stream = [System.IO.File]::Open($claim, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write)
}
$bytes = [System.Text.Encoding]::UTF8.GetBytes($body)
$stream.Write($bytes, 0, $bytes.Length)
$stream.Close()
Write-Output $claim
