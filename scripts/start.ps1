# Menu de démarrage : choisir l'affichage, les graphismes et la cadence, puis lancer le jeu
# avec scripts\play.ps1 (qui pose les réglages de fond du profil).
#   scripts\start.ps1          le menu
#   scripts\start.ps1 -Last    relance directement avec les derniers choix
# (La fenêtre de scripts\lanceur.ps1, ouverte par « Lancer VIctory.cmd », enregistre les mêmes
# choix et lance par ici.)
# Les choix sont gardés hors du dépôt, dans %LOCALAPPDATA%\victory-7900xtx\start.json
# (le chemin de votre copie du jeu n'y entre jamais dans git).
param([switch]$Last)

$store = Join-Path $env:LOCALAPPDATA 'victory-7900xtx\start.json'

# Les réglages, leur valeur de départ.
$s = [ordered]@{
    Exe        = ''
    Console    = 'series-s'
    Clock      = '60'
    Window     = 'windowed'
    Monitor    = '0'
    VSync      = '0'
    FpsLimit   = '0'
    Output     = 'jeu'
    Anisotropy = '16'
    Upscaler   = '6'
    RenderScale = '0.5'
    Sharpness  = '0.5'
    TextureStore = '0'
    Ambient    = '0.8'
    RayTracing = 'bas'
    Hud        = '0'
    Safe       = '0'
    Build      = 'release'
    Extra      = ''
}
if (Test-Path $store) {
    try {
        $saved = Get-Content $store -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($k in @($s.Keys)) {
            if ($null -ne $saved.$k -and "$($saved.$k)" -ne '') { $s[$k] = "$($saved.$k)" }
        }
    } catch { Write-Host "Réglages enregistrés illisibles, valeurs de départ reprises." }
}

# Le jeu : pris de victory.toml (game_dir) si rien n'est enregistré.
if (-not $s.Exe) {
    $toml = Join-Path $PSScriptRoot '..\victory.toml'
    if (Test-Path $toml) {
        $m = Select-String -Path $toml -Pattern "^\s*game_dir\s*=\s*'([^']+)'" | Select-Object -First 1
        if ($m) {
            $found = Get-ChildItem $m.Matches[0].Groups[1].Value -Filter '*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($found) { $s.Exe = $found.FullName }
        }
    }
}

