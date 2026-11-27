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

## Phase 3 — Doublons et qualité

### D-301 · Emplacement de la quarantaine
Le cahier des charges place `_Quarantaine` « à la racine du même disque ». Sur le disque interne d'un Mac, la racine (`/` ou `/System/Volumes/Data`) n'est pas modifiable sans mot de passe administrateur, et la racine de `C:\` peut l'être aussi sur certains Windows. Le dossier `_Quarantaine` est donc créé **en haut du dossier de bibliothèque** qui contient le fichier (par exemple `E:\Musique\_Quarantaine\…`). Quand ce dossier de bibliothèque est la racine du disque, c'est exactement la racine du disque. Les garanties demandées restent vraies : même disque, déplacement instantané (simple renommage, aucune copie), arborescence d'origine gardée, dossier jamais indexé, rien n'est écrasé (un nom déjà pris reçoit « (2) »).

### D-302 · Fiche d'un morceau en quarantaine
Le morceau garde sa fiche en base (état « quarantined ») : ses verrous, son historique et ses résultats en ligne reviennent avec lui à la restauration. Il est caché de la bibliothèque, des compteurs et des recherches. Une nouvelle indexation ne le marque jamais « manquant ». Mise en quarantaine, restauration et leurs annulations passent par l'historique (opération « quarantine »), même après redémarrage. « Vider la quarantaine » (après confirmation affichant le nombre et la taille) envoie chaque fichier dans la corbeille du système avec la bibliothèque `trash` (licence MIT), un fichier à la fois, puis retire sa fiche ; cette dernière étape n'est plus annulable dans l'application (le fichier reste récupérable depuis la corbeille). Les fichiers d'un disque débranché sont laissés de côté.

### D-303 · Recherche des doublons
- **Copies exactes** : même hash du seul flux audio (calculé à l'indexation).
- **Même enregistrement** : présélection par artiste principal et titre normalisés (sans accents, sans ponctuation ; nom de fichier si le titre manque), écart de durée de 2 s au plus, versions compatibles (« Original » = « Original Mix »…). Les empreintes Chromaprint de la phase 2 (gardées en base) sont alors comparées sur l'ordinateur : part des bits identiques au meilleur décalage (± 5 s). Seuil : **0,85** (deux morceaux différents donnent environ 0,5 ; un WAV et son MP3 320 plus de 0,9).
- Deux versions connues différentes ne sont **jamais** regroupées, même avec des empreintes proches (test automatique). Lors de la fusion des paires en groupes, chaque nouveau membre est vérifié contre tous les autres.
- « Ce ne sont pas des doublons » enregistre toutes les paires du groupe (table `not_duplicates`) : elles ne seront plus proposées. Un « À garder » choisi à la main est conservé aux recherches suivantes.
- La recherche se lance à la demande (bouton « Rechercher les doublons ») : elle peut calculer beaucoup d'empreintes, donc lire beaucoup de fichiers.

### D-304 · Meilleure version
Ordre : 1. sans perte non suspect (WAV, AIFF, FLAC, ALAC dans l'ordre réglable, puis la plus haute résolution) ; 2. avec perte d'au moins 320 kbit/s ; 3. fichier suspect (faux sans perte, gonflé), donc derrière un vrai MP3 320 ; 4. avec perte sous 320 kbit/s, par débit décroissant. À égalité : tags les plus complets, puis pochette, puis note. Le réglage propose les six ordres possibles des trois formats sans perte (liste fermée, validée par le cœur).

### D-305 · Traitement d'un groupe
Aperçu obligatoire. Le fichier gardé reçoit **uniquement ce qui lui manque** (champs vides), pris dans le meilleur des autres fichiers, ainsi que la note la plus haute du groupe et une pochette s'il n'en a pas. Ce report se fait comme une automatisation : un champ verrouillé n'est jamais écrasé. Puis les autres fichiers partent en quarantaine. Tout est dans **un seul lot d'historique** : une seule annulation remet tags et fichiers en place. Si le report échoue (fichier verrouillé, disque plein…), les autres fichiers du groupe **ne sont pas écartés**.

### D-306 · Analyse spectrale
Chaque fichier est décodé en entier (Symphonia). Une fenêtre de 4 096 échantillons (Hann, FFT `rustfft`, licence MIT / Apache-2.0) est analysée toutes les 0,25 s ; les passages silencieux sont ignorés. Le spectre moyen est découpé en bandes de 250 Hz. On cherche la bande la plus haute dont le niveau dépasse « niveau du médium (1 à 6 kHz) − 50 dB », puis on vérifie que la chute est **nette** (au moins 25 dB entre juste en dessous et juste au-dessus) : une atténuation progressive (vieil enregistrement) n'est pas une coupure.
- WAV, AIFF, FLAC, ALAC avec une coupure nette sous **20,5 kHz** : « suspect (faux sans perte) ».
- MP3 ou AAC annoncé à 256 kbit/s ou plus, avec une coupure sous **17,5 kHz** : « suspect (gonflé) ».
Aucune action automatique. Le spectrogramme est calculé à la demande (image de 600 × 256 points) et n'est pas gardé, pour ne pas alourdir la base.

### D-307 · Fichiers abîmés ou tronqués
Pendant la même lecture : un paquet audio impossible à décoder est compté (il était déjà sauté sans couper le son du lecteur) ; une fin de fichier inattendue, ou une durée lue inférieure de plus de 2 s et de 5 % à la durée annoncée, signale un fichier **tronqué**. Le contrôle qualité se lance à la demande (« Santé de la bibliothèque › Lancer le contrôle qualité ») ou pour un morceau (panneau de détail). Les résultats sont gardés en base et refaits seulement si le fichier change.

### D-308 · Compteurs de santé
Les compteurs sont calculés par le cœur et filtrent la bibliothèque avec exactement la même règle (test automatique : chaque compteur ouvre une liste du même nombre de morceaux). « Pochette trop petite » : moins de 500 px de large. « Débit faible » : MP3, AAC, OGG, Opus sous le seuil réglable (320 kbit/s par défaut ; un M4A sans perte ALAC n'est pas compté). « Chemins trop longs » : plus de 250 caractères avec le point de montage connu. « Caractères interdits » : `< > : " \ | ? *`, caractères de contrôle, espace ou point en fin de nom, noms réservés de Windows. « Noms non conformes » : le nom ne correspond pas au modèle de renommage actuel (un suffixe « (2) » est toléré ; un morceau aux tags insuffisants n'est pas compté).

