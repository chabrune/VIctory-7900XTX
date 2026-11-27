# Lanceur : une fenêtre pour choisir les réglages et lancer le jeu.
#   double-clic sur « Lancer VIctory.cmd » à la racine du dépôt, ou
#   powershell -ExecutionPolicy Bypass -File scripts\lanceur.ps1
# Les choix sont ceux de scripts\start.ps1 (le menu en mode texte), gardés au même endroit,
# hors du dépôt : %LOCALAPPDATA%\victory-7900xtx\start.json. « Lancer » les enregistre et
# démarre scripts\start.ps1 -Last dans une console à part.
#   -Check   construit la fenêtre sans l'afficher (vérification) et rend la main ;
#            avec -Shot <fichier.png>, en écrit aussi l'image.
param([switch]$Check, [string]$Shot = '')

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$store = Join-Path $env:LOCALAPPDATA 'victory-7900xtx\start.json'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# Les réglages et leur valeur de départ : ce qui a donné les meilleurs résultats mesurés
# (RX 7700 XT, 32 Go, docs\findings.md) : profil Series S, ray tracing bas, rendu 960x540
# porté à 1080p par le jeu, cadence libre, version sans diagnostics.
$s = [ordered]@{
    Exe = ''; Console = 'series-s'; Clock = '60'; Window = 'windowed'; Monitor = '0'; VSync = '0'
    FpsLimit = '0'; Output = 'jeu'; Anisotropy = '16'; Upscaler = '6'; RenderScale = 'jeu'
    Sharpness = '0.5'; TextureStore = '0'; Ambient = '0.8'; RayTracing = 'bas'; Hud = '0'
    Safe = '0'; Build = 'release'; Extra = ''
}
if (Test-Path $store) {
    try {
        $saved = Get-Content $store -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($k in @($s.Keys)) {
            if ($null -ne $saved.$k -and "$($saved.$k)" -ne '') { $s[$k] = "$($saved.$k)" }
        }
        # (Un réglage que les anciens fichiers n'ont pas garde sa valeur de départ.)
    } catch { }
}
if (-not $s.Exe) {
    $toml = Join-Path $root 'victory.toml'
    if (Test-Path $toml) {
        $m = Select-String -Path $toml -Pattern "^\s*game_dir\s*=\s*'([^']+)'" | Select-Object -First 1
        if ($m) {
            $found = Get-ChildItem $m.Matches[0].Groups[1].Value -Filter '*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($found) { $s.Exe = $found.FullName }
        }
    }
}

# Les préréglages : ce qu'ils posent, et ce qui a été mesuré avec (première scène jouée).
$presets = [ordered]@{
    'Équilibré (conseillé)' = @{ Set = @{ Console = 'series-s'; RayTracing = 'bas'; RenderScale = 'jeu'; Clock = '60' }
        Note = 'Ray tracing bas, rendu 1280x720 porté à 1080p : 30-34 img/s en jeu, image plus nette.' }
    'Performance (image plus douce)' = @{ Set = @{ Console = 'series-s'; RayTracing = 'bas'; RenderScale = '0.5'; Clock = '60' }
        Note = 'Ray tracing bas, rendu 960x540 porté à 1080p. Le meilleur débit mesuré avec ray tracing : 32-35 img/s en jeu, la carte travaille 3 ms de moins par image que ci-dessous ; image un peu plus douce.' }
    'Sans ray tracing (le plus rapide)' = @{ Set = @{ Console = 'series-s'; RayTracing = '0'; RenderScale = 'jeu'; Clock = '60' }
        Note = 'Lumière ambiante estimée à la place des rayons : 35-36 img/s dans la cinématique d''ouverture, intérieurs moins justes.' }
    'Ray tracing complet' = @{ Set = @{ Console = 'series-s'; RayTracing = '1'; RenderScale = 'jeu'; Clock = '60' }
        Note = 'Rayons calculés à chaque image : 16-18 img/s sur RX 7700 XT.' }
    'Personnalisé' = @{ Set = @{}; Note = 'Vos propres choix ci-dessous.' }
}