# Chaque ligne du menu : la clé, son nom, les valeurs permises (vide : saisie libre) et l'aide.
$menu = @(
    @{ K = 'Exe'; N = 'Jeu (exécutable de votre copie)'; V = @(); H = 'chemin complet du .exe' }
    @{ K = 'Console'; N = 'Profil console'; V = @('series-s', 'series-x'); H = 'series-s : rendu 720p porté à 1080p, 30-35 img/s sur RX 7700 XT ; series-x : 4K, bien plus lourd (10-20 img/s)' }
    @{ K = 'Clock'; N = 'Cadence du jeu (img/s visées)'; V = @('30', '60'); H = '30 : celle du jeu ; 60 : le jeu ne se bride plus à 30 et rend ce que la machine tient' }
    @{ K = 'Window'; N = 'Fenêtre'; V = @('windowed', 'borderless', 'fullscreen'); H = 'windowed : une fenêtre ajustée à l''écran (celle de toutes les mesures) ; borderless : plein écran sans bordure, alt-tab possible ; fullscreen : la même fenêtre, affichée directement par le système' }
    @{ K = 'Monitor'; N = 'Écran'; V = @(); H = 'numéro de l''écran, 0 pour le premier' }
    @{ K = 'VSync'; N = 'Synchronisation verticale'; V = @('0', '1'); H = '0 : non (conseillé avec un écran à fréquence variable ou AMD Fluid Motion Frames) ; 1 : oui' }
    @{ K = 'FpsLimit'; N = 'Limite d''images par seconde'; V = @(); H = '0 : aucune ; sinon un nombre (par ex. 30 pour une cadence régulière)' }
    @{ K = 'Output'; N = 'Résolution de sortie du jeu'; V = @('jeu', '1280x720', '1664x936', '1920x1080', '2560x1440', '3840x2160'); H = 'jeu : celle du profil (1080p en series-s, 4K en series-x). Autres tailles : expérimental, en series-s le jeu s''arrête au démarrage en 2560x1440' }
    @{ K = 'Anisotropy'; N = 'Filtrage anisotrope'; V = @('0', '2', '4', '8', '16'); H = '0 : ce que le jeu demande ; 16 : sols et murs plus nets de biais, sans coût mesuré' }
    @{ K = 'Upscaler'; N = 'Mise à l''échelle de l''image'; V = @('6', '7'); H = '6 : celle du jeu, image lisse (conseillé) ; 7 : FidelityFX, ici en mode de secours : image en pointillés' }
    @{ K = 'RenderScale'; N = 'Échelle de rendu (series-s)'; V = @('jeu', '0.6', '0.55', '0.5'); H = 'jeu : 2/3 de la sortie (1280x720 pour 1080p) ; 0.5 : 960x540, image plus douce, la carte travaille 3 ms de moins par image (24,9 ms contre 28,2 sur RX 7700 XT). Ne rapporte des images que dans les scènes où la carte seule limite ; series-s et mise à l''échelle du jeu (6) uniquement' }
    @{ K = 'Sharpness'; N = 'Netteté (avec FidelityFX, la 7)'; V = @(); H = 'de 0 (aucune) à 1 ; 0.5 conseillé ; sans effet avec la mise à l''échelle du jeu' }
    @{ K = 'TextureStore'; N = 'Magasin de textures sur disque'; V = @('0', '1'); H = '1 : les textures déjà vues s''affichent tout de suite en pleine définition la fois suivante ; prend environ 2 Go de mémoire vive en plus (à éviter avec 32 Go) et de la place sur le disque (1,5 Go pour trois minutes de jeu, dans %LOCALAPPDATA%\victory-7900xtx\texture-store, à effacer pour repartir de zéro)' }
    @{ K = 'Ambient'; N = 'Lumière ambiante estimée'; V = @(); H = '0 : sortie du jeu telle quelle ; 0.8 conseillé (intérieurs moins noirs sans ray tracing)' }
    @{ K = 'RayTracing'; N = 'Ray tracing du jeu'; V = @('0', 'bas', '1'); H = '0 : sans (lumière ambiante estimée à la place) ; bas : éclairage indirect par rayons calculé une image sur trois à six, 33 img/s dans la première scène jouée et 28-31 dans la cinématique d''ouverture sur RX 7700 XT (conseillé) ; 1 : calculé à chaque image, 16-18 img/s' }
    @{ K = 'Hud'; N = 'Affichage des performances'; V = @('0', '1'); H = '1 : un panneau en surimpression (images par seconde, temps d''image, mémoire carte) ; 0 : rien' }
    @{ K = 'Safe'; N = 'Mode sûr'; V = @('0', '1'); H = '1 : sans les chemins rapides récents, à essayer d''abord si quelque chose ne va pas' }
    @{ K = 'Build'; N = 'Version du programme'; V = @('diag', 'release'); H = 'diag : celle de toutes les mesures (avec diagnostics) ; release : sans diagnostics' }
    @{ K = 'Extra'; N = 'Réglages libres'; V = @(); H = 'NOM=valeur séparés par des virgules, par ex. VICTORY_PANEL=1 ; vide : aucun' }
)

function Show-Menu {
    Clear-Host
    Write-Host 'VIctory - réglages de démarrage' -ForegroundColor Cyan
    Write-Host ''
    for ($i = 0; $i -lt $menu.Count; $i++) {
        $e = $menu[$i]
        $value = $s[$e.K]
        if ($value -eq '') { $value = '(vide)' }
        Write-Host ('{0,3}. {1,-34} {2}' -f ($i + 1), $e.N, $value)
    }
    Write-Host ''
    Write-Host 'Un numéro pour changer un réglage, Entrée pour lancer, Q pour quitter sans lancer.'
}

function Set-Choice($e) {
    Write-Host ''
    Write-Host "$($e.N) : $($e.H)" -ForegroundColor Yellow
    if ($e.V.Count -gt 0) {
        for ($i = 0; $i -lt $e.V.Count; $i++) { Write-Host ('   {0}. {1}' -f ($i + 1), $e.V[$i]) }
        $answer = Read-Host 'Votre choix (numéro, Entrée pour ne rien changer)'
        $n = 0
        if ([int]::TryParse($answer, [ref]$n) -and $n -ge 1 -and $n -le $e.V.Count) { $s[$e.K] = $e.V[$n - 1] }
    } else {
        $answer = Read-Host "Nouvelle valeur (Entrée pour garder « $($s[$e.K]) », un tiret pour vider)"
        if ($answer -eq '-') { $s[$e.K] = '' } elseif ($answer -ne '') { $s[$e.K] = $answer.Trim().Trim('"') }
    }
}