### D-309 · Corrections proposées
Toujours avec aperçu ou validation : recherche en ligne (les cas incertains vont dans « À valider »), renommage (fenêtre de renommage habituelle), resynchronisation des WAV (liste des fichiers puis confirmation ; seule la partie RIFF INFO est réécrite d'après l'ID3 ; l'annulation ne recrée pas l'ancien désaccord, qui était une erreur), fichiers manquants retrouvés dans un dossier choisi.

### D-310 · Fichiers manquants retrouvés
Recherche dans le dossier choisi : même nom et même taille, sinon même son (hash du flux audio, pour un fichier dont les tags ont changé). Aperçu, puis la fiche d'origine est rattachée au nouvel emplacement (verrous, historique et résultats en ligne gardés). Si le fichier avait déjà été indexé à sa nouvelle place, cette fiche en double est retirée. Aucun fichier n'est déplacé.

## Phase 4 — Rekordbox et finitions

### D-401 · Reconnaissance des morceaux du XML Rekordbox
L'attribut `Location` (`file://localhost/…`, caractères spéciaux encodés) est décodé puis comparé au chemin complet des fichiers indexés (sans tenir compte des majuscules). Si le disque a changé de lettre ou de point de montage depuis l'export, le chemin relatif au disque est comparé (avec le nom du disque sous macOS, `/Volumes/NOM/…`) ; si plusieurs morceaux pourraient correspondre, aucun n'est choisi (on ne devine pas). Un nouvel import remplace entièrement les badges. Le fichier XML n'est jamais modifié. Bibliothèque `quick-xml` (licence MIT), déjà utilisée par Tauri.

