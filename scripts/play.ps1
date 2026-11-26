# Starts the title with the settings this fork is played with (README.md, "How to run it").
#   scripts\play.ps1 -Exe <path of your own copy's executable>
# -Safe         without the newer, faster ways (the first thing to try when something is wrong)
# -RayTracing   the title's own ray-traced indirect light: lit interiors, about a third slower
# -Build        target\diag (the default, the build all measurements are of) or target\release
# -More         other settings: -More @{VICTORY_PANEL='1'}
# -Console      series-x (the default: 3840x2160) or series-s: the title renders lower and
#               scales up to 1920x1080 itself; on an RX 7700 XT 17-20 fps in the opening
#               cutscene against about 10 (2026-11-26). Its menus show black: press Enter
#               (or A) on the black screen and the title goes on.
# Run it from a normal window, not as administrator: the driver then keeps its shader cache elsewhere.
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [ValidateSet('series-x', 'series-s')][string]$Console = 'series-x',
    [hashtable]$More = @{}
)
$settings = @{
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'
    VICTORY_JOIN_COPIES = '1'; VICTORY_INDIRECT_WINDOW = '1'; VICTORY_RELAY_COPY = '1'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_TITLE_PRIORITIES = '0'; VICTORY_CARD_AWAKE = '1'
    VICTORY_SHOW_AFTER = '1'; VICTORY_AMBIENT = '0.8'; VICTORY_D3D12_DRED = 'light'
}
if ($Safe) {
    $settings.VICTORY_UNDER_WIDE = '0'; $settings.VICTORY_LAND_ECHO = '0'
} else {
    $settings.VICTORY_THIN = '1'; $settings.VICTORY_JOIN_PARTS = '8'; $settings.VICTORY_UPLOAD_AHEAD = '1'
}
if ($RayTracing) { $settings.VICTORY_BVH_BUILDER = '1' }
if ($Console -eq 'series-s') {
    # Every title buffer's copy from 64 KiB up in the card's memory (at most
    # 1 GiB of them), so that the card, not the CPU, finds what shaders wrote
    # there. RX 7700 XT, opening cutscene, three runs against two: 17.0-18.0 fps
    # -> 21.3, 16.8 (one run with slow stretches) and 20.5; copying back 30 -> 20
    # ms a frame, preparing 47-49 -> 38-45, the card working 45 -> 32 ms. The
    # card's memory then stands at 9.2-9.7 of a 10.2 GB budget, which Series X
    # (already over its budget) has no room for.
    $settings.VICTORY_CARD_BUFFERS_MIN_KB = '64'; $settings.VICTORY_CARD_BUFFERS_MB = '1024'
    # The title renders 1280x720 and its FidelityFX upscaler takes it to
    # 1920x1080 with its sharpening off; on at half strength the picture is
    # visibly sharper (hair, edges), at the same frame rate (20.1 fps both, the
    # same scene, RX 7700 XT).
    $settings.VICTORY_FSR_SHARPNESS = '0.5'
    # 1 GiB of the windows of title memory (its own vertices, indices and constants) in card
    # memory the CPU writes straight into, so that draws do not read them over the bus and the
    # PC keeps no second copy. RX 7700 XT, opening cutscene (2026-11-26), none / 1 GiB / 2 GiB:
    # card working 32 / - / 27 ms a frame, the PC's free memory 1.5 / 2.3 / 3.5 GB, textures
    # alive on the card 50,000 / 38,000 / 20,000 (the card is full: what windows take, textures
    # lose), frames 21-23 / 22 / 23 a second. -More @{VICTORY_WINDOWS_BAR_MB='0'} or '2048'.
    $settings.VICTORY_WINDOWS_BAR_MB = '1024'
    # The title's 185 MB heap of constants read by the card where the title writes it (renderer
    # port 1 of avatarmanz/VIctory-7900XTX): nothing of it sent up, fetched back or compared. RX
    # 7700 XT, opening cutscene (2026-11-26), three runs with against one without: 27-28 fps
    # against 25, the submitting thread 35.1 ms a frame against 38.9; the picture as before.
    # The other switches of that tree measured here in the same run, on and off by turns
    # (VICTORY_AB): VICTORY_LANE_CORE worse (48.6 ms a frame against 37.4), VICTORY_ASK_NORESET
    # 40.2 against 37.8, VICTORY_ASK_SPREAD, VICTORY_UP_QUEUE and VICTORY_JOIN_BARRIERS no
    # different; VICTORY_PORT_DESCRIPTORS with it 25 fps and slots its patches miss.
    # -More @{VICTORY_PORT_CONSTANTS='0'}: as before.
    $settings.VICTORY_PORT_CONSTANTS = '1'
    # Translated shaders read the title's constant buffers where they are (renderer port 5 of
    # avatarmanz/VIctory-7900XTX: 10,900 tables' constant buffers a frame not copied), and a
    # title buffer of a megabyte and more that shaders write is the card's alone until
    # something on the CPU touches it (crates/victory-gpu/src/host/owner.rs: nothing of it
    # compared or fetched). RX 7700 XT, opening cutscene, the clock at 60 Hz (2026-11-26):
    # neither 30-31 fps, the card working 29.0 ms a frame, the preparing thread 30.3; port 5
    # alone 32-33 fps, 26.8 and 29.5; both 33-34 fps, 27.4 and 26.5, 4.1 GB of the PC's
    # memory free against 2.2; the picture the same. The first run with port 5 makes every
    # pipeline again (minutes of a few frames a second, once).
    # Tried and not usable on this profile: VICTORY_OUTPUT=2560x1440 (the title ends at its
    # start, with or without VICTORY_UPSCALER=6 and VICTORY_RENDER_SCALE=0.5) and
    # VICTORY_FFX_SCALE=0.6 (it ends where the world begins); series-x at VICTORY_OUTPUT=
    # 1920x1080 runs at 19-20 fps (the card 44 ms a frame).
    # -More @{VICTORY_PORT_CBV='0'; VICTORY_CARD_OWNED='0'}: as before.
    $settings.VICTORY_PORT_CBV = '1'
    $settings.VICTORY_CARD_OWNED = '1'
}
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; exit 1 }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; exit 1 }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
$inherited = @(Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $paths })
foreach ($entry in $inherited) { Remove-Item "Env:$($entry.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$discarded = $inherited | Where-Object {
    $_.Name -notin $More.Keys -and -not $_.Value.Equals([Environment]::GetEnvironmentVariable($_.Name))
} | Sort-Object Name
if ($discarded) {
    Write-Host "Discarded inherited values for $($discarded.Name -join ', '). Use -More to pass them explicitly."
}
$Exe = (Resolve-Path $Exe).Path
Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try { & $victory run $Exe --execute --console $Console } finally { Pop-Location }
