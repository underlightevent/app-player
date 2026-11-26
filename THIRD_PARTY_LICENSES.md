# Composants tiers et licences

Uniquement des licences permissives (principe 12 du cahier des charges). La liste complète des dépendances indirectes est vérifiée automatiquement à chaque compilation par `scripts/check-licenses.mjs` et `scripts/check-rust-licenses.mjs` ; voir aussi `docs/DECISIONS.md` (D-011).

## Cœur (Rust)

| Composant | Rôle | Licence |
| --- | --- | --- |
| tauri, tauri-build | application de bureau | MIT ou Apache-2.0 |
| tauri-plugin-log | journal local | MIT ou Apache-2.0 |
| tauri-plugin-window-state | mémoriser taille et position de la fenêtre | MIT ou Apache-2.0 |
| rusqlite | accès à la base SQLite | MIT |
| SQLite (intégré via libsqlite3-sys) | base de données | domaine public |
| serde, serde_json | échange de données avec l'interface | MIT ou Apache-2.0 |
| log | messages du journal | MIT ou Apache-2.0 |
| thiserror | erreurs | MIT ou Apache-2.0 |
| tempfile (tests uniquement) | dossiers temporaires | MIT ou Apache-2.0 |

## Interface (JavaScript)

| Composant | Rôle | Licence |
| --- | --- | --- |
| React, React DOM | interface | MIT |
| @tauri-apps/api | appel du cœur | MIT ou Apache-2.0 |
| i18next, react-i18next | traductions | MIT |
| lucide-react | icônes | ISC |

## Outils de développement (non livrés dans l'application)

Vite, Vitest, TypeScript, @vitejs/plugin-react, @tauri-apps/cli : MIT ou Apache-2.0.