### D-402 · Mode protégé
Réglage `rekordbox_protected`, activé par défaut. Il empêche le renommage automatique de l'Inbox pour un morceau badgé (seule automatisation qui renomme ; aucune ne déplace). Les aperçus de renommage, de déplacement, de traitement des doublons, de mise en quarantaine et de conversion affichent une alerte quand un morceau badgé est concerné. L'alerte s'affiche même si le mode protégé est désactivé : elle informe sans bloquer.

### D-403 · Journal des chemins
Chaque renommage, déplacement (de morceaux ou de dossier), mise en quarantaine et restauration inscrit (ancien chemin complet ; nouveau chemin complet ; présent dans Rekordbox ; date) dans la table `path_journal`, y compris les annulations. Export CSV : séparateur « ; » et BOM UTF-8 pour qu'Excel en français l'ouvre correctement ; option « seulement les morceaux présents dans Rekordbox » (cochée par défaut).

### D-404 · « À recharger »
Toute écriture de tags réussie (y compris une annulation) sur un morceau badgé l'inscrit dans `rekordbox_reload`. Export en M3U8 (UTF-8, `#EXTINF` avec durée, artistes et titre, chemins complets). Les morceaux d'un disque débranché sont laissés de côté et comptés. Le cahier des charges demande de vérifier le comportement de « Reload Tag » sur une copie de bibliothèque Rekordbox : **je n'ai pas pu le faire** (Rekordbox n'est pas disponible ici) ; c'est un test du rapport de phase.

### D-405 · Export XML vers Rekordbox
Format `DJ_PLAYLISTS` 1.0.0 : `COLLECTION` (TrackID = identifiant interne, Name = titre écrit comme dans les fichiers selon le réglage « version dans le titre », Artist = artistes séparés par « , », Mix = version, Tonality = notation musicale « Am », Rating 0 / 51 / 102 / 153 / 204 / 255, Location encodée, Kind, Size, TotalTime, BitRate, SampleRate, Year, AverageBpm, Remixer, Label, DateAdded = jour de l'export) puis `PLAYLISTS` (ROOT › dossier « DJ Library Manager » › une playlist par playlist ou filtre intelligent, filtres exportés avec leurs morceaux du moment). Fichier écrit dans un fichier temporaire puis renommé. Comportement de Rekordbox pour les morceaux déjà présents dans sa collection (mise à jour de la note ?) : **non vérifié** (pas de Rekordbox ici), à tester par l'utilisateur après sauvegarde ; l'aide le dit.

### D-406 · Mises à jour
« Mise à jour automatique » : l'application demande au démarrage à l'API publique de GitHub la dernière version publiée (aucune donnée de l'utilisateur envoyée), la compare à la sienne et, si elle est plus récente, propose « Télécharger » qui ouvre la page de la version dans le navigateur. Désactivable (réglage `update_check`, activé par défaut parce que le cahier des charges demande les mises à jour automatiques ; c'est un accès réseau en plus des sources de recherche, signalé dans le README). L'installation entièrement automatique (module « updater » de Tauri) exige une clé de signature des mises à jour à créer et à garder secrète : elle est préparée dans `docs/SIGNATURE.md` et reste inactive. L'ouverture de page passe par le cœur, qui n'accepte que les adresses du dépôt du projet.

### D-407 · Playlists et filtres intelligents
Tables `playlists` et `playlist_tracks`. Une playlist garde l'ordre d'ajout et refuse les doublons. Un filtre intelligent est un ensemble de règles (champ, condition, valeur ; « toutes » ou « au moins une ») traduit en SQL à partir d'une **liste fermée** de champs et de conditions (aucune injection possible ; règle invalide refusée à l'enregistrement). « contient » ne distingue pas majuscules et minuscules pour les lettres sans accent (limite de SQLite). Supprimer une playlist ne touche ni aux morceaux ni aux fichiers. Les morceaux en quarantaine n'apparaissent dans aucune playlist.