$form = New-Object System.Windows.Forms.Form
$form.Text = 'VIctory - lancer le jeu'
$form.StartPosition = 'CenterScreen'
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.ClientSize = New-Object System.Drawing.Size(640, 664)
$form.Font = New-Object System.Drawing.Font('Segoe UI', 9)
$tip = New-Object System.Windows.Forms.ToolTip
$tip.AutoPopDelay = 20000

$y = 14
$controls = @{}
function Add-Label([string]$text, [int]$top) {
    $l = New-Object System.Windows.Forms.Label
    $l.Text = $text; $l.Location = New-Object System.Drawing.Point(16, ($top + 3)); $l.Size = New-Object System.Drawing.Size(210, 20)
    $form.Controls.Add($l)
}
# Une liste déroulante : la clé, le libellé, les couples (valeur, texte montré), l'aide.
function Add-Choice([string]$key, [string]$label, [object[]]$pairs, [string]$help) {
    Add-Label $label $script:y
    $c = New-Object System.Windows.Forms.ComboBox
    $c.DropDownStyle = 'DropDownList'
    $c.Location = New-Object System.Drawing.Point(232, $script:y); $c.Size = New-Object System.Drawing.Size(390, 24)
    foreach ($p in $pairs) { [void]$c.Items.Add($p[1]) }
    $c.Tag = $pairs
    $form.Controls.Add($c)
    if ($help) { $tip.SetToolTip($c, $help) }
    $script:controls[$key] = $c
    $script:y += 30
}
function Add-Text([string]$key, [string]$label, [string]$help, [int]$width = 390) {
    Add-Label $label $script:y
    $t = New-Object System.Windows.Forms.TextBox
    $t.Location = New-Object System.Drawing.Point(232, $script:y); $t.Size = New-Object System.Drawing.Size($width, 24)
    $form.Controls.Add($t)
    if ($help) { $tip.SetToolTip($t, $help) }
    $script:controls[$key] = $t
    $script:y += 30
}
function Set-Shown([string]$key, [string]$value) {
    $c = $controls[$key]
    if ($c -is [System.Windows.Forms.ComboBox]) {
        $i = 0
        for ($n = 0; $n -lt $c.Tag.Count; $n++) { if ($c.Tag[$n][0] -eq $value) { $i = $n } }
        $c.SelectedIndex = $i
    } else { $c.Text = $value }
}
function Get-Shown([string]$key) {
    $c = $controls[$key]
    if ($c -is [System.Windows.Forms.ComboBox]) { return [string]$c.Tag[[Math]::Max(0, $c.SelectedIndex)][0] }
    return $c.Text.Trim().Trim('"')
}

# Le préréglage, et ce qu'il donne.
Add-Label 'Préréglage' $y
$preset = New-Object System.Windows.Forms.ComboBox
$preset.DropDownStyle = 'DropDownList'
$preset.Location = New-Object System.Drawing.Point(232, $y); $preset.Size = New-Object System.Drawing.Size(390, 24)
foreach ($name in $presets.Keys) { [void]$preset.Items.Add($name) }
$form.Controls.Add($preset)
$y += 30
$note = New-Object System.Windows.Forms.Label
$note.Location = New-Object System.Drawing.Point(16, $y); $note.Size = New-Object System.Drawing.Size(606, 50)
$note.ForeColor = [System.Drawing.Color]::FromArgb(40, 90, 150)
$form.Controls.Add($note)
$y += 56

