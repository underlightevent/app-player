# Journal des versions

## 0.6.1 — Correction de la compilation (à valider)

- Même contenu que la 0.6.0. Un test de la quarantaine comparait les chemins comme du texte et échouait sur les machines de compilation de GitHub (lien /var sous macOS, nom court sous Windows). L'application n'était pas en cause.

## 0.6.0 — Phase 5 : veille musicale (à valider, avec les phases 3 et 4)

- Écran Sorties : classements par pays (Apple Music) et par genre (Deezer), nouvelles sorties d'un label ou d'un pays (MusicBrainz). Sources désactivées par défaut.
- Liste d'envies : ajout depuis les classements et les sorties, « trouvé » automatique quand le morceau arrive dans la bibliothèque, liens de recherche vers Beatport, Bandcamp, Juno et Traxsource, export CSV.
- Compte Spotify lié (optionnel) : ajout des titres de la liste d'envies à une playlist Spotify, avec aperçu des correspondances.
- Écoute d'extraits de 30 secondes dans le lecteur du bas, en cliquant sur la pochette ; recherche automatique sur Deezer puis Apple Music (ou l'inverse).
- Liste des grands labels, labels favoris et « Tous mes favoris » ; ajout de labels dans les réglages.
- L'écran « Veille » s'appelle maintenant « Sorties ».
- Réglage du site d'ouverture des titres : source, Apple Music, Deezer, Spotify, YouTube, YouTube Music, SoundCloud ou Beatport.

## 0.5.0 — Phase 4 : Rekordbox et finitions (à valider, avec la phase 3)

