# Cahier des charges Codex – Gestionnaire audio DJ

Nov 26, 2026 · @Tristan Faudé

## Mode d'emploi (pour toi)

Ce document est le prompt complet à donner à Codex : un cahier des charges découpé en 5 phases, chacune testée et validée avant la suivante.

1. Crée un dépôt GitHub vide (par exemple `dj-library-manager`) et connecte-le à Codex.
2. Exporte ce document en Markdown et ajoute-le au dépôt sous le nom `docs/CAHIER_DES_CHARGES.md`.
3. Ajoute à la racine du dépôt un fichier `AGENTS.md` avec le contenu ci-dessous. Codex le relit à chaque tâche.
4. Lance une tâche Codex par phase, avec le message type ci-dessous. N'enchaîne jamais deux phases dans la même tâche.
5. À la fin de chaque phase, télécharge les installateurs Windows et Mac produits par GitHub (onglet Actions ou Releases du dépôt), puis vérifie un à un les critères de validation de la phase.
6. Tant qu'une phase n'est pas validée, teste sur une copie de quelques dizaines de morceaux, jamais sur ta vraie bibliothèque.
7. Renvoie les problèmes à Codex dans une nouvelle tâche : « Corrige les points suivants de la Phase X : … ».

Contenu du fichier `AGENTS.md` :

```markdown
# Instructions permanentes
- Le cahier des charges de référence est docs/CAHIER_DES_CHARGES.md : relis-le avant chaque tâche.
- L'utilisateur ne sait pas coder : rapports, messages et documentation en français simple.
- Priorité absolue : ne jamais abîmer ni perdre un fichier audio (section « Principes non négociables »).
- Ne réalise que la phase demandée ; note les idées hors phase dans docs/IDEES.md.
- Consigne chaque choix technique non prévu dans docs/DECISIONS.md, avec sa raison.
- Chaque tâche se termine par : tests automatiques au vert, compilation Windows et macOS au vert,
  rapport en français (fait, pas fait, étapes pour tester soi-même).
```

Message type pour lancer une phase :

```text
Lis AGENTS.md et docs/CAHIER_DES_CHARGES.md. Réalise uniquement la Phase 1 (MVP).
Respecte tous ses critères de validation. Termine par un rapport en français :
ce qui est fait, ce qui reste, et les étapes pour que je teste moi-même l'application.
```

## Mission

*À partir d'ici, le texte s'adresse à Codex.*

Tu es le développeur principal d'une application de bureau pour Windows et macOS qui organise la bibliothèque audio d'un DJ : tags, pochettes, noms de fichiers, notes, doublons, écoute et dossiers.

**Utilisateur.** Un DJ organisateur francophone qui ne code pas. Il prépare ses playlists dans Rekordbox, puis les exporte sur clé USB pour jouer sur du matériel Pioneer DJ / AlphaTheta. Sa musique est sur un disque externe. Formats principaux : WAV (le plus utilisé), MP3 et FLAC. Aujourd'hui moins de 10 000 morceaux, demain beaucoup plus.

**Objectif.** Chaque morceau doit avoir des informations justes et complètes, un nom de fichier propre et une pochette, avec le moins de travail manuel possible et sans jamais mettre un fichier en danger.

**Ce que fait l'application :**

- indexer la musique d'un ou plusieurs dossiers, y compris sur disque externe ;
- lire et écrire titre, version, artistes, album, année, genre, BPM, tonalité, note et pochette ;
- renommer les fichiers selon un modèle, en lot, avec aperçu et annulation ;
- compléter les informations en ligne de façon autonome, avec une file « À valider » pour les cas incertains ;
- fonctionner entièrement hors connexion pour tout le reste ;
- détecter les doublons et garder la meilleure version ;
- écouter les morceaux avec une forme d'onde cliquable ;
- gérer les dossiers depuis l'application ;
- rester compatible avec Rekordbox.

**Nom.** Provisoire. Utilise l'identifiant neutre `dj-library-manager`, facile à changer plus tard.

**Méthode.** Tu travailles phase par phase (section « Plan de réalisation »). Quand un point n'est pas précisé, choisis l'option la plus sûre pour les fichiers et la plus simple pour l'utilisateur, puis note-la dans `docs/DECISIONS.md`.

## Principes non négociables

Ces règles passent avant tout le reste, y compris la vitesse et le confort.

