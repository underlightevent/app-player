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