Add-Choice 'RayTracing' 'Ray tracing du jeu' @(@('0', 'Sans (lumière ambiante estimée)'), @('bas', 'Bas : rayons calculés une image sur 3 à 6 (conseillé)'), @('1', 'Complet : à chaque image (lent)')) 'Éclairage indirect par rayons du jeu. Bas : ce que les programmes de rayons ont écrit reste en place entre deux passages, le jeu le lisse de toute façon.'
Add-Choice 'RenderScale' 'Échelle de rendu' @(@('0.5', '0.5 : 960x540, le plus rapide'), @('0.55', '0.55'), @('0.6', '0.6'), @('jeu', '2/3 : 1280x720, le plus net')) 'Taille à laquelle le jeu calcule l''image avant de la porter à 1920x1080 (profil Series S, mise à l''échelle du jeu).'
Add-Choice 'Clock' 'Cadence visée par le jeu' @(@('60', '60 : le jeu rend ce que la machine tient'), @('30', '30 : bridé à 30 img/s, comme sur console')) ''
Add-Choice 'Window' 'Fenêtre' @(@('windowed', 'Fenêtre ajustée à l''écran'), @('borderless', 'Plein écran sans bordure'), @('fullscreen', 'Plein écran')) ''
Add-Choice 'VSync' 'Synchronisation verticale' @(@('0', 'Non (conseillé avec FreeSync ou AMD Fluid Motion Frames)'), @('1', 'Oui')) ''
Add-Text 'FpsLimit' 'Limite d''images par seconde' '0 : aucune ; sinon un nombre, par exemple 30 pour une cadence régulière.' 80
Add-Choice 'Anisotropy' 'Filtrage anisotrope' @(@('16', '16 : sols et murs nets de biais (sans coût mesuré)'), @('8', '8'), @('4', '4'), @('2', '2'), @('0', 'Ce que le jeu demande')) ''
Add-Choice 'Hud' 'Affichage des performances' @(@('0', 'Non'), @('1', 'Oui : images par seconde, temps d''image, mémoire (F3 le montre ou le cache)')) ''
Add-Choice 'TextureStore' 'Magasin de textures sur disque' @(@('0', 'Non (conseillé avec 32 Go de mémoire vive)'), @('1', 'Oui : textures déjà vues affichées tout de suite, 2 Go de mémoire en plus')) ''
Add-Choice 'Console' 'Profil console' @(@('series-s', 'Series S : sortie 1080p (conseillé)'), @('series-x', 'Series X : 4K, bien plus lourd (10-20 img/s)')) ''
Add-Choice 'Build' 'Version du programme' @(@('release', 'release : la plus légère (conseillé pour jouer)'), @('diag', 'diag : avec diagnostics (celle des mesures)')) ''
Add-Choice 'Safe' 'Mode sûr' @(@('0', 'Non'), @('1', 'Oui : sans les chemins rapides récents, à essayer si quelque chose ne va pas')) ''
Add-Text 'Ambient' 'Lumière ambiante (sans ray tracing)' '0 : sortie du jeu telle quelle ; 0.8 conseillé.' 80
Add-Text 'Extra' 'Réglages libres' 'NOM=valeur séparés par des virgules, par exemple VICTORY_SUBMIT_AFTER=1 ; vide : aucun.'

# L'exécutable du jeu.
Add-Label 'Jeu (exécutable de votre copie)' $y
$exe = New-Object System.Windows.Forms.TextBox
$exe.Location = New-Object System.Drawing.Point(232, $y); $exe.Size = New-Object System.Drawing.Size(300, 24)
$form.Controls.Add($exe)
$controls['Exe'] = $exe
$browse = New-Object System.Windows.Forms.Button
$browse.Text = 'Parcourir'; $browse.Location = New-Object System.Drawing.Point(540, ($y - 1)); $browse.Size = New-Object System.Drawing.Size(82, 25)
$browse.Add_Click({
    $d = New-Object System.Windows.Forms.OpenFileDialog
    $d.Filter = 'Exécutable (*.exe)|*.exe'
    if ($exe.Text -and (Test-Path (Split-Path $exe.Text -ErrorAction SilentlyContinue))) { $d.InitialDirectory = Split-Path $exe.Text }
    if ($d.ShowDialog() -eq 'OK') { $exe.Text = $d.FileName }
})
$form.Controls.Add($browse)
$y += 38

$status = New-Object System.Windows.Forms.Label
$status.Location = New-Object System.Drawing.Point(16, $y); $status.Size = New-Object System.Drawing.Size(606, 36)
$status.ForeColor = [System.Drawing.Color]::FromArgb(160, 60, 40)
$form.Controls.Add($status)
$y += 40