if (-not $Last) {
    while ($true) {
        Show-Menu
        $answer = Read-Host 'Choix'
        if ($answer -eq '') { break }
        if ($answer -match '^[qQ]$') { exit 0 }
        $n = 0
        if ([int]::TryParse($answer, [ref]$n) -and $n -ge 1 -and $n -le $menu.Count) { Set-Choice $menu[$n - 1] }
    }
}

# Vérifications avant de lancer.
if (-not $s.Exe -or -not (Test-Path $s.Exe)) {
    Write-Host "Le jeu n'est pas trouvé : « $($s.Exe) ». Relancez et donnez le chemin de l'exécutable (ligne 1)." -ForegroundColor Red
    exit 1
}
$number = { param($text, $least, $most, $name)
    $v = 0.0
    $ok = [double]::TryParse(($text -replace ',', '.'), [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$v)
    if (-not $ok -or $v -lt $least -or $v -gt $most) { Write-Host "$name : « $text » n'est pas entre $least et $most." -ForegroundColor Red; exit 1 }
    $v.ToString([Globalization.CultureInfo]::InvariantCulture)
}
$s.Sharpness = & $number $s.Sharpness 0 1 'Netteté'
$s.Ambient = & $number $s.Ambient 0 4 'Lumière ambiante'
$s.Monitor = & $number $s.Monitor 0 16 'Écran'
$s.FpsLimit = & $number $s.FpsLimit 0 1000 'Limite d''images'

New-Item -ItemType Directory -Force (Split-Path $store) | Out-Null
$s | ConvertTo-Json | Set-Content $store -Encoding UTF8

# Ce que play.ps1 reçoit en plus de son profil.
$more = @{
    VICTORY_WINDOW_MODE = $s.Window
    VICTORY_MONITOR     = $s.Monitor
    VICTORY_VSYNC       = $s.VSync
    VICTORY_FPS_LIMIT   = $s.FpsLimit
    VICTORY_ANISOTROPY  = $s.Anisotropy
    VICTORY_AMBIENT     = $s.Ambient
}
if ([double]::Parse($s.Sharpness, [Globalization.CultureInfo]::InvariantCulture) -gt 0) { $more.VICTORY_FSR_SHARPNESS = $s.Sharpness } else { $more.VICTORY_FSR_SHARPNESS = '0' }
if ($s.Clock -eq '60') { $more.VICTORY_FRAME_INTERVAL_MS = '16.667' }
if ($s.Output -ne 'jeu') { $more.VICTORY_OUTPUT = $s.Output }
if ($s.Hud -eq '1') { $more.VICTORY_PANEL = '1' }
$more.VICTORY_UPSCALER = $s.Upscaler
foreach ($pair in ($s.Extra -split ',')) {
    $name, $value = $pair.Trim() -split '=', 2
    if ($name -and $null -ne $value) { $more[$name.Trim()] = $value.Trim() }
}

$arguments = @{ Exe = $s.Exe; Console = $s.Console; Build = $s.Build; More = $more }
if ($s.RayTracing -eq '1') { $arguments.RayTracing = $true }
if ($s.RayTracing -eq 'bas') { $arguments.RayTracingLow = $true }
if ($s.Safe -eq '1') { $arguments.Safe = $true }
if ($s.TextureStore -eq '1') { $arguments.TextureStore = $true }
if ($s.RenderScale -ne 'jeu' -and $s.Console -eq 'series-s' -and $s.Upscaler -eq '6') {
    $arguments.RenderScale = [double]::Parse($s.RenderScale, [Globalization.CultureInfo]::InvariantCulture)
}
Write-Host ''
Write-Host "Lancement : profil $($s.Console), cadence $($s.Clock), fenêtre $($s.Window), mise à l'échelle $($s.Upscaler), anisotrope $($s.Anisotropy), ray tracing $($s.RayTracing), magasin de textures $($s.TextureStore)." -ForegroundColor Green
Write-Host '(Dans les menus du profil series-s, l''écran peut rester noir : appuyez sur Entrée ou A et le jeu continue.)'
& (Join-Path $PSScriptRoot 'play.ps1') @arguments
