# Journal des versions

## 0.2.0 — Phase 1 : bibliothèque et édition hors connexion (à valider)

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