$launch = New-Object System.Windows.Forms.Button
$launch.Text = 'Lancer le jeu'; $launch.Location = New-Object System.Drawing.Point(232, $y); $launch.Size = New-Object System.Drawing.Size(180, 36)
$launch.Font = New-Object System.Drawing.Font('Segoe UI', 10, [System.Drawing.FontStyle]::Bold)
$form.Controls.Add($launch)
$save = New-Object System.Windows.Forms.Button
$save.Text = 'Enregistrer'; $save.Location = New-Object System.Drawing.Point(422, $y); $save.Size = New-Object System.Drawing.Size(96, 36)
$form.Controls.Add($save)
$close = New-Object System.Windows.Forms.Button
$close.Text = 'Fermer'; $close.Location = New-Object System.Drawing.Point(526, $y); $close.Size = New-Object System.Drawing.Size(96, 36)
$close.Add_Click({ $form.Close() })
$form.Controls.Add($close)
$shortcut = New-Object System.Windows.Forms.Button
$shortcut.Text = 'Raccourci Bureau'; $shortcut.Location = New-Object System.Drawing.Point(16, $y); $shortcut.Size = New-Object System.Drawing.Size(116, 36)
$form.Controls.Add($shortcut)
$prepare = New-Object System.Windows.Forms.Button
$prepare.Text = 'Préparer'; $prepare.Location = New-Object System.Drawing.Point(138, $y); $prepare.Size = New-Object System.Drawing.Size(86, 36)
$form.Controls.Add($prepare)
$tips = New-Object System.Windows.Forms.ToolTip
$tips.AutoPopDelay = 20000
$tips.SetToolTip($prepare, "À faire une fois après une mise à jour du programme ou du pilote graphique :`nprépare les shaders déjà vus, pour que la première partie ne saccade pas.`nQuelques secondes si le pilote les a déjà, quelques minutes sinon. Rien d'autre n'est à faire après une compilation.")
$tips.SetToolTip($shortcut, 'Met un raccourci « VIctory » vers cette fenêtre sur le Bureau.')
$form.AcceptButton = $launch

# Ce que la fenêtre montre : les choix enregistrés, et le préréglage qui leur correspond.
foreach ($k in @($controls.Keys)) { Set-Shown $k $s[$k] }
function Find-Preset {
    foreach ($name in $presets.Keys) {
        $set = $presets[$name].Set
        if ($set.Count -eq 0) { continue }
        $same = $true
        foreach ($k in $set.Keys) { if ((Get-Shown $k) -ne $set[$k]) { $same = $false } }
        if ($same) { return $name }
    }
    'Personnalisé'
}
$applying = $false
function Show-Preset {
    $script:applying = $true
    $name = Find-Preset
    $preset.SelectedItem = $name
    $note.Text = $presets[$name].Note
    $script:applying = $false
}
$preset.Add_SelectedIndexChanged({
    if ($script:applying) { return }
    $chosen = $presets[[string]$preset.SelectedItem]
    $script:applying = $true
    foreach ($k in $chosen.Set.Keys) { Set-Shown $k $chosen.Set[$k] }
    $note.Text = $chosen.Note
    $script:applying = $false
})
foreach ($k in 'RayTracing', 'RenderScale', 'Clock', 'Console') {
    $controls[$k].Add_SelectedIndexChanged({ if (-not $script:applying) { Show-Preset } })
}
Show-Preset

