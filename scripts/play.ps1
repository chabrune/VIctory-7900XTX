# Starts the title with the settings this fork is played with (README.md, "How to run it").
#   scripts\play.ps1 -Exe <path of your own copy's executable>
# -Safe         without the newer, faster ways (the first thing to try when something is wrong)
# -RayTracing   the title's own ray-traced indirect light: lit interiors, about half the frame rate
# -RayTracingLow the same light with the title's trace programs run one frame in three to six, by
#               turns (what they wrote stands in between): 33 frames a second in the first scene
#               that is played on an RX 7700 XT, 28-31 in the opening cutscene
# -RenderScale  series-s: the size the title renders at, as a share of the output's width and
#               height, 0.5 to 0.66 (2/3 without it: 1280x720 for 1920x1080; 0.5: 960x540). A
#               softer picture for less of the card's work (see below).
# -TextureStore keeps the textures' larger levels on disk so that they show at once next time
# -Fsr4K        PC FidelityFX 4.1.2 at 3840x2160 (quality, balanced or performance), series-x only;
#               after Belobug/VIctory-Belobug-AMD cd8dfc0 (README.md says where its DLL comes from)
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
    [switch]$RayTracingLow,
    [ValidateRange(0.5, 0.67)][double]$RenderScale,
    [switch]$TextureStore,
    [ValidateSet('quality', 'balanced', 'performance')][string]$Fsr4K,
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
if ($RayTracing -or $RayTracingLow) {
    $settings.VICTORY_BVH_BUILDER = '1'
    # With ray tracing the title's memory is given to shaders by address (crates/victory-gpu/
    # src/memory.rs), in some twenty arenas of 64 MiB. The title's pool-wide buffers and the
    # arenas are left to the write guards instead of asked of the kernel whole by every
    # submission that traces or builds (480 askings over 1.7 GB in 3.5 ms a frame against 618
    # over 6.7 GB in 8.9), and an arena is nothing but its copy (no memory of its own: the PC
    # had 4.3-4.4 GB free in the first played scene against 2.4-2.8, runs w1 and s50b against
    # g1 and p1, RX 7700 XT with 32 GB, 2026-11-26; the picture the same).
    # -More @{VICTORY_QUIET_GUARD='0'; VICTORY_MEMORY_DIRECT='0'}: as before.
    $settings.VICTORY_QUIET_GUARD = '1'
    $settings.VICTORY_MEMORY_DIRECT = '1'
}
if ($Fsr4K) { $settings.VICTORY_FSR_4K = $Fsr4K.ToLowerInvariant() }
if ($RayTracingLow) {
    # The title's 13 trace programs (those that test rays against its structures) each run
    # in one frame of three to six, by turns, and what they wrote stands in between; the
    # title blends what they give over frames anyway. The number follows the frame's time:
    # three while a frame takes under 29 ms, up to six while it takes over 32.8 (the heavy
    # gameplay scene of the opening: 34.9 ms at three, 31.1 at six, RX 7700 XT).
    # The five that cost the card most are given their turns so that every frame carries
    # about the same (7.9, 6.8 and 4.4 ms of a timing run's 20.4). These are hashes of the
    # title's shaders in the build measured; another build's are not found and its programs
    # take turns in the order they are first used.
    $settings.VICTORY_TRACE_EVERY = '3-6'
    $settings.VICTORY_TRACE_TURNS = 'f22a76fcb967ee59:0,6d7fe7745ab9c6d8:1,b58eb3ca6ee46427:2,3592426e3e9142d8:2,36f04a4ed0ad1d9b:2'
}
if ($TextureStore) {
    # The larger levels of textures seen whole are kept on disk and shown at once the next
    # time, instead of seconds later when the title has streamed them in (2,770 to 4,100
    # textures in the first three minutes of a second run). It grows with what is played
    # (1.5 GB for those three minutes; nothing is ever removed: delete the folder to start
    # over), and keeps 512 MB of it in memory; the title then takes 27.5 GB of the PC's
    # memory against 25.2 (1.5 GB left free of 32).
    $settings.VICTORY_TEXTURE_STORE = Join-Path $env:LOCALAPPDATA 'victory-7900xtx\texture-store'
    $settings.VICTORY_TEXTURE_STORE_KEEP_MB = '512'
}
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
    # 2026-11-26, later: 0. With renderer port 1 and the copies of one page sharing the card's
    # memory the frame rate is the same with and without (34.4-35.3 fps against 34.3-35.8, the
    # card working 27.0 ms a frame against 25.0), and the textures have the room: 29,000 host
    # copies of them alive against 21,700, 26 passes that made none for lack of room against
    # 1,700, the card 9.9 GB in use of a 10.2 GB budget against 10.3 GB.
    $settings.VICTORY_WINDOWS_BAR_MB = '0'
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
    # The title's anisotropic samplers (it asks 2, 4, 8 and 16 samples) take 16: sharper
    # ground and walls seen at an angle, at no frame rate cost here (34 fps, the card 26.6 ms
    # a frame, with and without). -More @{VICTORY_ANISOTROPY='0'}: what the title asks.
    $settings.VICTORY_ANISOTROPY = '16'
    # Renderer ports 6 and 7 of avatarmanz/VIctory-7900XTX: the write guards' lock shared by
    # faults and refreshes, and the heaps' records' notes in parts with locks of their own. RX
    # 7700 XT, opening cutscene, the clock at 60 Hz (2026-11-26), with the two above: the
    # preparing thread 26-27 ms a frame against 28.1, 34.5-35 fps against 34 (the card, 25.7 ms
    # a frame, is what the frame waits for). -More @{VICTORY_PORT_GUARD='0';
    # VICTORY_PORT_JOURNAL='0'}: as before.
    $settings.VICTORY_PORT_GUARD = '1'
    $settings.VICTORY_PORT_JOURNAL = '1'
    # The copies of one page share the card's memory (crates/victory-gpu/src/host/pages.rs): the
    # title shows a page of its memory at several addresses and has buffers at each, and what
    # shaders write through one copy is the others' at once, as on the console, where the block
    # is the card's alone (VICTORY_CARD_OWNED). RX 7700 XT, opening cutscene, the clock at 60 Hz
    # (2026-11-26): 35.3-36.3 fps against 33-34 without, 1,050 pieces compared on the card a
    # frame against 5,400, 11.6 MB fetched against 35, 3.3 MB sent up against 18; no block
    # written through a copy that does not share it (the log's line says how many).
    # -More @{VICTORY_CARD_PAGES='0'}: as before.
    $settings.VICTORY_CARD_PAGES = '1'
    # The title's own upscaler (6) instead of FidelityFX (7), which the title picks on Series S:
    # its FSR 4 pipelines come ready-made for the console's card and cannot be read here
    # ("DeserializeComputePipelineStateX FAILED ... falling back"), and what the fallback gives
    # is a dotted, pixelated picture (hair, shadow edges, anything the title dithers and leaves
    # to the upscaler to smooth). The title's own smooths them; same render size (1280x720 to
    # 1920x1080), same frame rate. -More @{VICTORY_UPSCALER='7'}: FidelityFX, as before.
    # Its render scale (VICTORY_RENDER_SCALE, -RenderScale) can go down from 2/3, not up (0.8
    # ends the title at its start on this profile, as the other output sizes do). At 0.5
    # (960x540) with -RayTracingLow, the first played scene, RX 7700 XT (2026-11-26, runs g1
    # and s50b): the card works 24.9 ms a frame against 28.2, and the frame is the same 29 to
    # 30 ms (33 fps), because the thread that prepares submissions, at 26 ms, is then what it
    # waits for. So it pays only where the card alone sets the frame (the dormitory that
    # follows: 39-43 ms of the card against 41-47), for a softer picture: off unless asked.
    $settings.VICTORY_UPSCALER = '6'
    if ($RenderScale) { $settings.VICTORY_RENDER_SCALE = $RenderScale.ToString([Globalization.CultureInfo]::InvariantCulture) }
    # A list's prelude (the states of its textures, and since the merge of Belobug/VIctory-
    # Belobug-AMD 5d6bd84 the uploads of the title's textures) is a list from a pool on the
    # list's own allocator: 49 to 51 preludes made in a run against 1,400, each of which took
    # 1.1 MiB of the PC's memory for good on an RX 7700 XT (about 1.6 GB; every run of
    # 2026-11-26 from rt-low1 on). -More @{VICTORY_PRELUDE_POOL='0'}: as before.
    $settings.VICTORY_PRELUDE_POOL = '1'
}
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; exit 1 }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; exit 1 }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR', 'VICTORY_FIDELITYFX_DIR'
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
