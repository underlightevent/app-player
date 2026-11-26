# Décisions techniques

Choix non prévus par le cahier des charges, ou précisions, avec leur raison.

## Phase 0

### D-001 · Versions des outils
Tauri 2.12, React 19, Vite 8, i18next 27, lucide-react 1.81. TypeScript **5.9** plutôt que la 7.0 toute récente : la 5.9 est la plus éprouvée avec Vite et Vitest ; passer à la 7 plus tard ne demandera pas de changer le code.

### D-002 · Identifiants
- Nom technique : `dj-library-manager` (paquets npm et Rust, exécutable).
- Nom affiché : « DJ Library Manager ».
- Identifiant système : `com.dj-library-manager.desktop`. Il fixe l'emplacement des données de l'application : le changer plus tard déplacerait la base (prévoir une migration ce jour-là).

### D-003 · Migrations de la base
Écrites à la main dans `src-tauri/src/db.rs`, numérotées, version rangée dans `PRAGMA user_version`. Pas de bibliothèque de migrations : il y a peu de besoins et cela évite une dépendance.
- Chaque migration tourne dans sa propre transaction : une coupure au milieu laisse la base à l'étape précédente.
- Avant de migrer une base existante, une copie `library.before-vN.sqlite` est faite avec `VACUUM INTO`.
- Une base créée par une version plus récente de l'application est refusée (jamais modifiée).
- Mode WAL pour que les lectures ne bloquent pas les écritures.
- La sauvegarde quotidienne (7 copies) et l'export/import de la base arrivent en Phase 1, avec les premières vraies données.

### D-004 · Démarrage sans base
Si la base ne peut pas s'ouvrir, l'application démarre quand même et affiche un bandeau d'erreur en français, plutôt que de se fermer sans explication.

### D-005 · Journal local
Extension officielle `tauri-plugin-log` : fichier `dj-library-manager.log` dans le dossier de journaux du système, 2 Mo maximum par fichier, 5 anciens fichiers gardés. Les erreurs de l'interface et les plantages y sont aussi écrits. Le « rapport de diagnostic » reprend les 200 dernières lignes.

### D-006 · Réglages validés par le cœur
Le cœur n'accepte que des clés et des valeurs connues (`theme` : light/dark/system ; `language` : fr/en). Le thème est aussi gardé dans le stockage local de l'interface, uniquement pour éviter un éclair blanc au démarrage ; la base reste la référence.

### D-007 · Barre de titre
- macOS : barre de titre intégrée (`titleBarStyle: Overlay`), boutons natifs à gauche, au-dessus de la barre latérale.
- Windows : barre de titre native conservée pour l'instant. Tauri ne sait pas encore afficher les boutons natifs Windows par-dessus le contenu ; les redessiner nous-mêmes imiterait mal le système (accrochage des fenêtres de Windows 11, etc.). À revoir en Phase 4.
- Taille et position de la fenêtre mémorisées par l'extension officielle `tauri-plugin-window-state`.

### D-008 · Raccourcis du navigateur intégré
Désactivés : F5, Ctrl/Cmd+R (rechargement), Ctrl/Cmd + / − / 0 et Ctrl+molette (zoom), Ctrl/Cmd+P, S, U, G, J, Alt+flèches. Le zoom par raccourci est aussi coupé dans la configuration de la fenêtre. Menu contextuel du navigateur désactivé en version finale.