# Les choix de la fenêtre, vérifiés ; $null (et la raison affichée) s'ils ne vont pas.
function Read-Choices {
    $c = [ordered]@{}
    foreach ($k in $s.Keys) { $c[$k] = if ($controls.ContainsKey($k)) { Get-Shown $k } else { $s[$k] } }
    if (-not $c.Exe -or -not (Test-Path $c.Exe)) { $status.Text = "Le jeu n'est pas trouvé : donnez le chemin de l'exécutable de votre copie (bouton Parcourir)."; return $null }
    foreach ($check in @(@('FpsLimit', 0, 1000, 'Limite d''images'), @('Ambient', 0, 4, 'Lumière ambiante'))) {
        $v = 0.0
        $ok = [double]::TryParse(($c[$check[0]] -replace ',', '.'), [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$v)
        if (-not $ok -or $v -lt $check[1] -or $v -gt $check[2]) { $status.Text = "$($check[3]) : « $($c[$check[0]]) » n'est pas entre $($check[1]) et $($check[2])."; return $null }
        $c[$check[0]] = $v.ToString([Globalization.CultureInfo]::InvariantCulture)
    }
    $built = Join-Path $root "target\$($c.Build)\victory.exe"
    if (-not (Test-Path $built)) { $status.Text = "La version « $($c.Build) » n'est pas compilée ($built). Choisissez l'autre, ou compilez-la (README.md)."; return $null }
    $c
}
function Save-Choices($c) {
    New-Item -ItemType Directory -Force (Split-Path $store) | Out-Null
    $c | ConvertTo-Json | Set-Content $store -Encoding UTF8
}
$save.Add_Click({
    $c = Read-Choices
    if ($c) { Save-Choices $c; $status.ForeColor = [System.Drawing.Color]::FromArgb(40, 120, 60); $status.Text = 'Réglages enregistrés.' }
})
$launch.Add_Click({
    $status.ForeColor = [System.Drawing.Color]::FromArgb(160, 60, 40)
    $c = Read-Choices
    if (-not $c) { return }
    if (Get-Process victory -ErrorAction SilentlyContinue) { $status.Text = 'Le jeu tourne déjà (un seul à la fois tient en mémoire) : fermez-le d''abord.'; return }
    Save-Choices $c
    $start = Join-Path $PSScriptRoot 'start.ps1'
    Start-Process powershell.exe -WorkingDirectory $root -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$start`"", '-Last')
    $form.Close()
})
$prepare.Add_Click({
    $status.ForeColor = [System.Drawing.Color]::FromArgb(160, 60, 40)
    $c = Read-Choices
    if (-not $c) { return }
    if (Get-Process victory -ErrorAction SilentlyContinue) { $status.Text = 'Le jeu tourne : fermez-le avant de préparer.'; return }
    Save-Choices $c
    $start = Join-Path $PSScriptRoot 'start.ps1'
    Start-Process powershell.exe -WorkingDirectory $root -ArgumentList @('-NoProfile', '-NoExit', '-ExecutionPolicy', 'Bypass', '-File', "`"$start`"", '-Last', '-Warm')
    $status.ForeColor = [System.Drawing.Color]::FromArgb(40, 120, 60)
    $status.Text = 'Préparation lancée dans une autre fenêtre : attendez « Préparation terminée » avant de lancer le jeu.'
})
$shortcut.Add_Click({
    try {
        $desktop = [Environment]::GetFolderPath('Desktop')
        $link = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $desktop 'VIctory.lnk'))
        $link.TargetPath = Join-Path $root 'Lancer VIctory.cmd'
        $link.WorkingDirectory = $root
        $link.Description = 'VIctory : choisir les réglages et lancer le jeu'
        $link.Save()
        $status.ForeColor = [System.Drawing.Color]::FromArgb(40, 120, 60)
        $status.Text = 'Raccourci « VIctory » créé sur le Bureau.'
    } catch { $status.Text = "Le raccourci n'a pas pu être créé : $_" }
})

if ($Check) {
    # (Vérification : tout est construit, rien n'est affiché.)
    $c = Read-Choices
    "fenêtre construite : $($form.Controls.Count) éléments, préréglage « $($preset.SelectedItem) »"
    if ($c) { ($c.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' ; ' } else { "choix refusés : $($status.Text)" }
    if ($Shot) {
        # (Sans l'afficher : les poignées sont créées, la fenêtre se dessine dans une image.)
        $null = $form.Handle
        foreach ($child in $form.Controls) { $null = $child.Handle }
        $picture = New-Object System.Drawing.Bitmap($form.Width, $form.Height)
        $form.DrawToBitmap($picture, (New-Object System.Drawing.Rectangle(0, 0, $form.Width, $form.Height)))
        $picture.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
        "image : $Shot"
    }
    $form.Dispose()
    exit 0
}
[void]$form.ShowDialog()
