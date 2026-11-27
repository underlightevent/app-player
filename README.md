# DJ Library Manager

Application de bureau (Windows et macOS) pour organiser une bibliothèque audio de DJ : tags, pochettes, noms de fichiers, notes, doublons, écoute et dossiers. Nom provisoire.

> **État actuel : Phases 3, 4 et 5 (doublons, qualité, Rekordbox, finitions et veille musicale), version 0.6.2 à valider.** L'application indexe votre musique, modifie les tags et les pochettes, renomme les fichiers, gère les dossiers, lit les morceaux, complète les informations en ligne, repère les doublons et les fichiers de mauvaise qualité, met les fichiers écartés en quarantaine restaurable, gère des playlists et des filtres intelligents, travaille avec Rekordbox par son fichier XML officiel (badges, mode protégé, export des notes et des playlists), et propose un écran Sorties (classements, nouveautés avec labels favoris, extraits à écouter, liste d’envies, ajout à une playlist Spotify). Tout fonctionne sans internet, sauf la recherche en ligne, la veille musicale et la vérification des mises à jour.
>
> **Testez d'abord sur une copie de quelques dizaines de morceaux, jamais sur votre vraie bibliothèque, tant que la phase n'est pas validée.** Mode d'emploi complet : [docs/GUIDE_UTILISATEUR.md](docs/GUIDE_UTILISATEUR.md).

Le cahier des charges complet est dans [docs/CAHIER_DES_CHARGES.md](docs/CAHIER_DES_CHARGES.md).

## Télécharger l'application

Vous n'avez rien à compiler : GitHub fabrique les installateurs automatiquement.

- **Version publiée** : page **Releases** du dépôt (colonne de droite sur la page d'accueil du dépôt), puis « Assets ».
- **Dernière version en cours** : onglet **Actions** › dernière exécution « Compilation » marquée d'une coche verte › en bas de la page, rubrique « Artifacts ».

| Système | Fichier à prendre |
| --- | --- |
| Windows 10 ou 11 (64 bits) | `DJ Library Manager_x.y.z_x64-setup.exe` (recommandé) ou le `.msi` |
| macOS 12 ou plus récent (Apple Silicon et Intel) | `DJ Library Manager_x.y.z_universal.dmg` |

## Installer et ouvrir malgré l'alerte de sécurité

L'application n'est pas encore signée par un certificat officiel. Votre système vous avertira la première fois : c'est normal.

### Windows

1. Double-cliquez sur le fichier `.exe`.
2. Si un écran bleu « Windows a protégé votre ordinateur » apparaît, cliquez sur **Informations complémentaires**, puis sur **Exécuter quand même**.
3. Suivez l'installation. L'application apparaît dans le menu Démarrer sous le nom « DJ Library Manager ».

### macOS

1. Ouvrez le fichier `.dmg` et glissez **DJ Library Manager** dans le dossier **Applications**.
2. Lancez l'application depuis le dossier Applications. macOS refuse de l'ouvrir : cliquez sur **OK** (ou **Terminé**).
3. Ouvrez **Réglages Système › Confidentialité et sécurité**. Descendez jusqu'au message concernant DJ Library Manager et cliquez sur **Ouvrir quand même**, puis confirmez avec votre mot de passe.
4. Les fois suivantes, l'application s'ouvre normalement.

## Premier lancement

La fenêtre comporte cinq zones :

1. **Barre latérale** (à gauche, repliable) : Bibliothèque, Inbox, À valider, Doublons, Santé, Quarantaine, Playlists, Dossiers et Réglages.
2. **Barre du haut** : recherche, boutons d'action, indicateur En ligne / Hors ligne, bouton de thème clair/sombre.
3. **Centre** : la liste des morceaux (vide pour l'instant).
4. **Panneau de détail** (à droite) : se masque et s'affiche avec **Ctrl+I** (Cmd+I sur Mac).
5. **Lecteur** (en bas).

Dans **Réglages**, vous pouvez changer le thème (clair, sombre, suivre le système) et la langue (français, anglais). Le changement est immédiat et retenu au prochain lancement.

## Conseil pour votre disque externe

Si votre disque de musique sert à la fois sur Windows et sur Mac, le format **exFAT** est recommandé : les deux systèmes peuvent y lire et écrire.

## Signaler un problème

**Réglages › Signaler un problème › Copier le rapport de diagnostic**, puis collez le texte dans votre message. Rien n'est envoyé automatiquement.

Le journal de l'application se trouve ici :

- Windows : `%LOCALAPPDATA%\com.dj-library-manager.desktop\logs\`
- macOS : `~/Library/Logs/com.dj-library-manager.desktop/`

## Vie privée

Aucune télémétrie. L'application ne se connecte à internet que pour les sources de recherche que vous activez, et au démarrage pour demander à GitHub le numéro de la dernière version (désactivable dans Réglages › Mises à jour).

## Pour les développeurs

- Cœur en Rust (`src-tauri/`), interface React + TypeScript (`src/`), application Tauri 2.
- Commandes utiles : `npm ci`, `npm run tauri dev`, `npm test`, `cargo test --manifest-path src-tauri/Cargo.toml`.
- Choix techniques : [docs/DECISIONS.md](docs/DECISIONS.md). Licences : [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