### D-408 · Conversion WAV → AIFF
Seulement les WAV PCM entiers (8, 16, 24, 32 bits, y compris WAVE_FORMAT_EXTENSIBLE) : un WAV en virgule flottante demanderait un AIFF-C, moins bien lu ; il est refusé et signalé. Les échantillons sont recopiés en changeant l'ordre des octets (8 bits : non signé → signé, même valeur). Le tag ID3 du WAV (avec les données Serato, Rekordbox, etc.) est recopié tel quel, puis tous les champs (RIFF INFO compris) et la pochette sont réécrits par l'écriture sûre habituelle (ID3v2.4 dans l'AIFF). Vérification : les deux fichiers sont décodés et comparés échantillon par échantillon ; au moindre écart, le fichier temporaire est supprimé et le WAV reste intact. L'AIFF reprend les cadenas et les playlists du WAV ; le WAV part en quarantaine. Annulation : l'AIFF part en quarantaine (jamais effacé) et le WAV revient. Un AIFF du même nom déjà présent bloque la conversion de ce fichier.

### D-409 · Captures d'écran du guide
Prises avec Microsoft Edge sans fenêtre (`--headless`, 1 400 × 900) sur l'aperçu de l'interface avec des données de démonstration (`?demo`, `?view=…`, `?theme=dark`, options qui n'existent que dans l'aperçu navigateur, jamais dans l'application installée). Elles montrent donc l'interface réelle avec des morceaux fictifs.

### D-410 · Artiste dans le titre (demande de l'utilisateur, hors cahier des charges)
Les platines n'affichent souvent que le titre dans les playlists exportées par Rekordbox. Nouveau réglage `artist_in_title` (désactivé par défaut), combiné avec `version_in_title` : l'interface propose 4 présentations (titre ; titre (version) ; artiste principal - titre ; artiste principal - titre (version)). Seul le **champ titre du fichier** change (et `Name` dans l'export XML Rekordbox) : la base garde titre, version et artistes séparés. À la relecture d'un fichier, « Artiste principal - » est retiré du début du titre s'il correspond au premier artiste (séparateurs « - », « – », « — »), pour que la recherche en ligne, les doublons et le renommage voient le vrai titre. Quand l'artiste change et que le réglage est actif, le titre est réécrit aussi. « Appliquer aux fichiers existants » écrit le texte exact du titre (`raw_title`) dans un seul lot d'historique : l'état d'avant garde le texte exact précédent, donc Ctrl+Z le remet tel quel. Les fichiers déjà écrits ainsi, sans titre, manquants, illisibles ou en quarantaine sont laissés de côté. Cette réécriture est une modification automatique mais elle n'est pas bloquée par un cadenas sur le titre : la valeur du titre ne change pas, seule sa présentation change.

### D-411 · Tri par note
Le tri par note existait par clic sur la colonne. Ajout d'un menu « Trier par » dans la barre de filtres (préréglages : ordre d'ajout, note 5 étoiles d'abord, sans note d'abord, ajoutés récemment, artiste principal, titre ; à note égale, tri par artiste puis titre). La colonne Note trie du mieux noté au moins bien noté au premier clic.

### D-412 · MP3 marqués « illisible » à tort
Constat sur la bibliothèque de l'utilisateur (`E:\MUSICS\ALL`, 3 273 fichiers, vérification en lecture seule) : 44 MP3 lisibles par tous les lecteurs étaient refusés par lofty, pour deux raisons :
1. un **tag APE de fin abîmé** (en-tête « APETAGEX » qui annonce 4 éléments mais ne contient que des zéros), placé devant l'ID3v1 : lofty abandonne le fichier entier ;
2. des **octets vides entre l'ID3v2 et le premier bloc de son** : lofty ne devine pas le type du fichier.

Lecture : si lofty échoue, on réessaie avec le type donné par l'extension ; pour un MP3, en dernier recours, les tags sont lus sur une copie **en mémoire** réduite à l'ID3v2, au début du son et à l'ID3v1, et les propriétés (durée, débit) sur le fichier entier sans les tags. Rien n'est écrit pendant la lecture.
Écriture : si la lecture stricte échoue, l'ID3v2 est relu à part (toujours en mode strict, donc refus si lui-même est douteux) et seul l'ID3v2 est réécrit ; la fin du fichier (APE abîmé, ID3v1) reste octet pour octet identique et le son est vérifié comme d'habitude. Le tag APE abîmé n'est pas supprimé (il ne contient rien, mais on ne retire jamais un tag de nous-mêmes).
Indexation : un fichier « illisible » est relu à chaque indexation, même s'il n'a pas changé, pour profiter des corrections ; s'il est dans un dossier d'arrivée, il entre dans l'Inbox dès qu'il devient lisible.
Un test reproduit le cas réel. Un diagnostic en lecture seule d'un dossier reste disponible pour les développeurs (`DJLM_DIAG_DIR=… cargo test diag_dossier -- --ignored --nocapture`).

### D-413 · MP3 au gros tag ID3 refusés par le moteur de son
Constat sur `E:\MUSICS\ALL` (lecture seule) : 67 MP3 sur 3 273 étaient mal ouverts par Symphonia, le moteur de décodage :
- 48 avec un tag ID3v2 de plus de 1 Mo (grandes pochettes, données de logiciels DJ) : Symphonia ne cherche le début du son que dans le premier Mo et refuse le fichier (« no suitable format reader found ») ; marqués « illisible » et muets ;
- 19 dont la pochette contient des octets qui ressemblent à un en-tête AAC : Symphonia prend la pochette pour le son (« unsupported audio codec » à l'écoute, empreinte du son calculée sur la pochette) ;
- 1 avec deux tags ID3v2 à la suite, le premier annonçant une taille un peu trop courte.

Correction (`audio/source.rs`) : avant de passer le fichier à Symphonia, l'application lit l'en-tête ID3v2, saute le ou les tags (un second tag est cherché dans les 4 Kio qui suivent) et présente à Symphonia le fichier à partir du premier octet de son. Lecture seule. Vérifié sur la bibliothèque de l'utilisateur : 3 273 fichiers sur 3 273 s'ouvrent ; sur un échantillon de 321 fichiers déjà lisibles, l'empreinte du son est identique dans 320 cas (le dernier était l'un des fichiers décodés de travers).
Migration 8 : pour les MP3, l'empreinte du son, l'empreinte acoustique, la forme d'onde et l'analyse de qualité sont effacées pour être recalculées (automatiquement pour l'empreinte du son ; à la demande pour le reste). Coût : un nouveau calcul des empreintes des MP3 en tâche de fond au premier démarrage, et le contrôle qualité des MP3 à relancer.

### D-414 · « La vérification après écriture a échoué » en modifiant un titre
Constat (copies des fichiers de `E:\MUSICS\ALL`, originaux seulement lus) : beaucoup de titres d'origine sont écrits « Artiste - Titre (Version) ». Trois défauts liés :
1. la vérification après écriture comparait le titre **relu et découpé** (sans « Artiste - », sans « (Version) ») au titre voulu : toute écriture d'un titre commençant par l'artiste, ou finissant par une version sans champ Version, était refusée. Elle compare maintenant le **texte exact** relu dans le fichier à celui qu'on voulait écrire ; la version n'est contrôlée que si elle est renseignée ;
2. « Appliquer aux fichiers existants » (artiste dans le titre) ajoutait l'artiste une seconde fois quand le titre en base le contenait encore (« 113 - 113 - Au Summum (Extended) ») : l'artiste n'est plus jamais ajouté s'il est déjà en tête, et la lecture retire « Artiste - » autant de fois qu'il apparaît ;
3. la base gardait les titres lus avant D-410 pour les fichiers inchangés : migration 9, chaque fichier est relu une fois au prochain scan (sans recalcul des empreintes ; les champs verrouillés ne sont pas touchés).
Les fichiers déjà doublés se corrigent en relançant « Appliquer aux fichiers existants » (ou Ctrl+Z sur ce lot).
Autre cas trouvé : certains MP3 (octets vides après l'ID3v2) ne pouvaient pas être enregistrés, lofty ne reconnaissant pas le type du fichier au moment d'enregistrer ; le tag ID3v2 du début est alors remplacé directement, puis le son vérifié comme toujours. Restent refusés par sécurité (environ 0,5 %) les fichiers dont un champ de date ID3 est mal formé (voir docs/IDEES.md).
Sur un échantillon de 819 copies : 815 titres modifiés sans erreur, 4 refusés (dates mal formées).

## Phase 5 — Veille musicale

### D-501 · Sources choisies
Gratuites et sans compte : Apple Music (seule source publique de classements **par pays**), Deezer (classements par genre), MusicBrainz (seule source gratuite permettant de chercher les sorties **par label** et **par pays** sur une période). Beatport, Shazam et Bandcamp n'ont pas d'accès public ; Discogs demanderait une clé et ne trie pas par date de sortie. Chaque source est désactivée par défaut (même règle que la recherche en ligne). Les classements Deezer ne sont jamais mis en cache (conditions de Deezer).

### D-502 · Liste d'envies
Table `wishlist` (migration 10). Rapprochement avec la bibliothèque par « artiste principal | titre » normalisés (sans accents, ponctuation, version, « feat. » ou « with ») : une Extended Mix achetée satisfait une envie repérée en Radio Edit. L'état « trouvé » est recalculé à chaque affichage de la liste et du badge (pas de tâche de fond). Retirer une envie la masque (`deleted = 1`) pour permettre « Annuler » ; ce n'est pas une donnée audio, l'historique général n'est pas utilisé.

### D-503 · Liens vers les boutiques
Liens de **recherche** (Beatport, Bandcamp, Juno Download, Traxsource) construits avec « artiste titre » : rien n'est envoyé tant que l'utilisateur ne clique pas. Le cœur n'ouvre que les adresses de ces boutiques, des sources et de Spotify.

### D-504 · Compte Spotify
OAuth 2 avec PKCE (RFC 7636, aucune clé secrète dans l'application) et identifiant client créé par l'utilisateur : depuis février 2026, chaque application Spotify en mode développement appartient à un compte Premium et accepte 5 comptes. Retour sur `http://127.0.0.1:43821/callback` (Spotify refuse « localhost » et accepte l'adresse de bouclage en http) : l'application écoute ce port seulement pendant la connexion (3 minutes au plus) et vérifie le paramètre `state`. Jeton de renouvellement dans le trousseau du système (comme la clé AcoustID), jeton d'accès en mémoire. Recherche par ISRC si connu, puis « track:… artist:… », puis texte libre ; le résultat retenu doit avoir le même titre et le même artiste principal, la version comptant en plus. L'utilisateur voit les correspondances et peut les décocher avant l'ajout. Points d'accès de février 2026 : `POST /me/playlists`, `POST /playlists/{id}/items` (100 titres par appel).
Non vérifié faute de compte : la connexion et l'ajout réels (test de l'utilisateur).

### D-505 · Tidal reporté
Tidal propose une API récente (OAuth, écriture des playlists), mais ses conditions et son fonctionnement n'ont pas pu être vérifiés sur la documentation officielle pendant cette phase. Noté dans docs/IDEES.md.

### D-506 · Écoute d'extraits dans l'écran Veille (demande de l'utilisateur)
Bouton ▶ sur chaque titre (classements, sorties, liste d'envies) : extrait de 30 secondes de Deezer (par défaut) ou d'Apple Music (réglage `watch_preview`, « désactivés » possible). L'adresse de l'extrait est demandée **au clic seulement** (recherche « artiste titre version », même règle de choix que pour Spotify : titre et artiste principal identiques) ; pour un titre d'un classement Deezer, l'extrait fourni par le classement est utilisé directement. Le son est lu par l'interface directement depuis les serveurs du service (élément audio du navigateur intégré) : rien n'est téléchargé ni gardé, ce qui respecte les conditions de Deezer (pas de copie locale) et d'Apple ; jamais de cache (les adresses Deezer expirent). Le lecteur de la bibliothèque se met en pause pendant l'extrait. Les morceaux entiers ne sont pas proposés : Spotify n'autorise plus la lecture hors de ses applications pour les applications en mode développement, et aucune source gratuite ne le permet.
La règle de sécurité de l'interface (CSP) n'autorisait aucune image ni aucun son extérieur : elle autorise maintenant les pochettes d'Apple (`*.mzstatic.com`), de Deezer (`*.dzcdn.net`) et de Cover Art Archive (`coverartarchive.org`, `archive.org`), et les extraits de Deezer et d'Apple (`media-src`), rien d'autre. Sans cela, les pochettes de l'écran Veille ne s'affichaient pas dans l'application installée (seulement dans l'aperçu).

### D-507 · Site d'ouverture des titres
Réglage `watch_open_with` : page de la source (par défaut), Apple Music, Deezer, Spotify, YouTube, YouTube Music, SoundCloud ou Beatport. Si le titre vient du site choisi, sa page s'ouvre directement ; sinon une recherche « artiste titre » est ouverte sur le site choisi. Rien n'est envoyé avant le clic ; le cœur n'ouvre que les adresses de ces sites.

### D-508 · Extraits dans le lecteur du bas
Les extraits ne sont plus lus par l'interface mais par le lecteur de l'application (même moteur que les fichiers) : l'extrait (≈ 0,5 Mo en MP3 pour Deezer, ≈ 1 Mo en AAC pour Apple) est téléchargé **en mémoire** au clic, décodé par Symphonia depuis la mémoire et joué ; il n'est jamais écrit sur le disque et disparaît à l'extrait suivant. La barre du bas affiche pochette, titre, artistes et « Extrait Deezer / Apple Music » ; précédent / suivant et l'enchaînement de fin parcourent la liste affichée (un titre sans extrait est sauté, 6 essais au plus). Vérifié : extraits Deezer (MP3) et Apple (AAC) décodés entièrement (30,0 s).
Recherche de l'extrait sur plusieurs plateformes : service choisi, puis l'autre ; Apple en France puis aux États-Unis ; titre sans « (feat. …) » ; avec puis sans la version. Sans connexion, la recherche s'arrête tout de suite.
Le bouton ▶ est sur la pochette (toujours visible, plus marqué au survol et pendant la lecture).

### D-509 · Labels par défaut et favoris
Liste d'environ 70 grands labels dans l'interface (électro, majors, rap français, afro, latino), complétée par les labels ajoutés (réglage `watch_labels_custom`) ; favoris dans `watch_labels_fav` (listes JSON). « Tous mes favoris » envoie une seule requête MusicBrainz `label:("A" OR "B" …)` (40 labels au plus), au lieu d'une requête par label (1 par seconde). Un label ajouté devient favori.

### D-510 · « Veille » renommé « Sorties »
À la demande de l'utilisateur. L'onglet « Nouvelles sorties » devient « Nouveautés » pour éviter « Sorties › Nouvelles sorties ». Les noms internes (`watch_…`) ne changent pas, pour garder les réglages déjà enregistrés.

### D-511 · Disque interne du Mac présenté depuis « / »
Sur les Mac récents, les fichiers de l'utilisateur sont sur la partie « données » du disque interne, montée à `/System/Volumes/Data` et visible depuis `/` par les liens du système (firmlinks : /Users, /private…). L'application construisait des chemins `/System/Volumes/Data/Users/…` : les fichiers étaient trouvés, mais ces chemins n'auraient pas correspondu à ceux de Rekordbox (export XML, import, journal) ni à ce que l'utilisateur connaît. Le point de montage présenté pour ce volume est maintenant `/`. Les chemins relatifs enregistrés et l'identifiant du volume (UUID) ne changent pas. Repéré grâce au test de quarantaine sur la machine macOS de GitHub (0.6.1).
