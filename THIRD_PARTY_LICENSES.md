# Composants tiers et licences

Uniquement des licences permissives (principe 12 du cahier des charges). La liste complète des dépendances indirectes est vérifiée automatiquement à chaque compilation par `scripts/check-licenses.mjs` et `scripts/check-rust-licenses.mjs` ; voir aussi `docs/DECISIONS.md` (D-011 et D-112).

## Cœur (Rust)

| Composant | Rôle | Licence |
| --- | --- | --- |
| tauri, tauri-build | application de bureau | MIT ou Apache-2.0 |
| tauri-plugin-log | journal local | MIT ou Apache-2.0 |
| tauri-plugin-window-state | mémoriser taille et position de la fenêtre | MIT ou Apache-2.0 |
| tauri-plugin-dialog | fenêtres « choisir un dossier / une image » | MIT ou Apache-2.0 |
| tauri-plugin-opener | « Afficher dans l'Explorateur / le Finder » | MIT ou Apache-2.0 |
| rusqlite | accès à la base SQLite | MIT |
| SQLite (intégré via libsqlite3-sys) | base de données | domaine public |
| lofty | lecture et écriture des tags | MIT ou Apache-2.0 |
| symphonia | décodage audio (lecture, empreintes, formes d'onde) | MPL-2.0 |
| rodio, cpal | sortie son | MIT ou Apache-2.0 / Apache-2.0 |
| image | pochettes (décodage, recadrage, JPEG) | MIT ou Apache-2.0 |
| notify | surveillance des dossiers | CC0-1.0 (domaine public), voir D-112 |
| xxhash-rust (utilisé par notify) | calcul de hachage | BSL-1.0 (Boost), voir D-112 |
| blake3 | hash du flux audio | CC0-1.0 ou Apache-2.0 |
| rayon | lecture des fichiers en parallèle | MIT ou Apache-2.0 |
| walkdir | parcours des dossiers | MIT ou Unlicense |
| regex | règles de nettoyage | MIT ou Apache-2.0 |
| unicode-normalization | noms de fichiers NFC | MIT ou Apache-2.0 |
| base64 | échange d'images avec l'interface | MIT ou Apache-2.0 |
| serde, serde_json, log, thiserror | outils | MIT ou Apache-2.0 |
| windows-sys (Windows), libc (macOS) | identification des disques | MIT ou Apache-2.0 |
| ureq, native-tls | accès réseau (chiffrement du système) | MIT ou Apache-2.0 |
| rusty-chromaprint | empreintes audio | MIT |
| keyring | clés dans le trousseau du système | MIT ou Apache-2.0 |
| strsim | ressemblance des titres et artistes | MIT |
| rustfft | analyse spectrale (faux sans perte, spectrogramme) | MIT ou Apache-2.0 |
| trash | envoi à la corbeille du système (« Vider la quarantaine ») | MIT |
| quick-xml | lecture et écriture du XML Rekordbox | MIT |
| tempfile (tests uniquement) | dossiers temporaires | MIT ou Apache-2.0 |

## Interface (JavaScript)

| Composant | Rôle | Licence |
| --- | --- | --- |
| React, React DOM | interface | MIT |
| @tauri-apps/api, @tauri-apps/plugin-dialog | appel du cœur, fenêtres système | MIT ou Apache-2.0 |
| @tanstack/react-virtual | tableau virtualisé | MIT |
| i18next, react-i18next | traductions | MIT |
| lucide-react | icônes | ISC |

## Outils de développement (non livrés dans l'application)

Vite, Vitest, TypeScript, @vitejs/plugin-react, @tauri-apps/cli, @types/node : MIT ou Apache-2.0.
