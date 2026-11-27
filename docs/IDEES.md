# Idées hors phase

Idées notées en chemin, à discuter avant de les réaliser.

- Phase 0 : afficher le raccourci réel (Ctrl ou Cmd) dans les infobulles selon le système. Aujourd'hui l'infobulle du panneau de détail indique toujours « Ctrl+I ».
- Phase 0 : barre de titre intégrée sous Windows, quand Tauri saura afficher les boutons natifs par-dessus le contenu (voir DECISIONS D-007).
- Phase 1 : édition directement dans les cellules du tableau (aujourd'hui, F2 ouvre le panneau de détail).
- Phase 1 : option « calculer les formes d'onde dès l'indexation ».
- Phase 1 : écran de réglage de la table de remplacement des caractères interdits.
- Phase 1 : vignettes de pochettes dans le tableau (aujourd'hui, un simple repère « pochette / pas de pochette »).
- Phase 1 : déplacement de morceaux entre deux disques (copie vérifiée puis mise en quarantaine de l'original).
- Phase 1 : indicateur « en cours de lecture » dans le tableau (sans utiliser la couleur d'accent).
- Phase 2 : règle « Après validation, déplacer vers [dossier] » pour l'Inbox (désactivée par défaut selon le cahier des charges).
- Phase 2 : recherche par ISRC quand le fichier en contient déjà un.
- Phase 2 : glisser une image depuis un navigateur web vers la pochette (aujourd'hui : fichier, presse-papiers, Explorateur / Finder).
- Phase 3 : contrôle qualité automatique des nouveaux morceaux (Inbox), en option.
- Phase 3 : réglage de la taille minimale d'une pochette correcte (aujourd'hui 500 px).
- Phase 3 : recherche des doublons sans métadonnées (comparer toutes les empreintes entre elles, plus long).
- Phase 4 : installation automatique des mises à jour (module « updater » de Tauri ; demande une clé de signature des mises à jour, voir docs/SIGNATURE.md).
- Phase 4 : glisser des morceaux sur une playlist ; réordonner les morceaux d'une playlist.
- Phase 4 : filtres intelligents sur la date d'ajout et sur le résultat du contrôle qualité.
- Phase 4 : export XML qui recrée les filtres intelligents comme « playlists intelligentes » de Rekordbox (format partiellement documenté).
- Réparer les champs de date ID3 abîmés (TDRC, TDEN… mal formés, environ 0,5 % des fichiers de l'utilisateur) : aujourd'hui l'écriture des tags de ces fichiers est refusée par sécurité. Idée : action « Réparer » avec aperçu, qui remplace seulement ces dates illisibles.
- Veille musicale (demandée par l'utilisateur le 27/11/2026, à cadrer avant de développer) : classements par pays (flux RSS d'Apple Music, classements Deezer), nouvelles sorties par label (MusicBrainz, Discogs) ou par pays (Apple Music), ajout à une « liste d'envies » avec lien d'achat ; un morceau de la liste arrivé dans l'Inbox serait reconnu et coché. Le téléchargement de musique lui-même reste hors périmètre (cahier des charges) : seulement des liens vers les boutiques.
