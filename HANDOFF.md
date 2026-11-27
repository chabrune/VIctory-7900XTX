> **Dépassé depuis le 2026-11-26 au soir.** L'état courant est dans `docs/findings.md` (sections datées du 26) : 30 fps atteints sur `main`, suite du chantier sur la branche `optimisation` (allocateur maison, propriété carte `VICTORY_CARD_OWNED=1`). Ce qui suit est le topo du matin, gardé pour mémoire.

# Passation : VIctory sur RX 7700 XT — objectif 30 fps, image complète

Tu reprends ce dépôt à ma suite. **Ne t'arrête pas tant que l'objectif n'est pas atteint ou qu'un blocage réel n'est pas démontré par une mesure.** Pas de point d'étape qui attend une réponse, pas de « veux-tu que je continue ? » : tu mesures, tu corriges, tu committes, tu passes au point suivant. Si une piste échoue, tu le notes avec le chiffre et tu prends la suivante.

## L'objectif

Le jeu (titre Xbox Series, lancé par cette couche de compatibilité) doit tourner à **30 fps stables** avec une **image complète** (toutes les textures, pas d'écran noir, interface affichée) sur cette machine :

- Radeon RX 7700 XT 12 Go, i5-14600KF (6 cœurs rapides + 8 lents), **32 Go de RAM**, écran 1440p.
- État actuel : **22 à 24 fps** dans la cinématique d'ouverture, profil Series S (rendu 1280x720 agrandi en 1920x1080 par FSR).
- Le jeu vise lui-même 30 fps (intervalle de 33,3 ms) : il suffit de passer sous 33 ms par image pour qu'il s'y cale.

## Le problème de fond (à garder en tête pour chaque décision)

Sur la console, processeur et carte graphique lisent **la même mémoire**. Sur PC la carte a la sienne. La couche garde donc deux copies de presque tout et les resynchronise à chaque image : détecter ce que le jeu a écrit, le monter vers la carte, détecter ce que les shaders ont écrit, le redescendre. Environ 60 Mo montés et 70 Mo redescendus par image.

Tout ce qui coûte découle de là : la RAM pleine (95 %), la mémoire carte pleine (9,7 à 10 Go sur un budget de 10,1), un thread saturé, et une carte qui lisait ses données à travers le bus. Le processeur, lui, n'est qu'à 46 % : il reste des cœurs libres.

## Règles de travail (non négociables)

1. **Mesurer avant d'affirmer.** Un run par mesure, écart entre runs d'environ ±1,5 fps : ne conclus rien sur une différence plus petite.
2. **Un changement = un réglage `VICTORY_...` qui le désactive**, décrit dans un commentaire là où il est lu, avec le chiffre mesuré.
3. **Un commit par étape**, directement sur `main`, message au style du dépôt (ce que ça fait, ce qui a été mesuré, sur quelle machine, à quelle date).
4. **Jamais de fichier du jeu dans le dépôt** (`docs/asset-policy.md`). Dumps et captures vont hors du dépôt.
5. **Une seule instance du jeu à la fois** : deux ne tiennent pas dans 32 Go. Ne tue jamais un `victory.exe` que tu n'as pas lancé toi-même.
6. Ne compile pas pendant qu'un run mesure : ça fausse les chiffres.

## Comment lancer et mesurer

```powershell
cargo build --offline --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe 'C:\Users\chabrune\Downloads\GTA6.Xbox.v1.0.2.1\game_scarlett_master.exe' -Console series-s
```

- `-More @{ NOM = 'valeur' }` ajoute des réglages. `VICTORY_KEYS='6:Enter,10:Enter,14:Enter,18:Enter,22:Enter,26:Enter'` passe les menus tout seul ; le monde commence environ 45 s après le lancement.
- `VICTORY_LOG_FILE=<fichier>` pour un log par run. La ligne `... fps, ... ms/frame (... preparing ...)` sort toutes les 5 s.
- **Comparer dans un même lancement** : `VICTORY_AB='8,VICTORY_MON_REGLAGE'` alterne le réglage toutes les 8 s et logue `A/B phase ...` (il faut que le réglage soit lu par `crate::toggled!`).
- Outils de diagnostic déjà dans le build `diag` :
  - `VICTORY_TIMELINE=<fichier>` (+ `VICTORY_TIMELINE_AT`, `VICTORY_TIMELINE_FRAMES`) : la chaîne exacte d'attentes de quelques images. **Le meilleur outil pour le thread de soumission.**
  - `VICTORY_GPU_TIMING=1` avec `VICTORY_D3D12_DRED=1` : temps carte par commande et par pipeline.
  - `VICTORY_CARD_SPANS=1` : temps de travail réel de la carte par image.
  - `VICTORY_PROFILE=<de>,<à>,<ms>,<fichier>` : profil d'échantillonnage des threads (adresses `module+rva`, à nommer avec le PDB).
  - `VICTORY_CAPTURE=<dossier>` : images présentées, pour vérifier le rendu.
  - `VICTORY_SHADER_SAVE=<dossier>` : chaque shader sous forme console (`.cdxil`) et traduite (`.dxil`) ; `dxc.exe -dumpbin` du SDK Windows les désassemble.
- Tout ce qui a été appris est dans **`docs/findings.md`** (lis-le en premier, sections du 2026-11-26), l'architecture dans `docs/architecture.md`.

## Ce qui est fait (tout est sur `main`, arbre propre)

- Travail de trois sessions fusionné ; builds `diag` et `release` OK ; toute la suite de tests passe.
- **Textures** : la réserve de mémoire carte pour les cibles de rendu dépend du profil (1 536 Mo en Series S au lieu de 3 072). Textures vivantes sur la carte : 15 800 → 38 000 à 50 000.
- **Mémoire carte** : les copies de tampons et les tampons de travail des listes sont découpés dans des blocs de 64 Mo (le pilote arrondit chaque tampon isolé à 256 Ko).
- **Géométrie en mémoire carte écrite directement par le processeur** (`VICTORY_WINDOWS_BAR_MB`, 1 Go par défaut en Series S) : la carte ne la lit plus à travers le bus. Un shader de maillage est passé de 2,5 à 0,4 ms.
- **Logs écrits par un thread séparé** (ils bloquaient l'affichage 17 ms chaque seconde).
- Second plan d'affichage du jeu composé (menus et interface visibles en Series S), netteté FSR, vidage des vues non ordonnées.
- Pipelines précompilés (méthodes 0x1c8 et 0x218) : format décodé et documenté. La création « compute » existe mais est **désactivée** (`VICTORY_PIPELINE_PARTS=1` donne un écran noir : plus de la moitié de ces shaders ne se traduisent pas depuis ce format). Le jeu recrée ces pipelines par la voie normale : **ce n'est pas une cause d'image incomplète, ne perds pas de temps dessus.**

## Mesuré et sans effet (ne refais pas)

- Nombre de threads de traduction : 8 est le bon réglage (6 s'effondre à 4 fps, 12 n'apporte rien). Ces threads attendent plus de la moitié du temps.
- Lancer plus tôt le thread qui recopie les descripteurs (`VICTORY_HEAPS_EARLY`) : 40,9 ms avec et sans.
- File d'objets libérés par thread, parcours des copies touchées seulement : dans le bruit.
- Découpage plus fin des blocs de géométrie (`VICTORY_WINDOW_KB=1024`) : **pire** (19 fps, carte à 39 ms).
- Micro-optimisations du traducteur de shaders : l'auteur précédent les a mesurées, rien. Le code traduit est propre.
- Ray tracing en Series S : 8,9 fps. `VICTORY_LEVELS_ON_DEMAND` : 1 fps. `VICTORY_TEXTURE_POOL=0` : 6 fps.

## État au moment où tu lis ceci (mis à jour après tes derniers commits)

- **La chute à 3 fps n'existe pas avec les réglages actuels** : un run de 290 s tient 25 fps constants, 3,5 Go de RAM libres. L'épisode vu plus tôt venait de deux lancements en même temps. **Un seul jeu à la fois, et un seul outil qui écrit dans ce dossier à la fois.**
- Mesuré sur la partie stable de la cinématique (vers 3 min, 25 fps = 40 ms, la carte travaille 28,3 ms) :

| Réglage | fps | Travail de la carte | Textures vivantes |
|---|---|---|---|
| Défaut (géométrie 1 Go en mémoire carte) | 25 | 28,3 ms | 32 700 |
| `VICTORY_WINDOWS_BAR_MB=2048` | 25 | 28,2 ms | 22 100 |
| `VICTORY_TEXTURE_SHARDS=1` (un seul verrou) | 25 | 29,5 ms | 30 700 |
| `VICTORY_LEGACY_MUL=0` (produit simple) | 26 | 28,0 ms | 33 700 |

  Conclusions : garder 1 Go ; garder les 16 parts ; le produit simple ne vaut pas son inexactitude (0,3 ms).
- **Chronologie de 8 images à 25 fps** (38,9 ms l'image), la chaîne par image : la carte 8,6 ms ; le code du jeu lui-même 6,3 ms ; rafraîchissements 2,6 ; atterrissages 3,2 ; **threads réveillés mais pas encore lancés 2,1 ms** (les cœurs rapides sont surchargés : regarde `VICTORY_FAST_CORES` et le nombre de threads auxiliaires sur un processeur à 6 cœurs rapides) ; **le compteur d'images 2,0 ms** (16 ms d'un coup une fois par seconde, même avec l'écriture des logs déportée : c'est la mise en forme des lignes).
- **Changement en attente dans `host.rs`, non committé, de ma main** : les lignes du compteur d'images sont fabriquées par un thread à part (`meter_lines`, `VICTORY_METER_ASIDE=0` pour l'ancien comportement). Il est dans l'arbre de travail à côté de ton travail en cours sur `sender.rs`. Il compile avec. **Garde-le, mesure-le (une chronologie avant/après : « the frame meter » doit disparaître de la chaîne) et committe-le séparément.**

## Pistes pour atteindre 30 fps, par ordre de rendement attendu

### 1. Le thread de soumission (le mur principal : 33 à 36 ms par image)

Fonction `Host::submit` dans `crates/victory-gpu/src/host.rs` (vers la ligne 5430). Ses postes, par image :

| Poste | ms |
|---|---|
| Recopie de ce que le jeu a modifié (rafraîchissements) | 9 à 12 |
| Copies vers la mémoire carte | 5,8 |
| Attente des soumissions déjà envoyées | 5,6 |
| Attente du thread des descripteurs | 4,9 |
| Octets du jeu remis sous les écritures des shaders | 3,6 |
| Planification des étapes faites sur la carte | 3,4 |
| Interrogation du suivi d'écritures | 3,2 |

Réflexions :

- Tout se passe sous un seul verrou, soumission après soumission (15 par image). **La contrainte d'ordre réelle** : la prise des octets à envoyer (`card_before`, `send::take`) de la soumission N doit précéder les rafraîchissements de N+1. Ce qui vient après (attente des descripteurs, envoi, notation de ce qui est en vol) peut partir sur un second thread en file. Gain plafonné à environ 5 ms : à faire seulement si le reste ne suffit pas.
- **Le thread des descripteurs (`sync_heaps`) passe 40 % de son temps bloqué sur un verrou** : la file des textures à charger (`HostHeap::textures`, `queue_texture`, `note_buffer` dans `descriptor/host.rs`), que les 8 threads de traduction prennent à chaque écriture. Le découper par plages d'indices réduirait l'attente de 4,9 ms. Attention : l'ordre de service des textures (le plus récent d'abord) doit rester le même.
- **Les rafraîchissements** : 5 800 par image dont 4 700 « connus pour ne rien copier ». Cherche pourquoi ils sont encore soumis : peut-on ne pas les mettre dans la liste du tout plutôt que les écarter après coup ?
- **Moins de soumissions** : 15 par image. `VICTORY_JOIN_PARTS` en joint déjà. Mesure combien se joignent et ce qui empêche les autres.
- Prends une chronologie (`VICTORY_TIMELINE`) **avant** de toucher quoi que ce soit, et une après.

### 2. La carte : ce que lisent les calculs (7,5 ms par image, inchangés)

Les maillages sont réglés (données sur la carte). Les calculs (« dispatch ») n'ont pas bougé : le plus lourd, `compute 39f0e9aeb156a61c`, coûte 2,4 ms pour un seul appel ; puis `7a071f906f98311b`, `34fb51b8a0312e82`, `5cf00dc39042edaf`.

- Trouve **quels tampons et textures ils lisent et où ils sont** (mémoire PC partagée ou mémoire carte). S'ils lisent des tampons de moins de 64 Ko restés en mémoire PC, ou des copies à deux faces dont le côté carte est en mémoire partagée, c'est le même problème que la géométrie et la même solution.
- 766 Mo de copies sur la carte sont écrits par des shaders (deux faces obligatoires) ; 173 Mo ne le sont jamais et pourraient passer en écriture directe comme la géométrie.

### 3. Les copies entre textures ajoutées par la passerelle (1,4 ms de carte par image)

`texture::alias` dans `crates/victory-gpu/src/texture.rs` : 77 copies par image via un tampon, pour une texture que le jeu voit sous deux formats. Créer la texture hôte sous un format sans type et deux vues supprimerait la copie.

### 4. L'arbitrage de la mémoire carte (image complète)

12 Go ne contiennent pas tout : textures (≈ 3,5 à 4,3 Go de blocs), cibles (1,4), géométrie (3,4 si tout y est), copies (1,3), base (0,6). Le giga manquant est trouvé (petits tampons à 256 Ko pièce, désormais dans les blocs).

- Fragmentation des blocs de textures : 4,3 Go de blocs pour 3,1 à 3,6 Go tenus. Séparer les petites textures (≤ 64 Ko, alignées sur 4 Ko) des grandes dans des blocs distincts réduirait les trous.
- Ne libère **pas** les blocs vides : rendre de la mémoire encore lue par une liste a déjà fait perdre la carte deux fois.

### 5. La RAM (95 % pleine)

25 Go privés. Les plus gros postes : copies de la mémoire du jeu (4,2 Go, dont 3,4 de géométrie — chaque Go passé en mémoire carte en libère autant), mémoire des listes de commandes (1 Mio par liste côté PC, 2 000 listes), tas du processus (2,5 Go). La pagination provoque des à-coups : surveille « the PC has ... MB free ».

## Ce que « fini » veut dire

- 30 fps de médiane sur **5 minutes** de la cinématique d'ouverture puis du premier passage jouable, sans tranche de 5 s sous 25 fps.
- Captures propres (monde, personnages, textures, interface).
- `cargo test --workspace` vert, builds `diag` et `release` OK.
- `docs/findings.md` à jour avec ce que tu as mesuré, y compris ce qui n'a rien donné.

Tant que ces quatre points ne sont pas tenus, tu continues. Si tu crois avoir atteint une limite du matériel, prouve-le par une mesure (chronologie + temps carte) et écris-le dans `docs/findings.md` avant de t'arrêter.