1. **L'audio n'est jamais modifié.** L'application ne touche qu'aux métadonnées et aux noms de fichiers. Seule exception : la conversion WAV → AIFF de la Phase 4, lancée explicitement par l'utilisateur.
2. **Écriture sûre.** Chaque écriture passe par une copie temporaire dans le même dossier, une relecture de contrôle (tags relus identiques aux tags voulus, flux audio identique), puis un remplacement atomique. En cas d'échec, l'original reste intact et l'erreur s'affiche en clair. Une optimisation est permise si elle garde ces garanties ; note-la dans `docs/DECISIONS.md`.
3. **Rien n'est supprimé définitivement.** Les fichiers écartés vont dans une quarantaine restaurable. Ils ne partent dans la corbeille du système qu'après confirmation explicite.
4. **Tout s'annule.** Chaque opération (tag, pochette, renommage, déplacement, quarantaine) est inscrite dans un historique persistant avec l'état avant et après. L'utilisateur peut annuler une opération ou un lot entier, même après redémarrage.
5. **Aperçu avant toute action en lot.** Renommage, nettoyage ou application de résultats en ligne : l'utilisateur voit l'avant/après et confirme.
6. **Champs verrouillés.** Un champ modifié à la main est verrouillé (icône cadenas). Aucune automatisation ne l'écrase. L'utilisateur peut le déverrouiller.
7. **Tags inconnus conservés.** Les tags que l'application ne gère pas (commentaires, données Serato, champs Rekordbox, etc.) sont préservés à l'identique.
8. **Hors connexion d'abord.** Tout fonctionne sans internet, sauf la recherche en ligne. Aucune fonction ne bloque ni ne plante faute de réseau.
9. **Aucun déplacement automatique.** Un fichier ne change de dossier que sur action manuelle, ou par une règle créée volontairement par l'utilisateur.
10. **Vie privée.** Aucune télémétrie. Le réseau ne sert qu'aux sources activées. Les clés d'API sont rangées dans le trousseau du système (Gestionnaire d'identification Windows, Trousseau macOS), jamais dans le code ni dans le dépôt.
11. **Performance.** L'interface ne se fige jamais : analyses, écritures et recherches tournent en tâche de fond, avec progression et bouton Annuler. Cibles : défilement fluide à 100 000 morceaux, résultat de recherche en moins de 200 ms, indexation initiale de 10 000 fichiers en moins de 5 minutes sur disque USB 3 (hors empreintes audio).
12. **Licences permissives.** Uniquement des composants sous MIT, Apache-2.0, BSD, ISC, Zlib ou MPL-2.0, pour pouvoir partager l'application plus tard. Pas de GPL ni d'AGPL. Liste tenue à jour dans `THIRD_PARTY_LICENSES.md`.

## Plateformes, technologie et livraison

**Plateformes cibles :** Windows 10 et 11 (64 bits) ; macOS 12 ou plus récent, Apple Silicon et Intel (binaire universel).

**Pile technique recommandée.** Tu peux t'en écarter si tu justifies le choix dans `docs/DECISIONS.md`. Vérifie la licence de chaque dépendance (principe 12).

| Besoin | Recommandation |
| --- | --- |
| Application de bureau | Tauri 2, dernière version stable (2.12 publiée en septembre 2026, [source](https://v2.tauri.app/release/tauri/v2.12.0/)) |
| Interface | React + TypeScript + Vite |
| Tableau des morceaux | TanStack Table + TanStack Virtual (lignes virtualisées) |
| Traductions | i18next |
| Icônes | Lucide |
| Lecture et écriture des tags | lofty (Rust) : WAV (ID3v2 + RIFF INFO), AIFF, FLAC, MP3, M4A ([source](https://docs.rs/crate/lofty/latest)) ; écriture ID3v2.3 via `WriteOptions::use_id3v23` ([source](https://docs.rs/crate/lofty/0.24.0/source/src/config/write_options.rs)) |
| Décodage audio | Symphonia |
| Sortie audio | cpal ou rodio |
| Base de données locale | SQLite avec recherche plein texte FTS5 (rusqlite) |
| Empreintes audio | rusty-chromaprint, portage Rust de Chromaprint sous licence MIT ([source](https://lib.rs/crates/rusty-chromaprint)) |
| Surveillance des dossiers | notify |
| Pochettes | image (redimensionnement, recadrage, JPEG) |
| Corbeille système | trash |
| Trousseau | keyring |

**Architecture.** Le cœur (fichiers, tags, base, réseau, audio) est écrit en Rust. L'interface affiche et appelle le cœur par des commandes, sans logique métier. Modules séparés et testables : `library` (indexation), `tags` (un adaptateur par format), `rename`, `online` (un module par source), `artwork`, `duplicates`, `quality`, `audio` (lecture, forme d'onde), `history` (journal et annulation), `rekordbox`.

**Compilation et livraison.** L'utilisateur ne compile rien lui-même.

- GitHub Actions compile à chaque modification sur Windows et macOS et produit les installateurs : `.msi` ou `.exe` pour Windows, `.dmg` universel pour macOS.
- Chaque phase validée donne une version numérotée dans les Releases GitHub, avec ses installateurs.
- L'application n'est pas encore signée. Sur macOS, applique au minimum une signature ad hoc, sinon les Mac Apple Silicon refusent de lancer l'application.
- Le `README.md` explique en français comment ouvrir l'application malgré les alertes. Windows : « Informations complémentaires », puis « Exécuter quand même ». macOS : Réglages Système > Confidentialité et sécurité > « Ouvrir quand même ».
- Prépare dès maintenant la signature officielle (certificat Windows, Developer ID Apple et notarisation) dans la configuration CI. Elle reste inactive tant que les secrets ne sont pas fournis.

## Bibliothèque et données

**Dossiers de bibliothèque.** L'utilisateur ajoute un ou plusieurs dossiers racines. L'application les indexe, puis surveille les ajouts, suppressions, renommages et modifications faits en dehors d'elle.

**Disque externe.**

- Chaque racine est rattachée à son volume (identifiant du volume et nom du disque). Les chemins sont enregistrés par rapport à ce volume. Si la lettre de lecteur change sous Windows (E: devient F:) ou si le point de montage change sous macOS, la bibliothèque se reconnecte seule.
- Disque débranché : les morceaux restent visibles en grisé avec leurs informations, en lecture seule. Un bandeau indique par exemple « Disque MUSIQUE déconnecté ». Les actions en attente reprennent au rebranchement.
- L'aide recommande le format exFAT pour un disque utilisé à la fois sur Windows et sur Mac.

**Base locale.** SQLite, dans le dossier de données de l'application. Elle contient : les morceaux ; les artistes de chaque morceau, ordonnés, avec l'artiste principal ; l'état de chaque champ (verrouillé ou non, origine de la valeur) ; les empreintes audio ; les formes d'onde en cache ; le cache des réponses en ligne ; l'historique des opérations ; les réglages. La base est sauvegardée chaque jour (7 dernières copies conservées). L'utilisateur peut aussi l'exporter et la réimporter.

**Indexation.**

- Formats reconnus : WAV, MP3, FLAC, AIFF, M4A (AAC et ALAC), OGG, Opus. Priorité de qualité et de tests : WAV, MP3, FLAC.
- Données relevées pour chaque fichier : chemin, taille, date de modification, format, codec, débit, fréquence d'échantillonnage, profondeur de bits, canaux, durée, tags, présence d'une pochette, et un hash du seul flux audio (hors tags) qui sert aux doublons exacts.
- Réindexation incrémentale : un fichier n'est relu que si sa taille ou sa date de modification a changé.
- L'indexation tourne en parallèle, en tâche de fond, avec progression et annulation. L'utilisateur peut travailler pendant ce temps.

**Fichiers à problème.** Un fichier illisible, corrompu ou introuvable est marqué et listé dans « Santé de la bibliothèque ». Il n'est jamais ignoré en silence.

## Formats et tags

Le modèle interne d'un morceau est le même pour tous les formats ; un adaptateur par format le traduit en tags.

**Champs gérés**

| Champ | Contenu | Règle |
| --- | --- | --- |
| Titre | nom du morceau, sans la version | obligatoire |
| Version | Extended Mix, Original Mix, Radio Edit, DJ X Remix… | pour un remix, contient le nom du remixeur |
| Artistes | liste ordonnée ; le premier est l'artiste principal | ordre modifiable par glisser-déposer |
| Album | texte |  |
| Année | 4 chiffres |  |
| Genre | texte, avec liste de genres déjà utilisés en suggestion |  |
| BPM | nombre, décimales gardées en interne | écrit arrondi à l'entier par défaut |
| Tonalité | stockée en notation musicale (Am, F#m…) | affichée et écrite en Camelot (8A…) par défaut, réglable |
| Note | 0 à 5 étoiles | saisie au clic ou touches 0 à 5 |
| Pochette | image de face | une seule pochette de type « Cover (front) » |

Le BPM et la tonalité ne sont pas analysés par l'application : Rekordbox s'en charge. Ils sont lus dans les fichiers, complétés en ligne quand une source les fournit, ou saisis à la main.

**Écriture par format**

| Format | Tags écrits par l'application | Ce que Rekordbox lit dans le fichier |
| --- | --- | --- |
| WAV | bloc RIFF INFO et bloc ID3v2.3, toujours écrits ensemble et identiques | RIFF INFO seulement : titre (INAM), artiste (IART), album (IPRD), commentaire (ICMT), genre (IGNR). Ni pochette ni tonalité |
| MP3 | ID3v2.3, texte en UTF-16 | ID3v2.3 : titre, artiste, album, genre, année, Mix Name (TIT3), Remixer (TPE4), label, tonalité (TKEY), pochette (APIC) |
| AIFF | ID3v2.4 dans le chunk ID3 | ID3v2.4, mêmes champs que le MP3 |
| FLAC | Vorbis Comments et bloc PICTURE | Vorbis Comments |
| M4A | atomes iTunes (ilst) | atomes iTunes |

Sources : [liste officielle des champs lus par Rekordbox](https://forums.pioneerdj.com/hc/en-us/articles/205021569-rekordbox-metadata), [lecture du FLAC et du WAV par Rekordbox](https://vibesdj.io/how-to/add-album-artwork-in-rekordbox).

**Règles d'écriture**

- **Version dans le titre.** Par défaut, le titre écrit dans le fichier est « Titre (Version) » (réglage activé d'origine). Ainsi la version s'affiche partout, y compris sur les lecteurs et pour les WAV. Pour MP3 et AIFF, la version est aussi écrite dans TIT3 (Mix Name).
- **Remixeur.** Quand la version se termine par Remix, Edit, Rework, Bootleg ou Dub, le nom qui précède est aussi écrit dans le champ remixeur (TPE4) quand le format le permet.
- **Artistes.** Ils sont écrits dans un seul champ texte, l'artiste principal en premier, séparés par « , » (réglable : « , », « & » ou « ; »). À la lecture, l'application découpe les artistes sur les séparateurs courants (« , », « & », « feat. », « ft. », « x », « vs. »). Elle tient une liste d'exceptions (par exemple « Above & Beyond »), enrichie quand l'utilisateur corrige un découpage.
- **Note.** Écrite dans POPM pour les formats ID3 (WAV, MP3, AIFF), selon l'échelle courante de l'Explorateur Windows : 1★ = 1, 2★ = 64, 3★ = 128, 4★ = 196, 5★ = 255. Pour le FLAC, champ RATING. La note n'apparaît pas dans la liste des champs que Rekordbox lit dans les fichiers : elle lui sera transmise par l'export XML (Phase 4).
- **Pochette.** Image JPEG carrée, 1000 × 1000 pixels par défaut, 1 Mo au plus, de type « Cover (front) ». Rekordbox ignore les pochettes rangées sous un autre type ([source](https://forums.pioneerdj.com/hc/fr/community/posts/900003237143-Album-Art-missing)). Option « Une seule pochette par fichier », recommandée : certains logiciels DJ n'affichent rien quand un fichier contient plusieurs images ([source](https://community.enginedj.com/t/some-cover-art-not-displaying/29299)).
- **WAV.** La pochette et les champs riches vont dans le bloc ID3, lu par d'autres logiciels, mais Rekordbox ne les affiche pas. Les WAV portent un badge d'information : « Rekordbox lira titre, artiste, album, genre et commentaire, mais pas la pochette ».
- **WAV désynchronisés.** À la lecture, si RIFF INFO et ID3 ne disent pas la même chose, ID3 fait foi. L'écart est signalé dans « Santé de la bibliothèque », et la correction réécrit les deux blocs.
- **Accents.** Teste les caractères accentués (é, ü, ñ) dans RIFF INFO avec Rekordbox et documente l'encodage retenu dans `docs/DECISIONS.md`.

## Renommage et nettoyage

**Modèle de nom par défaut :** `{artiste_principal} - {titre}[ ({version})]`

- Exemples : `Artiste A - Titre (Extended Mix).wav` ; pour un remix, `Artiste A - Titre (DJ X Remix).wav` ; sans version, `Artiste A - Titre.mp3`.
- Les autres artistes (B, C, D…) n'apparaissent pas dans le nom de fichier, seulement dans les tags.
- Les crochets `[ ]` délimitent une partie facultative, retirée quand le champ qu'elle contient est vide.

**Éditeur de modèles.** Variables : `{artiste_principal}`, `{artistes}`, `{titre}`, `{version}`, `{album}`, `{annee}`, `{genre}`, `{bpm}`, `{tonalite}`, `{note}`. Elles s'insèrent d'un clic. Un aperçu en direct montre le résultat sur 5 morceaux de la sélection. Les modèles s'enregistrent sous un nom.

**Noms sûrs sur Windows, macOS et clés USB**

- Caractères interdits remplacés : `\ / : * ? " < > |`. Par défaut, `:` devient `  - `, `/` devient `-`, `"` devient `'`, `?` et `*` sont retirés. La table de remplacement est réglable.
- Noms réservés de Windows refusés (CON, PRN, AUX, NUL, COM1 à COM9, LPT1 à LPT9). Espaces et points en fin de nom retirés, espaces doubles réduits.
- Unicode normalisé en NFC. macOS peut stocker les accents décomposés : deux noms identiques à l'œil doivent l'être aussi pour l'application.
- Nom limité à 200 caractères. Alerte si le chemin complet dépasse 250 caractères : Rekordbox sur macOS refuse les chemins de plus de 255 caractères ([source](https://github.com/edkennard/rekordbox-repair)).
- Extension toujours en minuscules.
- Conflit (deux fichiers visant le même nom) : signalé dans l'aperçu. L'utilisateur choisit d'ajouter un suffixe `  (2) ` ou d'ignorer ce fichier.

**Quand renommer**

- Sur l'action « Renommer », pour une sélection ou un lot, avec aperçu avant/après et confirmation.
- Automatiquement après l'application d'informations, selon un réglage : « Inbox seulement » (par défaut), « Toute la bibliothèque sauf les morceaux présents dans Rekordbox », ou « Jamais ».
- Les morceaux déjà présents dans Rekordbox sont protégés (section « Intégration Rekordbox »).

**Nettoyage intelligent.** Règles activables une par une, toujours avec aperçu avant application :

- retirer les mentions parasites : adresses de sites (`www.…`, `….com`), `[Free Download]`, `(Official Video)`, `(Official Audio)`, `(Lyrics)`, `[HQ]` ;
- retirer les numéros de piste en tête (` 01 -  `, ` 01.  `) et remplacer les `_` par des espaces ;
- déplacer les artistes « feat. » du titre vers la liste des artistes ;
- extraire la version du titre : `Titre (Extended Mix)` donne le titre `Titre` et la version `Extended Mix` ;
- harmoniser la casse des versions (`extended mix` devient `Extended Mix`). Mise en majuscules des titres disponible mais désactivée par défaut, pour respecter les noms stylisés ;
- règles personnelles : rechercher/remplacer, avec expressions régulières en option, enregistrables.

**Nom de fichier et tags dans les deux sens**

- Tags depuis le nom : modèles de lecture (`Artiste - Titre (Version)`, `01. Artiste - Titre` ou personnalisé), utiles pour les fichiers sans tags.
- Nom depuis les tags : le modèle de renommage ci-dessus.

## Recherche en ligne et autonomie

L'application identifie les morceaux et complète leurs informations toute seule quand elle est sûre d'elle. Les cas incertains vont dans une file « À valider ».

**Sources.** Chacune a son module, son cache et un interrupteur dans les réglages.

| Source | Sert à | Accès | Limite |
| --- | --- | --- | --- |
| AcoustID | reconnaître un morceau à son empreinte audio, même mal nommé | clé d'application gratuite pour un usage non commercial ; usage commercial payant via AcoustID OÜ ([source](https://acoustid.org)) | voir sa documentation |
| MusicBrainz | titre, artistes, version, album, année, label, ISRC | sans clé ; en-tête User-Agent obligatoire avec nom, version et contact de l'application | 1 requête par seconde ([source](https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting)) |
| Cover Art Archive | pochettes des sorties MusicBrainz | sans clé | voir sa documentation |
| Deezer | correspondance artiste et titre, album, année, BPM quand il existe, pochette | API publique de recherche, sans clé | à vérifier dans ses conditions |
| iTunes Search | pochettes haute définition, album, année, genre | sans clé | environ 20 appels par minute ([source](https://performance-partners.apple.com/search-api)) |

Avant de coder un module, vérifie les conditions d'utilisation de la source et résume-les dans `docs/SOURCES.md`. L'usage est personnel aujourd'hui, mais l'application pourra être partagée plus tard.

**Déroulé pour un morceau**

1. Lire les tags existants et le nom de fichier, puis appliquer les règles de nettoyage.
2. Calculer l'empreinte audio en local. Elle sert aussi aux doublons : elle n'est calculée qu'une fois.
3. Interroger AcoustID avec l'empreinte et la durée, pour obtenir des identifiants MusicBrainz candidats.
4. Interroger MusicBrainz sur ces candidats. Si l'empreinte n'est pas reconnue, chercher par artiste, titre et version.
5. Compléter avec Deezer puis iTunes : recherche par ISRC si on le connaît, sinon par artiste et titre. On en tire la pochette, l'album, l'année, le genre et le BPM.
6. Calculer un score de confiance pour chaque candidat, puis décider.

Pour la musique électronique pointue, MusicBrainz peut être lacunaire : la combinaison des sources compte plus que chacune prise seule.

**Score de confiance (0 à 100).** Il combine : le score de l'empreinte AcoustID ; la ressemblance du titre et de l'artiste principal, comparés après normalisation (minuscules, sans accents ni ponctuation) ; l'accord sur la version (Extended Mix et Radio Edit sont différents) ; l'écart de durée, pénalisé au-delà de 3 secondes ; l'accord entre plusieurs sources. Le détail du score est lisible dans la file, par exemple « empreinte 0,96 · titre identique · durée +1 s ».

**Décision.** Seuils réglables.

- **90 et plus : application automatique.** Les champs vides et non verrouillés sont remplis. Les valeurs déjà présentes ne sont remplacées que si l'utilisateur active « Remplacer aussi les valeurs existantes » ; sinon, ces remplacements vont dans la file. Une pochette manquante est ajoutée automatiquement.
- **De 60 à 89 : file « À valider ».**
- **Moins de 60 : « Non identifié »**, avec recherche manuelle proposée.

Toute application automatique est inscrite dans l'historique et s'annule comme le reste.

**File « À valider »**

- Pour chaque morceau : valeurs actuelles et proposées côte à côte, champ par champ, chacun avec sa case à cocher ; liste des candidats avec leur score et leur source ; pochettes proposées en grand.
- Raccourcis : Entrée accepte, Échap rejette, flèches haut et bas changent de morceau, touches 1 à 9 choisissent un candidat.
- Acceptation groupée de tous les morceaux d'un même album ou d'une même sortie.

**Recherche manuelle.** Un champ de recherche libre (artiste, titre) dans le panneau de détail interroge toutes les sources. Chaque résultat s'applique d'un clic, en entier ou champ par champ.

**Hors connexion**

- Un indicateur « En ligne / Hors ligne » est visible en permanence dans la barre du haut.
- Les recherches demandées hors connexion sont mises en file. Elles partent automatiquement au retour du réseau, puis une notification résume le résultat.
- Les réponses sont gardées en cache 30 jours et les pochettes téléchargées sont conservées : relancer une recherche déjà faite ne refait pas d'appel.
- Les limites de débit sont respectées strictement, avec reprise progressive après une erreur 429 ou 503.

**Réglages > Recherche en ligne.** Activer ou désactiver chaque source, saisir la clé AcoustID (avec un lien et une explication pas à pas pour l'obtenir), tester la connexion, régler les seuils.

## Pochettes

- **Sources :** pochette déjà intégrée au fichier ; image du dossier (`cover.jpg`, `folder.jpg`, `front.jpg`) ; Cover Art Archive ; Deezer ; iTunes ; fichier choisi sur l'ordinateur ; image collée depuis le presse-papiers ; image glissée depuis le navigateur ou l'explorateur de fichiers.
- **Choix :** grille des pochettes candidates avec dimensions et source. La plus grande et la plus carrée est présélectionnée.
- **Traitement automatique :** recadrage carré centré si l'image est presque carrée (écart de 10 % au plus), sinon ouverture de l'éditeur de recadrage. Redimensionnement à 1000 × 1000 pixels (réglable : 500, 600, 1000 ou 1400). JPEG qualité 85, 1 Mo au plus.
- **Éditeur :** recadrer, pivoter, comparer avant et après.
- **En lot :** appliquer une pochette à toute une sélection, par exemple tous les morceaux d'un album.
- **Export :** enregistrer la pochette en `cover.jpg` dans le dossier du morceau, sur demande.
- **Contrôle :** badge sur les morceaux sans pochette, avec une pochette trop petite (moins de 500 pixels) ou avec plusieurs images.
- **Rappel :** pour les WAV, Rekordbox n'affiche pas la pochette écrite dans le fichier (section « Formats et tags »).

## Doublons et quarantaine

**Ce qui compte comme doublon**

1. **Copie exacte :** même hash du flux audio, même si les tags ou le nom diffèrent.
2. **Même enregistrement dans un autre format ou une autre qualité**, par exemple le WAV et le MP3 320 d'un même morceau : empreintes audio très proches (comparées en local, sans internet) et durées à moins de 2 secondes d'écart. Les métadonnées normalisées (artiste, titre, version) présélectionnent les paires à comparer, pour rester rapide sur une grosse bibliothèque.

**Ce qui n'est pas un doublon :** deux versions différentes d'un même titre (Original Mix et Extended Mix, Radio Edit, remix). Si les versions diffèrent, les fichiers ne sont jamais regroupés, même avec des empreintes en partie proches. Si les versions sont inconnues et que les durées diffèrent de plus de 10 secondes, ils ne le sont pas non plus.

**Meilleure version.** Ordre par défaut, modifiable dans les réglages :

1. Sans perte authentique : WAV, puis AIFF, puis FLAC. À égalité, la résolution la plus haute (bits, fréquence).
2. Avec perte : par débit décroissant, MP3 320 kbit/s en tête.
3. Un fichier signalé « faux sans perte » ou « gonflé » (section « Dossiers, Inbox et contrôle qualité ») passe derrière un vrai MP3 320.
4. À égalité : les tags les plus complets, puis la présence d'une pochette, puis d'une note.

**Écran Doublons**

- Un groupe par morceau, sous forme de carte. Les fichiers sont côte à côte avec format, qualité, durée, taille, chemin, tags et pochette. Le fichier conservé porte le badge « À garder ».
- Écoute rapide de chaque fichier depuis la carte.
- Actions : changer le fichier gardé ; « Ce ne sont pas des doublons » (choix mémorisé) ; reporter la note, les tags et la pochette vers le fichier gardé avant d'écarter les autres ; traiter tous les groupes en lot, après aperçu.

**Quarantaine**

- Les fichiers écartés sont déplacés, et non copiés, dans un dossier `_Quarantaine` à la racine du même disque, en gardant leur arborescence d'origine. Le déplacement est instantané et ne consomme pas de place. Ce dossier est exclu de l'indexation.
- Écran Quarantaine : liste des fichiers avec leur date d'entrée, restauration à l'emplacement d'origine en un clic.
- « Vider la quarantaine » envoie les fichiers dans la corbeille du système, après une confirmation qui indique leur nombre et leur taille totale.
- Si un fichier écarté est présent dans Rekordbox, une alerte s'affiche avant le déplacement.

## Lecteur audio

Un lecteur simple, en bas de la fenêtre.

- **Commandes :** lecture/pause, précédent, suivant, volume, temps écoulé et durée totale.
- **Forme d'onde** sur toute la largeur. Un clic sur l'onde lance la lecture à cet endroit. La partie déjà jouée prend la couleur d'accent.
- **Cache :** les formes d'onde sont calculées en tâche de fond à la première écoute (ou dès l'indexation, si l'utilisateur l'active), puis gardées en cache. Elles s'affichent ensuite instantanément.
- **Formats :** tous les formats reconnus, y compris WAV 24 bits et FLAC haute résolution.
- **Raccourcis :** Espace pour lecture/pause ; double-clic ou Entrée sur un morceau pour le lancer ; flèche gauche ou droite pour reculer ou avancer de 10 secondes ; Maj + flèche gauche ou droite pour le morceau précédent ou suivant.
- **Édition pendant l'écoute :** modifier le morceau en cours de lecture ne coupe pas le son. Sous Windows, un fichier ouvert ne peut pas toujours être remplacé : l'écriture est alors différée à la fin de la lecture, avec un indicateur « en attente ».

## Dossiers, Inbox et contrôle qualité

**Gestion des dossiers** (panneau « Dossiers » de la barre latérale)

- Arborescence des racines de la bibliothèque, avec le nombre de morceaux par dossier.
- Créer, renommer, déplacer et fusionner des dossiers. Glisser des morceaux sur un dossier pour les y déplacer, après aperçu et confirmation.
- « Afficher dans l'Explorateur » (Windows) ou « Afficher dans le Finder » (macOS).
- Chaque opération est inscrite dans l'historique et s'annule. Rien n'est déplacé automatiquement (principe 9).

**Inbox (dossier d'arrivée)**

- L'utilisateur choisit un dossier d'arrivée, par exemple celui de ses téléchargements Beatport ou Bandcamp. Les nouveaux fichiers y sont détectés automatiquement.
- Chaque nouveau fichier suit la chaîne : indexation, nettoyage, identification en ligne, pochette, renommage selon le modèle. Les cas sûrs sont traités seuls ; les autres vont dans « À valider ».
- L'écran Inbox montre l'état de chaque morceau : en attente, traité, à valider, non identifié.
- Les morceaux prêts sont rangés à la main avec le bouton « Ranger… », qui suggère le dernier dossier utilisé. Une règle « Après validation, déplacer vers \[dossier\] » existe, désactivée par défaut.
- Un message d'aide explique qu'un morceau traité dans l'Inbox avant son import dans Rekordbox n'aura jamais de lien cassé.

**Contrôle qualité**

- **Débit faible :** MP3 et AAC sous 320 kbit/s signalés (seuil réglable).
- **Faux sans perte ou fichier gonflé :** une analyse spectrale repère la coupure nette des aigus typique d'un encodage avec perte (environ 16 kHz pour un 128 kbit/s, 19 à 20 kHz pour un 320). Le fichier est marqué « suspect » avec un spectrogramme consultable. Aucune action automatique.
- **Fichiers illisibles, corrompus ou tronqués.**
- **Fichiers manquants**, supprimés ou déplacés hors de l'application, avec une option pour les retrouver dans un dossier.
- **Cohérence :** tags WAV désynchronisés, noms non conformes au modèle, chemins trop longs, caractères interdits.

**Tableau de bord « Santé de la bibliothèque ».** Une liste de compteurs cliquables ; chacun ouvre la liste filtrée des morceaux concernés : sans pochette, pochette trop petite, sans année, sans genre, sans note, à valider, non identifiés, doublons, qualité suspecte, débit faible, noms non conformes, chemins trop longs, fichiers manquants ou illisibles. Un bouton « Corriger » apparaît quand une correction automatique existe, toujours avec aperçu.

## Intégration Rekordbox

Rekordbox garde sa propre base de données, d'où deux conséquences. Un fichier renommé ou déplacé après son import y apparaît comme manquant. Une modification de tags faite ailleurs n'y apparaît qu'après la commande « Reload Tag » ([source](https://forums.pioneerdj.com/hc/ja/community/posts/203068179/comments/203396569)).

Ne lis et n'écris jamais la base interne de Rekordbox : seul le format XML officiel est utilisé.

**Savoir quels morceaux sont dans Rekordbox**

- L'utilisateur exporte sa collection depuis Rekordbox (menu Fichier, « Export Collection in xml format ») puis l'importe dans Réglages > Rekordbox.
- L'attribut `Location` de chaque piste (de la forme `file://localhost/…`) est comparé aux fichiers indexés. Les morceaux retrouvés portent un badge « Rekordbox ».
- Si le fichier XML a plus de 7 jours, l'application propose de le réimporter.

**Mode protégé (activé par défaut)**

- Les morceaux badgés « Rekordbox » ne sont jamais renommés ni déplacés automatiquement.
- Un renommage ou déplacement manuel affiche d'abord une alerte claire : « Ce morceau est dans Rekordbox : il y apparaîtra comme manquant. »
- Leurs tags restent modifiables.
- Chaque renommage ou déplacement est consigné dans un journal exportable en CSV (ancien chemin ; nouveau chemin), pour aider à relocaliser les fichiers dans Rekordbox.

**Après une modification de tags**

- La liste « À recharger dans Rekordbox » recense les morceaux badgés dont les tags ont changé.
- Cette liste s'exporte en playlist M3U8. L'utilisateur l'importe dans Rekordbox, sélectionne tous ses morceaux et lance « Reload Tag ». Vérifie ce comportement sur une copie de bibliothèque Rekordbox ; s'il ne convient pas, passe par une playlist dans l'export XML.

**Export XML vers Rekordbox (Phase 4)**

- L'application génère un fichier XML au format Rekordbox. Pour les morceaux choisis, il contient : emplacement, titre, artiste, album, genre, année, tonalité (`Tonality`), remixeur, label, version (`Mix`) et note (`Rating`, valeurs 0, 51, 102, 153, 204 et 255 pour 0 à 5 étoiles, [source](https://pyrekordbox.readthedocs.io/en/stable/generated/pyrekordbox.rbxml.html)). Il contient aussi les playlists et filtres intelligents de l'application.
- L'utilisateur l'ouvre dans Rekordbox depuis le panneau Bridge, rubrique « rekordbox xml » ([source](https://rekordbox.com/en/support/developer)). L'aide décrit chaque étape, avec captures d'écran.
- Teste sur une copie de bibliothèque Rekordbox ce qui se passe pour les morceaux déjà présents dans sa collection (la note est-elle mise à jour ?), puis documente-le. L'aide recommande de sauvegarder la bibliothèque Rekordbox avant tout import.

**WAV et Rekordbox**

- Rekordbox ne lit dans un WAV que les 5 champs RIFF INFO, et aucune pochette ([source](https://www.lexicondj.com/manual/all)). L'application l'indique clairement par un badge sur les WAV.
- Option de la Phase 4 : convertir des WAV en AIFF. Les échantillons audio restent identiques au bit près, seul le conteneur change. L'AIFF porte tous les tags et la pochette que Rekordbox sait lire.
- Cette conversion n'est lancée qu'à la demande. L'application vérifie que l'audio décodé est identique, garde le WAV d'origine en quarantaine jusqu'à validation, et prévient si le WAV est déjà dans Rekordbox : l'AIFF y sera un nouveau morceau.

## Interface et UX

Une seule fenêtre, cinq zones. Tout se fait au clavier ou à la souris, sans jamais ouvrir de fenêtre superflue.

&#91;embedded content: fenêtre principale · 5 zones\]

La couleur d'accent marque trois choses seulement : la ligne sélectionnée, les écrans qui ont des éléments à traiter et la partie déjà jouée du morceau.

**Zones de la fenêtre**

- **Barre latérale gauche** (repliable) : Bibliothèque ; Inbox, À valider et Doublons avec un badge chiffré ; Santé de la bibliothèque ; Quarantaine ; Playlists et filtres intelligents ; Dossiers (arborescence).
- **Barre du haut :** recherche instantanée sur tous les champs, avec filtres rapides (format, sans pochette, note minimale…) ; boutons « Rechercher en ligne » et « Renommer » pour la sélection ; indicateur En ligne / Hors ligne avec la file d'attente ; accès au panneau des tâches en cours.
- **Centre :** tableau des morceaux, dense et lisible. Colonnes affichables et réordonnables : pochette miniature, note, artiste principal, titre, version, artistes, album, année, genre, BPM, tonalité, format, qualité, durée, nom de fichier, état. L'état regroupe des badges : Rekordbox, verrouillé, à valider, suspect, WAV. Tri sur plusieurs colonnes ; largeurs et ordre mémorisés.
- **Panneau de détail à droite** (repliable) : grande pochette (déposer, coller, rechercher) ; champs modifiables avec leur cadenas ; artistes en étiquettes réordonnables, une étoile marquant l'artiste principal ; note en étoiles ; aperçu du futur nom de fichier ; informations techniques (format, débit, fréquence, bits, taille, chemin) ; origine de chaque valeur (saisie manuelle, fichier, MusicBrainz…).
- **Lecteur en bas :** pochette miniature, titre, commandes, forme d'onde cliquable sur toute la largeur.

**Édition**

- Clic sur une cellule déjà sélectionnée, ou F2 : édition sur place. Tab passe au champ suivant, Entrée valide, Échap annule.
- Sélection multiple : le panneau de détail modifie tous les morceaux à la fois. Un champ aux valeurs différentes affiche « Valeurs multiples » et n'est changé que si l'utilisateur y touche.
- Rechercher/remplacer dans la sélection, échanger artiste et titre, lire les tags depuis le nom de fichier.
- Copier les tags d'un morceau et les coller sur d'autres.

**Retours à l'utilisateur**

- Après chaque action, une notification discrète avec un bouton « Annuler ».
- Les opérations longues s'affichent dans le panneau Tâches avec progression, temps restant estimé et bouton Annuler.
- Messages d'erreur en français simple, avec la cause et la solution. Par exemple : « Le disque MUSIQUE est en lecture seule. Vérifiez qu'il n'est pas protégé en écriture. »
- États vides utiles : une bibliothèque vide propose « Ajouter un dossier » ; une file À valider vide affiche « Tout est à jour ».

**Premier lancement** (3 écrans) : langue et thème ; dossier de bibliothèque et dossier Inbox ; recherche en ligne (clé AcoustID facultative, avec explication) et conseils d'usage avec Rekordbox.

**Palette de commandes :** Ctrl+K sous Windows, Cmd+K sous macOS. Elle cherche dans toutes les actions et tous les morceaux.

**Glisser-déposer :** fichiers ou dossiers depuis l'explorateur vers l'application pour les ajouter ; image vers la pochette ; morceaux vers un dossier (déplacement après confirmation) ou vers une playlist.

**Raccourcis principaux.** Cmd remplace Ctrl sur macOS. Tous sont listés et modifiables dans Réglages > Raccourcis. Désactive les raccourcis propres au navigateur intégré (rechargement de page, zoom), qui entreraient en conflit.

| Action | Raccourci |
| --- | --- |
| Palette de commandes | Ctrl+K |
| Rechercher dans la bibliothèque | Ctrl+F |
| Lecture / pause | Espace |
| Modifier la cellule | F2 |
| Noter de 0 à 5 étoiles | touches 0 à 5 |
| Rechercher en ligne la sélection | Ctrl+L |
| Renommer la sélection | Ctrl+R |
| Annuler / rétablir | Ctrl+Z / Ctrl+Maj+Z |
| Afficher le fichier dans l'Explorateur ou le Finder | Ctrl+Maj+R |
| Afficher ou masquer le panneau de détail | Ctrl+I |

**Accessibilité :** navigation complète au clavier avec focus visible ; contrastes conformes WCAG AA ; taille du texte réglable de 90 à 130 % ; aucune information portée par la seule couleur.

## Design et langues

**Direction :** minimaliste, moderne et calme. L'outil s'efface derrière la musique : beaucoup d'espace, peu de bordures, une seule couleur d'accent. La couleur vient des pochettes.

**Thèmes**

- Thème clair par défaut, thème sombre disponible, option « Suivre le système ».
- Couleurs personnalisables dans Réglages > Apparence : couleur d'accent (8 préréglages et un sélecteur libre) ; fond, surfaces et texte modifiables pour aller plus loin ; bouton « Rétablir les couleurs par défaut ». Le contraste est vérifié en direct, avec une alerte si un choix rend le texte illisible.
- Densité du tableau : compacte ou confortable.
- Tout repose sur des jetons de design (variables CSS) : couleurs, espacements, rayons, typographie. Aucune couleur n'est écrite en dur dans un composant.

**Palette de départ**

| Jeton | Thème clair | Thème sombre | Usage |
| --- | --- | --- | --- |
| Fond | #FAFAF9 | #0C0A09 | fond de la fenêtre |
| Surface | #FFFFFF | #1C1917 | tableau, panneaux |
| Bordure | #E7E5E4 | #292524 | séparations fines |
| Texte | #1C1917 | #FAFAF9 | texte principal |
| Texte secondaire | #78716C | #A8A29E | libellés, métadonnées |
| Accent | #4F46E5 | #818CF8 | sélection, onde déjà jouée, étoiles, bouton principal |
| Succès | #16A34A | #4ADE80 | états seulement |
| Attention | #D97706 | #FBBF24 | états seulement |
| Erreur | #DC2626 | #F87171 | états seulement |

**Typographie :** police du système pour un rendu natif (Segoe UI Variable sous Windows, SF Pro sous macOS). Chiffres à chasse fixe pour les durées, BPM et tailles. Tailles : 13 px dans le tableau, 14 px dans les panneaux, 20 px pour les titres d'écran.

**Formes et mouvement :** grille de 4 px ; rayons de 6 px pour les champs et 10 px pour les cartes ; ombres réservées aux menus flottants ; transitions de 150 ms, coupées si le système demande de réduire les animations.

**Icônes :** Lucide, trait fin, 16 px dans le tableau et 20 px ailleurs.

**Fenêtre :** barre de titre intégrée, avec les boutons natifs de chaque système (à gauche sur macOS, à droite sur Windows). Taille et position mémorisées.

**Langues**

- Français par défaut, anglais disponible. Le changement dans Réglages > Langue est immédiat, sans redémarrage.
- Tous les textes sont dans des fichiers de traduction (`fr.json`, `en.json`), aucun en dur dans le code. Dates, nombres et durées suivent le format de la langue choisie.
- Un test automatique échoue si une clé de traduction manque dans l'une des langues.

## Plan de réalisation

Cinq phases, chacune livrée sous forme d'une version installée et testée par l'utilisateur avant la suivante. Une phase n'est terminée que si tous ses critères sont remplis, et elle se clôt par un rapport en français.

&#91;embedded content: plan de réalisation · 5 phases, 4 validations\]

À chaque validation, l'utilisateur installe la version, fait les tests de la phase et coche ses critères avant que la suivante ne commence.

### Phase 0 — Fondations

Livrables : dépôt structuré (cœur Rust et interface) ; compilation automatique Windows et macOS avec installateurs ; jetons de design clair et sombre ; traductions français et anglais ; base SQLite avec migrations ; journal d'erreurs local ; fenêtre avec ses cinq zones vides ; `README.md` en français.

- [ ] Les installateurs Windows et macOS se téléchargent depuis GitHub et l'application s'ouvre sur les deux systèmes.
- [ ] Les bascules clair/sombre et français/anglais fonctionnent sans redémarrage.
- [ ] La compilation automatique et les tests sont au vert.

### Phase 1 — MVP : bibliothèque et édition hors connexion

Livrables : dossiers de bibliothèque et disque externe (reconnexion) ; indexation et surveillance ; tableau virtualisé avec recherche, tri et filtres ; panneau de détail ; édition d'un ou plusieurs morceaux ; lecture et écriture des tags WAV (RIFF INFO et ID3), MP3, FLAC, AIFF et M4A ; note ; pochette depuis un fichier, le presse-papiers ou un glisser-déposer ; renommage par modèle avec aperçu ; nettoyage ; nom de fichier vers tags ; champs verrouillés ; écriture sûre ; historique et annulation ; lecteur avec forme d'onde cliquable ; gestion des dossiers ; premier lancement.

- [ ] 1 000 fichiers de test s'indexent ; un tableau de 100 000 entrées simulées défile sans saccade.
- [ ] Sur un WAV, un MP3 et un FLAC : titre, artistes, version, album, année, genre, BPM, tonalité, note et pochette modifiés, puis relus correctement par l'application et par un autre logiciel. Pour le WAV, Rekordbox affiche bien titre, artiste, album et genre.
- [ ] Le hash du flux audio est identique avant et après chaque écriture (test automatique sur chaque format).
- [ ] Les tags inconnus (données Serato, Rekordbox, Beatport) sont toujours présents après écriture.
- [ ] 50 fichiers renommés après aperçu, puis tout annulé : les noms d'origine reviennent.
- [ ] Disque débranché : morceaux grisés, aucun plantage. Rebranché sous une autre lettre : reconnexion automatique.
- [ ] Un clic sur la forme d'onde déplace la lecture à cet endroit.
- [ ] Tout ce qui précède fonctionne réseau coupé.

### Phase 2 — Recherche en ligne et Inbox

Livrables : empreintes audio ; modules AcoustID, MusicBrainz, Cover Art Archive, Deezer et iTunes ; score de confiance ; application automatique ; file « À valider » ; recherche manuelle ; cache et file hors connexion ; pochettes (grille de choix, recadrage, redimensionnement) ; Inbox ; palette de commandes ; réglages des sources et clés dans le trousseau ; `docs/SOURCES.md`.

- [ ] Sur un échantillon de 100 morceaux mal tagués fourni par l'utilisateur, aucune information fausse n'est appliquée automatiquement. Le taux d'identification est mesuré et rapporté.
- [ ] Une Original Mix et une Extended Mix du même titre ne sont jamais confondues.
- [ ] Une recherche lancée hors connexion part toute seule au retour du réseau.
- [ ] Le journal des appels prouve le respect des limites de débit.
- [ ] Un fichier reconnu avec certitude et déposé dans l'Inbox est tagué, pourvu d'une pochette et renommé sans intervention.
- [ ] Un champ verrouillé n'est jamais modifié par l'automatisation.

### Phase 3 — Doublons et qualité

Livrables : doublons exacts et par empreinte ; classement de la meilleure version ; écran Doublons ; report des métadonnées ; quarantaine, restauration et corbeille ; contrôle qualité (débit, analyse spectrale, fichiers corrompus ou manquants) ; tableau de bord « Santé de la bibliothèque ».

- [ ] Le WAV et le MP3 d'un même morceau sont groupés, et le WAV est proposé à garder.
- [ ] Une Original Mix et une Extended Mix ne sont jamais groupées.
- [ ] Un fichier mis en quarantaine puis restauré revient à sa place, identique.
- [ ] Un MP3 128 kbit/s converti en WAV ou en FLAC est marqué « suspect ».
- [ ] Les compteurs du tableau de bord sont justes et ouvrent les bonnes listes.

### Phase 4 — Rekordbox et finitions

Livrables : import du XML Rekordbox, badges et mode protégé ; liste « À recharger » et export M3U8 ; journal CSV des renommages ; export XML vers Rekordbox (notes, playlists) ; playlists et filtres intelligents ; export M3U ; conversion WAV vers AIFF optionnelle ; mise à jour automatique de l'application ; guide utilisateur complet en français ; signature officielle prête à activer.

- [ ] Sur une copie de bibliothèque Rekordbox, les morceaux sont reconnus et badgés. Renommer un morceau badgé déclenche l'alerte.
- [ ] L'export XML s'ouvre dans Rekordbox (panneau Bridge) et montre notes et playlists.
- [ ] Après conversion WAV vers AIFF, l'audio décodé est identique et la pochette s'affiche dans Rekordbox.
- [ ] Le guide utilisateur explique chaque fonction, avec captures d'écran.

### Phase 5 — Veille musicale et liste d'envies

Ajoutée à la demande de l'utilisateur le 27/11/2026. Un écran « Sorties » (d'abord appelé « Veille ») consulte des classements et des nouvelles sorties sur internet, et permet de garder les titres repérés dans une « liste d'envies ». L'application ne télécharge jamais de musique (voir « Hors périmètre ») : elle donne des liens vers les boutiques, et peut ajouter les titres à une playlist d'un compte de streaming lié.

**Sources gratuites, sans compte** (chacune désactivée par défaut, activable dans les réglages, comme la recherche en ligne) :
- **Classements par pays** : flux publics d'Apple Music (morceaux et albums les plus écoutés, par pays, mis à jour chaque jour).
- **Classements par genre** : classements publics de Deezer (pays déduit par Deezer de la connexion).
- **Nouvelles sorties par label et par pays** : recherche MusicBrainz (sorties d'un label ou d'un pays sur une période : 7, 30 ou 90 jours). Liste des grands labels proposée par défaut ; labels favoris et labels ajoutés dans les réglages ; « Tous mes favoris » cherche tous les favoris en une seule requête.
- Les réponses sont gardées en cache 12 heures (classements Apple Music) ou 24 heures (sorties MusicBrainz) ; jamais pour Deezer, dont les conditions interdisent la copie locale. Seuls le pays, le genre ou le nom du label sont envoyés ; rien sur la bibliothèque ni sur l'utilisateur.

**Écoute et ouverture** (ajoutées à la demande de l'utilisateur) : bouton ▶ sur la pochette qui joue un extrait de 30 secondes dans le lecteur du bas de l'application (précédent, suivant, enchaînement dans la liste affichée) ; recherche de l'extrait sur Deezer et Apple Music (le service choisi d'abord, puis l'autre) ; extrait gardé en mémoire seulement ; bouton ↗ qui ouvre le titre sur le site choisi dans les réglages (source, Apple Music, Deezer, Spotify, YouTube, YouTube Music, SoundCloud, Beatport).

**Liste d'envies** :
- « Ajouter » depuis un classement ou une sortie : titre, artistes, version, label, date de sortie, pochette, source et lien.
- États : à trouver, trouvé (dans la bibliothèque), ignoré. Un morceau de la liste qui arrive dans la bibliothèque (Inbox ou scan) est reconnu par artiste et titre et passe à « trouvé » tout seul.
- Liens de recherche vers Beatport, Bandcamp, Juno Download, Traxsource et la page de la source (rien n'est acheté par l'application).
- Export CSV (séparateur « ; », ouvrable dans Excel). Suppression avec annulation.
- Le badge « dans la bibliothèque » s'affiche aussi sur les classements et les sorties.

**Comptes liés (optionnels)** : un compte lié débloque « Ajouter à une playlist » de ce compte (playlist existante ou nouvelle), un titre ou toute la liste d'envies.
- **Spotify** en premier : l'utilisateur crée gratuitement une « application » sur le site développeur de Spotify (abonnement Premium exigé par Spotify, 5 comptes au plus) et colle son identifiant client ; connexion par le navigateur (OAuth 2 avec PKCE, retour sur `http://127.0.0.1`, aucun mot de passe ni clé secrète dans l'application) ; le jeton est rangé dans le trousseau du système ; « Déconnecter » l'efface. Recherche du titre (ISRC si connu, sinon artiste et titre), aperçu des correspondances, puis ajout.
- **Tidal** ensuite, si ses conditions d'accès le permettent (à vérifier pendant la phase). **Deezer** (création d'applications fermée), **Apple Music** (abonnement développeur payant), **Beatport** et **Shazam** (pas d'accès public) : non pris en charge ; l'aide l'explique.

Critères de validation :
- [ ] Le classement Apple Music de la France et d'un autre pays s'affiche ; un titre déjà dans la bibliothèque porte le badge.
- [ ] Les nouvelles sorties d'un label choisi (30 derniers jours) s'affichent.
- [ ] Un titre ajouté à la liste d'envies y reste après redémarrage ; quand son fichier arrive dans l'Inbox, il passe à « trouvé ».
- [ ] Sans connexion, l'écran Veille l'indique et la liste d'envies reste utilisable.
- [ ] Avec un compte Spotify lié, un titre de la liste d'envies est ajouté à une playlist Spotify choisie ; « Déconnecter » efface le jeton.

### Phase 6 — Outils complets du DJ

Demandée par l'utilisateur le 27/11/2026 (« une seule grosse phase, même si non utilisé, pratique au besoin »). Mêmes principes que le reste : le son n'est jamais modifié (sauf la conversion WAV → AIFF déjà prévue), aperçu avant toute action en lot, annulation, rien d'automatique sans réglage.

1. **Préparation de sets** : écran « Sets ». Un set est une liste ordonnée de morceaux (monter, descendre, retirer, ajouter depuis la sélection). Pour chaque enchaînement : compatibilité de tonalité (roue Camelot : même tonalité, ±1, passage A/B) et écart de BPM (en %). **Suggestions du morceau suivant** dans la bibliothèque : tonalité compatible, BPM à ±6 %, même genre en priorité, morceaux pas encore dans le set. Durée totale. Export M3U8 et vers Rekordbox (XML) comme une playlist.
2. **Étiquettes personnelles** (ambiance, événement : « mariage », « cocktail », « dancefloor », « version clean », « explicite »…) : création, couleur, ajout et retrait en lot depuis la sélection, affichage dans le panneau de détail, filtre de la bibliothèque et règle des filtres intelligents. Gardées dans la base de l'application (pas écrites dans les fichiers).
3. **Historique de jeu** : import des playlists d'historique de Rekordbox (dans le XML exporté, ou une playlist d'historique exportée en M3U8 / TXT). Pour chaque morceau : nombre de passages et date du dernier passage (colonnes, filtres « jamais joué », « joué ces 30 derniers jours »). Le lecteur de l'application ne compte pas comme un passage.
4. **Sauvegarde de la bibliothèque** : copie vers un dossier de secours (disque externe), en ne copiant que ce qui a changé ; chaque copie est vérifiée (empreinte du fichier) ; rien n'est jamais effacé dans la sauvegarde ; tâche de fond annulable ; rappel au démarrage si la dernière sauvegarde a plus de N jours (réglage, désactivé par défaut).
5. **Copie vers une clé USB** : la sélection ou une playlist / un set vers un dossier choisi, copie vérifiée, rangement au choix (à plat, par playlist, ou arborescence d'origine), playlist M3U8 à chemins relatifs sur la clé. Les fichiers de la bibliothèque ne sont jamais déplacés.
6. **Volume perçu (LUFS)** : mesure EBU R128 (volume intégré et crête vraie) de chaque morceau, colonne « LUFS », compteurs « trop fort » / « trop faible » dans Santé de la bibliothèque. Mesure seulement : le son n'est pas modifié.
7. **Analyse BPM, tonalité et énergie** (ajoutée au périmètre à la demande de l'utilisateur) : pour les morceaux sans BPM ou sans tonalité, analyse sur l'ordinateur, aperçu des résultats, écriture dans les tags en modification automatique (champs verrouillés jamais remplacés, annulable). Énergie de 1 à 10 gardée dans la base. Les résultats de Rekordbox restent la référence : l'analyse ne remplace jamais une valeur existante.
8. **Statistiques** : écran avec la répartition par genre, année, format et qualité, la place occupée (par disque), les morceaux ajoutés par mois, les plus joués.
9. **Améliorations** : réparation des dates ID3 abîmées (aperçu, annulable) ; vignettes des pochettes dans le tableau (colonne optionnelle) ; ajout de morceaux à une playlist par glisser-déposer et réordonnancement des playlists ; installation automatique des mises à jour (préparée, active quand la clé de signature des mises à jour sera créée, voir docs/SIGNATURE.md) ; compte Tidal lié si son accès le permet (sinon noté avec la raison).

Critères de validation :
- [ ] Un set de 10 morceaux montre les enchaînements compatibles et propose des suivants ; il s'exporte et s'ouvre dans Rekordbox.
- [ ] Une étiquette posée sur 20 morceaux sert de filtre et de règle de filtre intelligent.
- [ ] Après import de l'historique Rekordbox, les morceaux joués ont leur nombre de passages ; « jamais joué » donne la bonne liste.
- [ ] Une sauvegarde complète puis une seconde ne recopie que les fichiers modifiés ; les copies sont identiques aux originaux.
- [ ] Une playlist copiée sur une clé USB se lit dans un lecteur avec son M3U8.
- [ ] Le volume LUFS d'un morceau est proche de celui mesuré par un autre logiciel (±1 LU).
- [ ] L'analyse BPM / tonalité d'un morceau sans valeur donne un résultat cohérent avec Rekordbox dans la plupart des cas, et ne touche pas un morceau qui en a déjà.

### Phase 7 — Remaster

Demandée par l'utilisateur le 28/11/2026. Améliorer le son d'un titre (volume, équilibre, densité, image stéréo), à la main ou automatiquement, et l'exporter dans un **nouveau fichier WAV**. **L'original n'est jamais modifié** (le principe « le son n'est jamais modifié » est respecté : on crée une copie traitée, comme la conversion WAV → AIFF crée un nouveau fichier).

**Chaîne de traitement** (dans cet ordre, chaque étage activable) :
- **Filtre passe-haut** (coupe les infra-graves inutiles, 20 à 40 Hz).
- **Égaliseur** : 5 bandes réglables (graves en plateau, bas-médiums, médiums, hauts-médiums, aigus en plateau), ±12 dB.
- **Égalisation mono / stéréo** : largeur stéréo (0 % = mono, 100 % = d'origine, jusqu'à 150 %), **graves en mono** sous une fréquence choisie (utile en club), mono complet en option.
- **Compresseur** : seuil, taux, attaque, relâchement, gain de compensation.
- **Volume cible** en LUFS (par exemple −8 pour le club), atteint automatiquement.
- **Limiteur** à anticipation : plafond de crête (−1 dB par défaut), jamais de saturation numérique.

**Mode automatique** : analyse du titre (volume, équilibre graves / médiums / aigus comparé à une courbe de référence « club », dynamique) puis réglages doux et bornés (égaliseur à ±4 dB au plus, compression légère, graves en mono sous 120 Hz, volume cible, limiteur). **Préréglages** : Automatique, Club fort, Chaud, Brillant, Punch, Doux.

**Écoute** : dans le lecteur du bas, avant / après **au même volume perçu** (bascule A/B), pour juger honnêtement. Les réglages changent le son en direct. Spectre affiché avant / après.

**Export** : WAV 24 bits, même fréquence d'échantillonnage que l'original, à côté de l'original sous le nom « … (Remaster).wav » (jamais d'écrasement), tags et pochette recopiés, version complétée de « Remaster » (le titre n'est donc pas regroupé avec l'original dans les doublons), ajouté à la bibliothèque. Les réglages utilisés sont gardés pour pouvoir refaire l'export. Traitement en lot (mode automatique ou préréglage) sur une sélection, en tâche de fond annulable.

**Ajouts du 28/11/2026 (version 0.8.1)** : curseurs plus précis avec saisie chiffrée ; **préréglages personnels** enregistrés (son + fondus, sans la coupe), utilisables en lot ; **coupe du début et de la fin** au dixième de seconde et **fondus d'entrée et de sortie** optionnels en secondes, entendus à l'écoute et appliqués à l'export.

**Limites, dites dans l'aide** : on ne recrée pas les aigus perdus d'un MP3 basse qualité ; l'outil améliore le volume, l'équilibre, la densité et l'image stéréo. Pas de restauration par IA.

Critères de validation :
- [ ] Un titre traité en mode automatique atteint le volume cible à ±0,5 LU et ne dépasse jamais le plafond de crête.
- [ ] La bascule A/B compare au même volume perçu.
- [ ] L'export crée un nouveau WAV avec tags et pochette ; l'original est identique octet pour octet.
- [ ] Avec « graves en mono », les graves sous la fréquence choisie sont identiques sur les deux canaux.
- [ ] Le traitement en lot de 10 titres se termine, s'annule proprement et chaque export est dans la bibliothèque.
- [ ] Une coupe de 12,3 s à 3:00,0 donne un fichier de 2:47,7 exactement ; les fondus partent et finissent au silence.
- [ ] Un préréglage enregistré réapparaît après redémarrage et redonne exactement les mêmes réglages.

## Tests, qualité et documentation

**Tests automatiques**, lancés par la CI à chaque modification :

- Un jeu de fichiers de test courts (quelques secondes de son généré, libre de droits) pour chaque format : avec et sans tags, avec des données Rekordbox, Serato et Beatport simulées, avec accents et noms longs.
- Aller-retour des tags : écrire puis relire chaque champ, pour chaque format ; tags inconnus préservés ; hash audio inchangé.
- WAV : RIFF INFO et ID3 identiques après chaque écriture.
- Renommage : modèles, caractères interdits, noms réservés, normalisation NFC, longueurs, conflits.
- Nettoyage : chaque règle, avec des cas pièges (« Above & Beyond », parenthèses qui font partie du titre).
- Score de confiance et décision : cas typiques et versions différentes.
- Doublons : copies exactes, formats différents, versions différentes.
- Historique : annulation d'un lot de renommages, de tags ou de mises en quarantaine, y compris après redémarrage.
- Sources en ligne testées avec des réponses enregistrées : aucun appel réel pendant la CI.
- Traductions complètes en français et en anglais.
- Performance : tableau de 100 000 entrées simulées.

**Robustesse.** Processus tué au milieu d'une écriture : l'original reste intact. Disque plein, disque en lecture seule, fichier verrouillé, disque débranché pendant une opération : message clair et aucune perte.

**Journal.** Fichier de log local avec rotation. Un bouton « Copier le rapport de diagnostic » permet de signaler un problème.

**Documentation dans le dépôt**

- `README.md` : installation et premier lancement, en français.
- `docs/GUIDE_UTILISATEUR.md` : chaque fonction pas à pas avec captures, complété à chaque phase.
- `docs/DECISIONS.md`, `docs/SOURCES.md`, `docs/IDEES.md`, `THIRD_PARTY_LICENSES.md`, `CHANGELOG.md`.

**Rapport de fin de phase**, en français simple : ce qui est fait ; ce qui ne l'est pas et pourquoi ; liste numérotée des tests que l'utilisateur doit faire à la main ; problèmes connus ; numéro de version publiée et lien de téléchargement.

## Hors périmètre et points à confirmer

**Hors périmètre.** Ne pas développer sans demande explicite :

- grilles de temps et cue points (Rekordbox s'en charge) ; l'analyse du BPM et de la tonalité est ajoutée en phase 6, seulement pour les morceaux qui n'en ont pas ;
- lecture ou écriture de la base interne de Rekordbox ;
- mixage, effets, pré-écoute sur une sortie casque séparée ;
- streaming (Beatport, Tidal…) et téléchargement de musique (la phase 5 donne seulement des liens vers les boutiques et ajoute des titres aux playlists d'un compte lié) ;
- synchronisation avec Serato, Traktor ou Engine DJ (envisageable plus tard) ;
- version mobile.

**Points à confirmer par l'utilisateur.** Tant qu'un point n'est pas tranché, Codex applique la valeur par défaut indiquée.

- [ ] Nom définitif de l'application. Par défaut : `dj-library-manager`.
- [ ] Titre écrit dans les fichiers : « Titre (Version) » ou titre seul. Par défaut : « Titre (Version) ».
- [ ] Séparateur des artistes dans le tag : « , », « & » ou « ; ». Par défaut : « , ».
- [ ] Quand aucune version n'est connue, écrire « (Original Mix) » ou rien. Par défaut : rien.
- [ ] Conversion des WAV en AIFF pour voir les pochettes dans Rekordbox. Par défaut : proposée en option, jamais automatique.
- [ ] Taille des pochettes. Par défaut : 1000 × 1000 pixels.
- [ ] Création d'une clé AcoustID gratuite, nécessaire à la reconnaissance par empreinte.

## Sources

- [Champs lus par Rekordbox selon le format (Pioneer DJ)](https://forums.pioneerdj.com/hc/en-us/articles/205021569-rekordbox-metadata)
- [Rekordbox et les pochettes des WAV (manuel Lexicon DJ)](https://www.lexicondj.com/manual/all)
- [Formats lus par Rekordbox et commande Reload Tag (VibesDJ)](https://vibesdj.io/how-to/add-album-artwork-in-rekordbox)
- [Type de pochette attendu par Rekordbox (forum Pioneer DJ)](https://forums.pioneerdj.com/hc/fr/community/posts/900003237143-Album-Art-missing)
- [Rechargement des tags dans Rekordbox (forum Pioneer DJ)](https://forums.pioneerdj.com/hc/ja/community/posts/203068179/comments/203396569)
- [Limite de 255 caractères des chemins sous macOS (rekordbox-repair)](https://github.com/edkennard/rekordbox-repair)
- [Format XML Rekordbox et échelle des notes (pyrekordbox)](https://pyrekordbox.readthedocs.io/en/stable/generated/pyrekordbox.rbxml.html)
- [Import XML dans Rekordbox (rekordbox for Developers)](https://rekordbox.com/en/support/developer)
- [Pochettes multiples et Engine DJ (forum Engine DJ)](https://community.enginedj.com/t/some-cover-art-not-displaying/29299)
- [Conditions d'AcoustID](https://acoustid.org)
- [Limites de l'API MusicBrainz](https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting)
- [Limites de l'API iTunes Search (Apple)](https://performance-partners.apple.com/search-api)
- [Formats pris en charge par lofty](https://docs.rs/crate/lofty/latest)
- [Écriture ID3v2.3 avec lofty](https://docs.rs/crate/lofty/0.24.0/source/src/config/write_options.rs)
- [rusty-chromaprint (licence MIT)](https://lib.rs/crates/rusty-chromaprint)
- [Tauri 2.12](https://v2.tauri.app/release/tauri/v2.12.0/)
- [Spotify : changements de février 2026 (mode développement)](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide)
- [Spotify : connexion PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow)
- [Spotify : adresses de retour (127.0.0.1 autorisé, localhost refusé)](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri)