- Rekordbox, uniquement par le fichier XML officiel : import de la collection, badge « RB » sur les morceaux retrouvés, rappel si l'import a plus de 7 jours.
- Mode protégé (activé par défaut) : pas de renommage automatique d'un morceau présent dans Rekordbox ; alerte avant tout renommage, déplacement, mise en quarantaine ou conversion manuel.
- Liste « À recharger dans Rekordbox » (tags modifiés) et export M3U8 pour « Reload Tag ».
- Journal des chemins (ancien ; nouveau) exportable en CSV.
- Export XML vers Rekordbox : notes, tags, version, tonalité, label, remixeur et playlists.
- Playlists et filtres intelligents (règles sur genre, note, BPM, année, format, présence dans Rekordbox…), export M3U.
- Conversion WAV vers AIFF à la demande : son identique vérifié échantillon par échantillon, tags et pochette recopiés, WAV d'origine en quarantaine, annulation en une fois.
- Vérification des mises à jour au démarrage (désactivable) avec lien de téléchargement.
- Guide utilisateur complet, avec captures d'écran.
- Réglage « Titre écrit dans les fichiers » : titre seul, titre et version, artiste principal et titre, ou artiste principal, titre et version (pour les platines qui n'affichent que le titre), avec « Appliquer aux fichiers existants… » annulable.
- Correction : des MP3 lisibles partout étaient marqués « illisible » (tag APE de fin de fichier abîmé, ou octets vides entre les tags et le son). Ils sont maintenant lus normalement, et leurs tags peuvent être modifiés sans toucher à la fin du fichier ni au son. Les fichiers illisibles sont relus à chaque indexation.
- Correction : des MP3 avec un gros tag (grandes pochettes) étaient « illisibles » et muets, ou décodés de travers. Le tag est maintenant sauté avant la lecture du son. Les empreintes des MP3 sont recalculées au premier démarrage ; le contrôle qualité des MP3 est à relancer.
- Correction : modifier le titre d'un morceau dont le titre commence par l'artiste (« Artiste - Titre ») échouait avec « la vérification après écriture a échoué ». L'artiste n'est plus ajouté deux fois par « Appliquer aux fichiers existants ». Tous les fichiers sont relus une fois au premier démarrage.
- Bibliothèque : menu « Trier par » (dont « Note : ★★★★★ d'abord ») ; la colonne Note trie les mieux notés en premier.

## 0.4.0 — Phase 3 : doublons et qualité (à valider)

- Doublons : copies exactes (même son) et même morceau dans un autre format ou une autre qualité (empreintes comparées sur l'ordinateur, durées à 2 s près). Deux versions différentes ne sont jamais regroupées.
- Écran Doublons : fichiers côte à côte (format, qualité, durée, taille, emplacement, tags, pochette, écoute), badge « À garder », meilleure version proposée (WAV > AIFF > FLAC, puis débit ; un fichier suspect passe derrière un vrai MP3 320), « Ce ne sont pas des doublons » mémorisé, report de la note, des tags et de la pochette vers le fichier gardé, traitement en lot après aperçu, une seule annulation pour tout.
- Quarantaine : fichiers déplacés (jamais copiés ni effacés) dans « _Quarantaine » sur le même disque, restauration en un clic, « Vider la quarantaine » vers la corbeille du système après confirmation (nombre et taille). Aussi disponible depuis le menu « … » pour la sélection.
- Contrôle qualité : faux sans perte et fichiers gonflés (analyse spectrale, spectrogramme consultable), fichiers abîmés ou tronqués, débit faible (seuil réglable).
- Santé de la bibliothèque : 17 compteurs cliquables (pochettes, année, genre, note, à valider, non identifiés, doublons, qualité, noms, chemins, caractères interdits, WAV désynchronisés, fichiers manquants ou illisibles) et boutons « Corriger » avec aperçu ; fichiers manquants retrouvés dans un dossier.

## 0.3.0 — Phase 2 : recherche en ligne et Inbox (validée sous Windows le 27 novembre 2026)

- Empreintes audio calculées sur l'ordinateur ; AcoustID, MusicBrainz et Cover Art Archive (Deezer et iTunes disponibles mais désactivés par défaut, voir docs/SOURCES.md).
- Score de confiance détaillé ; application automatique prudente (champs vides seulement, jamais sans empreinte, jamais entre deux versions différentes) ; file « À valider » avec comparaison champ par champ, choix du candidat et de la pochette, raccourcis clavier, acceptation d'un album entier ; liste « Non identifiés » ; taux d'identification affiché.
- Recherche manuelle depuis le panneau de détail.
- Pochettes : grille de choix, recadrage, rotation, comparaison avant/après.
- Recherches mises en file hors connexion, reprises au retour du réseau ; limites de débit strictes et journal des appels ; cache de 30 jours.
- Inbox : nouveaux fichiers du dossier d'arrivée traités automatiquement, puis « Ranger… ».
- Palette de commandes (Ctrl+K), recherche en ligne de la sélection (Ctrl+L).
- Clés AcoustID dans le trousseau du système.

## 0.2.0 — Phase 1 : bibliothèque et édition hors connexion (validée sous Windows le 26 novembre 2026)

- Dossiers de bibliothèque, disques externes reconnus même si la lettre ou le point de montage change, morceaux grisés quand le disque est débranché.
- Indexation en tâche de fond (WAV, MP3, FLAC, AIFF, M4A, OGG, Opus), incrémentale, avec progression et annulation ; surveillance des changements faits hors de l'application ; fichiers illisibles et manquants signalés.
- Tableau virtualisé (100 000 morceaux à 60 images/s) : recherche instantanée, tri multi-colonnes, filtres rapides, colonnes réglables.
- Panneau de détail modifiable pour un ou plusieurs morceaux : titre, version, artistes (ordre, artiste principal), album, année, genre, BPM, tonalité, note, pochette ; champs verrouillés.
- Écriture sûre des tags (copie, relecture de contrôle, remplacement atomique) ; WAV RIFF INFO + ID3 identiques ; données Serato, Rekordbox et Beatport conservées ; son jamais modifié.
- Historique persistant : Ctrl+Z / Ctrl+Maj+Z, même après redémarrage.
- Pochettes depuis un fichier, le presse-papiers ou un glisser-déposer.
- Renommage par modèle avec aperçu, conflits, noms sûrs pour Windows, macOS et clés USB ; nettoyage des tags ; tags depuis le nom de fichier ; échanger artiste et titre ; copier/coller les tags.
- Lecteur avec forme d'onde cliquable et raccourcis clavier.
- Gestion des dossiers : arborescence, création, renommage, déplacement de morceaux par glisser-déposer, affichage dans l'Explorateur / le Finder.
- Premier lancement en 3 écrans ; sauvegarde quotidienne de la base, export et import.

## 0.1.0 — Phase 0 : fondations (validée le 26 novembre 2026)

- Fenêtre principale avec ses cinq zones : barre latérale repliable, barre du haut, centre, panneau de détail (Ctrl+I), lecteur.
- Thèmes clair et sombre, option « Suivre le système », changement immédiat.
- Français et anglais, changement immédiat sans redémarrage.
- Réglages enregistrés dans une base SQLite locale, avec migrations et copie de sécurité avant migration.
- Journal d'erreurs local avec rotation, bouton « Copier le rapport de diagnostic ».
- Taille et position de la fenêtre mémorisées.
- Compilation automatique Windows et macOS, installateurs `.exe`, `.msi` et `.dmg` universel.
- Vérification automatique des licences de toutes les dépendances.
- Aucun fichier audio n'est lu ni modifié dans cette version.