### D-009 · Une seule chaîne de compilation
Un seul fichier `.github/workflows/build.yml` : tests et installateurs à chaque modification, et publication dans Releases quand une étiquette `vX.Y.Z` est poussée. Installateurs : `.exe` (NSIS, installation pour l'utilisateur courant, sans droits administrateur) et `.msi` pour Windows ; `.dmg` universel pour macOS 12+.

### D-010 · Signature
- macOS : signature ad hoc (`signingIdentity: "-"`) tant qu'il n'y a pas de Developer ID.
- Signature officielle préparée dans la CI, activée automatiquement dès que les secrets existent (voir `docs/SIGNATURE.md`).

### D-011 · Contrôle des licences
Deux scripts vérifient à chaque compilation les licences de toutes les dépendances : `scripts/check-licenses.mjs` (JavaScript livré) et `scripts/check-rust-licenses.mjs` (toutes les crates Rust, toutes plateformes). La compilation échoue si une licence non permise apparaît.
Licences acceptées en plus de la liste du principe 12, toutes permissives et compatibles avec un partage de l'application : **MIT-0** et **0BSD** (variantes de MIT/BSD sans obligation de citation), **Apache-2.0 WITH LLVM-exception** (Apache avec une exception plus permissive), **Unicode-3.0** (données Unicode utilisées par la gestion des adresses web). SQLite, intégré à l'application, est dans le **domaine public**.

### D-012 · Pas de base lue en dehors du dossier de données
La base vit dans le dossier de données de l'application (`%APPDATA%\com.dj-library-manager.desktop\` sous Windows, `~/Library/Application Support/com.dj-library-manager.desktop/` sous macOS), jamais sur le disque de musique.

## Phase 1

### D-101 · Écriture WAV et AIFF au niveau des blocs
lofty ignore les entrées RIFF INFO qui ne sont pas en UTF-8 (fréquent pour des WAV tagués sous Windows) : les réécrire les ferait disparaître, contraire au principe 7. Pour WAV et AIFF, l'application relit et réécrit elle-même les blocs du fichier (`src-tauri/src/tags/chunks.rs`) : chaque bloc est recopié octet pour octet, seuls les blocs `LIST/INFO` et `id3` sont remplacés. Le bloc audio (`data`, `SSND`) n'est jamais modifié. Lecture des textes RIFF INFO : UTF-8 s'il est valide, sinon Windows-1252. Les WAV de plus de 4 Go (RF64) sont refusés à l'écriture.

### D-102 · Hash audio calculé après l'indexation
Le hash du flux audio oblige à lire le fichier entier : sur un disque USB, 10 000 WAV représentent plusieurs centaines de Go, impossible en 5 minutes. L'indexation ne lit donc que les en-têtes et les tags (rapide) ; les hash sont calculés ensuite, en tâche de fond, un fichier à la fois, avec progression et Annuler. Un fichier modifié (taille ou date) voit son hash recalculé. Le hash porte sur les paquets audio démultiplexés par Symphonia (sans décodage) : tags, pochettes et autres blocs n'y entrent pas.

### D-103 · Tableau virtualisé sans TanStack Table
Le tableau utilise TanStack Virtual (lignes virtualisées) avec une grille CSS écrite pour l'application, sans TanStack Table : le tri, les filtres et la recherche sont faits par la base SQLite (rapide sur 100 000 morceaux), et la grille doit seulement afficher, redimensionner et réordonner les colonnes. Le tableau ne charge que la liste ordonnée des identifiants, puis les lignes visibles par paquets de 400.

### D-104 · Préférences d'affichage du tableau
Largeur, ordre et visibilité des colonnes sont gardés dans le stockage local de l'interface : ce sont des préférences d'affichage, sans conséquence sur les fichiers. Les réglages qui comptent (thème, langue, densité, notation des tonalités) restent dans la base.

### D-105 · Identification des disques
Windows : numéro de série du volume (`GetVolumeInformationW`), qui ne change pas avec la lettre de lecteur. macOS : `VolumeUUID` donné par `diskutil`. Les chemins des morceaux sont enregistrés par rapport à la racine du disque. Les disques sont revérifiés toutes les 4 secondes ; un disque rebranché déclenche une réindexation incrémentale de ses dossiers. Les fichiers « ._ » créés par macOS sur les disques exFAT, les fichiers cachés et le dossier `_Quarantaine` sont ignorés.

### D-106 · Fichiers disparus
Un fichier absent lors d'une indexation complète est marqué « manquant », jamais effacé de la base (il garde ses informations et ses champs verrouillés). S'il réapparaît, il redevient normal. Si le parcours d'un dossier rencontre une erreur d'accès, rien n'est marqué manquant.

### D-107 · Recherche
Index plein texte FTS5 (titre, version, artistes, album, genre, nom de fichier), insensible aux accents. Chaque mot tapé est un début de mot, tous doivent être présents. Mesuré : quelques millisecondes sur 100 000 morceaux (test automatique).

### D-108 · Manifeste Windows
Le composant de boîte de dialogue « Choisir un dossier » exige un manifeste Windows (contrôles communs v6). Il est intégré par l'éditeur de liens à l'application et aux programmes de test (`src-tauri/windows-app-manifest.xml`), sinon les tests refusent de démarrer sous Windows.

### D-109 · Journal
Les bibliothèques audio (Symphonia, lofty) écrivent une ligne par fichier lu : seules leurs erreurs sont gardées dans le journal.

### D-110 · Variables de développement
Dans les versions de développement uniquement (jamais dans les installateurs) : `DJLM_DEV_DATA_DIR` utilise une base séparée et `DJLM_DEV_ADD_ROOT` ajoute un dossier au démarrage, pour les essais automatiques sans toucher aux données de l'utilisateur.

### D-111 · Fichiers audio de test
Les fichiers de test sont fabriqués par le code (`src-tauri/src/testdata.rs`) : WAV, AIFF et FLAC contiennent une onde synthétique ; le MP3 est une suite de trames valides de silence (aucun encodeur MP3 sous licence permise n'est disponible en Rust). Aucun fichier audio n'est stocké dans le dépôt.

### D-112 · Licences de la surveillance des dossiers (à confirmer)
`notify`, recommandé par le cahier des charges pour surveiller les dossiers, est sous licence **CC0-1.0** (domaine public) et utilise `xxhash-rust`, sous licence **BSL-1.0** (Boost). Ces deux licences ne figurent pas mot pour mot dans la liste du principe 12, mais elles sont permissives : aucune obligation de publier le code, partage et usage commercial permis, rien de comparable à la GPL. Elles sont donc acceptées par le contrôle automatique des licences. Si l'utilisateur préfère s'en tenir strictement à la liste, la solution de repli est une vérification périodique des dossiers (sans surveillance en temps réel), au prix d'une détection des changements moins rapide.

### D-113 · Déroulé d'une écriture de tags
1. Hash du flux audio de l'original. 2. Copie temporaire cachée (`.djlm-tmp-…`) dans le même dossier, modifiée : seuls les champs demandés sont touchés. 3. Relecture complète de la copie : chaque champ modifié doit être relu à l'identique, et le hash audio doit être le même. 4. Remplacement de l'original par la copie (`rename`, atomique sur le même disque). À la moindre erreur, la copie est supprimée et l'original n'a pas bougé. Les tags existants sont lus en mode strict : s'ils ne se relisent pas parfaitement, l'application refuse d'écrire plutôt que de risquer une perte. Une erreur interne de la bibliothèque de tags est interceptée et transformée en message, sans plantage.
Optimisation non appliquée : le hash de l'original est recalculé à chaque écriture (lecture complète du fichier). Pour un WAV de 100 Mo sur USB 3, compter environ une seconde par morceau. C'est le prix de la garantie « flux audio identique ».

### D-114 · Correspondance des champs par format
- ID3 (MP3 et WAV en v2.3, AIFF en v2.4), texte en UTF-16 : TIT2 (titre composé « Titre (Version) »), TIT3 (version), TPE1 (artistes), TPE4 (remixeur déduit de la version), TALB, TCON, année (TYER en v2.3, TDRC en v2.4), TBPM, TKEY, POPM (adresse « Windows Media Player 9 Series », échelle 1/64/128/196/255 ; les notes des autres logiciels sont conservées), APIC « Cover (front) ».
- RIFF INFO (WAV), écrit en même temps que l'ID3 : INAM, IART, IPRD, IGNR, ICRD. Encodage : UTF-8. Les autres entrées RIFF INFO (ICMT, ISFT…) sont recopiées octet pour octet. **À vérifier par l'utilisateur dans Rekordbox** avec des accents (é, ü, ñ) : si Rekordbox affiche mal les accents, passer à Windows-1252 (une ligne à changer).
- Vorbis (FLAC, OGG, Opus) : TITLE, SUBTITLE (version), REMIXER, ARTIST, ALBUM, GENRE, DATE, BPM, INITIALKEY, RATING (0–100, soit 20 par étoile), image « Cover (front) ».
- M4A : atomes iTunes via lofty. La note n'est pas écrite dans les M4A (aucun usage commun) : elle reste dans la base de l'application et partira vers Rekordbox par l'export XML (phase 4).
- Tonalité écrite dans la notation choisie dans les réglages (Camelot par défaut), BPM arrondi à l'entier par défaut.

### D-115 · Historique
Chaque action forme un « lot » (table `history_batches`) ; chaque fichier touché y ajoute l'état avant et après des seuls champs concernés, avec les cadenas. Les pochettes remplacées sont gardées dans la base (`history_blobs`) pour être remises à l'identique. Annuler ou rétablir réécrit les fichiers avec la même écriture sûre. Une nouvelle action efface la possibilité de rétablir les actions annulées (comportement habituel). Seuls les 2 000 derniers lots sont conservés.

### D-116 · Cadenas et liste d'exceptions
Une saisie manuelle verrouille les champs modifiés (pas la pochette). Les modifications automatiques (phase 2) passent par le même chemin avec la source « automatique », qui ignore les champs verrouillés. Quand l'utilisateur garde entier un nom d'artiste contenant un séparateur (« Duo & Compagnie »), ce nom est ajouté à la liste d'exceptions.

### D-117 · Pochettes en phase 1
Image décodée (JPEG, PNG, WebP, GIF, BMP), recadrée au carré si l'écart est de 10 % au plus, réduite à 1000 pixels (réglable), jamais agrandie, enregistrée en JPEG qualité 85 (abaissée si le fichier dépasse 1 Mo). Une image nettement rectangulaire est gardée entière : l'éditeur de recadrage arrive en phase 2.

### D-118 · Renommage
Modèle par défaut `{artiste_principal} - {titre}[ ({version})]`. Les variables portent des noms français (`{annee}`, `{tonalite}`…). L'aperçu est recalculé par le cœur au moment d'appliquer : l'application ne fait jamais confiance à un aperçu ancien. Un fichier n'est **jamais écrasé** : un nom déjà pris est signalé (option « ajouter (2) » ou « ne pas renommer »). Les échanges de noms (A ↔ B) passent par un nom temporaire caché. Comparaison des noms insensible à la casse et aux accents décomposés (NFC). Table de remplacement des caractères interdits : `:` → « - », `/` et `\` et `|` → « - », `"` → « ' », `?` `*` `<` `>` retirés (réglable plus tard, pas d'écran en phase 1).

### D-119 · Nettoyage et lecture des noms de fichier
Ces actions sont « semi-automatiques » : elles passent toujours par un aperçu, respectent les champs verrouillés et ne verrouillent rien. Les règles et le modèle choisis sont retenus dans les réglages.

### D-120 · Lecteur
Décodage par Symphonia (déjà utilisé pour les hash), sortie son par rodio/cpal, dans un fil dédié ; l'interface reçoit l'état 5 fois par seconde. La sortie son n'est ouverte qu'à la première lecture. Forme d'onde : 1 000 colonnes calculées à la première écoute puis gardées en base (invalidées si la taille ou la date du fichier change). Le calcul dès l'indexation, prévu en option par le cahier des charges, n'est pas encore proposé (voir IDEES).
**Écriture pendant la lecture** : le cahier des charges prévoyait de différer l'écriture. Test fait sous Windows (et vrai sous macOS par construction) : le remplacement atomique réussit même quand le lecteur a le fichier ouvert, et la lecture continue sans coupure sur l'ancienne version. Aucun report n'est donc nécessaire ; si un autre programme verrouille le fichier, l'erreur « fichier utilisé par un autre programme » s'affiche et l'original reste intact.

### D-121 · Glisser-déposer interne
Sous Windows, Tauri intercepte le glisser-déposer du système pour recevoir les fichiers venus de l'Explorateur, ce qui désactive le glisser-déposer HTML5 dans la fenêtre. Les déplacements internes (morceaux vers un dossier, ordre des colonnes, ordre des artistes) utilisent donc un petit module maison basé sur la souris (`src/core/dnd.ts`). Les fichiers venus de l'Explorateur ou du Finder restent gérés par Tauri : images → pochette de la sélection, dossiers → ajoutés à la bibliothèque.

### D-122 · Dossiers
L'arborescence est lue sur le disque quand il est branché, sinon reconstituée depuis la base. Créer, renommer et déplacer un dossier, ou y déplacer des morceaux, est inscrit dans l'historique et s'annule. Annuler une création ne supprime qu'un dossier resté vide. Le déplacement entre deux disques différents n'est pas proposé en phase 1 (il faudrait copier puis supprimer). La « fusion » de dossiers se fait en glissant les morceaux d'un dossier vers l'autre.

### D-123 · Sauvegardes de la base
Une copie par jour au démarrage (`backups/library-AAAA-MM-JJ.sqlite`, 7 gardées). Export par `VACUUM INTO` vers le fichier choisi. Import : le fichier est vérifié (base de l'application, version compatible), puis mis en place **au démarrage suivant**, après une copie de la base actuelle (`backups/avant-import-….sqlite`) — remplacer une base ouverte serait risqué.

### D-124 · Performance du tableau
Les lignes chargées sont gardées dans une table jamais recopiée ; chaque ligne est un composant mémorisé ; étoiles et vignette de pochette sont dessinées en texte et CSS plutôt qu'en icônes. Mesure sur 100 000 morceaux simulés : défilement à la molette à 60 images par seconde (16,7 ms par image) ; traversée complète de la liste en 4 secondes avec la barre de défilement : environ 30 images par seconde.

### D-125 · Édition dans le tableau
Le cahier des charges prévoit l'édition sur place dans les cellules (clic ou F2). En phase 1, F2 ouvre le panneau de détail sur le champ Titre du morceau sélectionné ; Tab passe au champ suivant, Entrée valide, Échap annule. L'édition directement dans la cellule est notée dans IDEES.

## Phase 2

### D-201 · Sources activées par défaut
MusicBrainz, Cover Art Archive et AcoustID sont activés ; **Deezer et iTunes sont désactivés** : leurs conditions interdisent ce que l'application en ferait (copie locale du contenu pour Deezer, pochettes hors promotion de l'iTunes Store pour Apple). Détail et sources dans `docs/SOURCES.md`. Aucune réponse Deezer n'est gardée en cache, aucune pochette Deezer n'est gardée sur l'ordinateur.

### D-202 · Réseau
Bibliothèque `ureq` avec la couche de chiffrement **du système** (SChannel sous Windows, Security sous macOS) et ses certificats : aucune liste de certificats embarquée (celle habituellement utilisée est sous une licence hors principe 12). Limites strictes par source : MusicBrainz et Cover Art Archive 1 appel / 1,1 s, AcoustID 1 / 0,5 s, Deezer 1 / 0,25 s, iTunes 1 / 3,1 s. Reprise après 429 / 502 / 503 / 504 : 2, 4, 8, 16 s (ou le délai demandé par le service). Chaque appel est inscrit dans la table `online_calls` ; Réglages › Recherche en ligne affiche le nombre maximal d'appels par fenêtre de temps, preuve du respect des limites.

### D-203 · Contact du User-Agent
MusicBrainz exige un contact dans l'en-tête User-Agent. Pour ne rien envoyer sur l'utilisateur sans son accord, ce contact est un réglage **vide par défaut** (« contact non renseigné » est alors envoyé). À confirmer par l'utilisateur.

### D-204 · Empreintes
rusty-chromaprint (licence MIT), réglage « test2 » (celui de l'outil officiel fpcalc), 120 premières secondes, empreinte compressée en base64 sans remplissage. Calculée une fois et gardée en base (table `fingerprints`, avec l'empreinte brute pour les doublons de la phase 3). Recalculée si le fichier change.

### D-205 · Score de confiance (0 à 100)
Empreinte jusqu'à 45 points ; titre 20 ; artiste principal 15 ; version identique +10, **version différente −60** ; durée à 3 s près +10, au-delà −2 par seconde (40 au plus) ; accord d'une autre source +5. Sans empreinte : +15 mais **plafond à 75**, donc jamais d'application automatique sur le seul nom. Le détail (« empreinte 0,96 · titre identique · durée +1 s ») est affiché dans la file. Seuils par défaut : 90 (automatique) et 60 (à valider), réglables. **Confirmé par l'utilisateur le 26 novembre 2026** : pas d'application automatique sans empreinte, même pour une correspondance parfaite par le nom ; ces morceaux passent par « À valider ».

### D-206 · Application automatique
Seuls les champs **vides et non verrouillés** sont remplis ; une valeur existante différente envoie le morceau dans « À valider » (sauf si « Remplacer aussi les valeurs existantes » est coché). Une pochette manquante est ajoutée (Cover Art Archive en priorité). L'application automatique passe par l'écriture sûre et l'historique ; elle ne verrouille rien. Une proposition acceptée par l'utilisateur dans la file verrouille les champs acceptés (c'est son choix).

### D-207 · Choix de la sortie MusicBrainz
Pour l'album et l'année, l'application préfère la sortie officielle d'origine (album, single, EP) aux compilations, mixes de DJ et « Various Artists », puis la plus ancienne. La recherche par nom peut ne trouver que des enregistrements parus sur des compilations : c'est une des raisons pour lesquelles elle n'est jamais appliquée seule.

### D-208 · File hors connexion
Les recherches sont rangées dans `online_queue`. Un fil de travail les traite une par une ; sans réseau, il réessaie toutes les 30 secondes et prévient (« Connexion retrouvée : N recherches faites ») quand la file est rattrapée. Trois échecs d'un même morceau (hors coupure) le sortent de la file avec l'état « erreur ».

### D-209 · Inbox
Un dossier ajouté comme « dossier d'arrivée » est indexé comme les autres ; chaque **nouveau** fichier y est inscrit dans l'Inbox et mis en file de recherche. Après une application automatique, il est renommé selon le modèle (réglage « Inbox seulement » par défaut). Le rangement vers la bibliothèque reste manuel (« Ranger… », le dernier dossier utilisé est proposé). La règle « Après validation, déplacer vers [dossier] » n'est pas encore proposée (voir IDEES) ; elle était prévue désactivée par défaut.

### D-210 · Recadrage
L'éditeur garde un carré (déplaçable, redimensionnable), pivote par quarts de tour et compare avant / après. Le recadrage est appliqué par le cœur sur l'image d'origine (pas sur l'aperçu). Les images presque carrées (écart de 10 % au plus) sont recadrées automatiquement, sans ouvrir l'éditeur.
