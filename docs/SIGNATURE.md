# Activer la signature officielle (plus tard)

Sans signature, Windows et macOS affichent une alerte au premier lancement. La compilation automatique est déjà prête : il suffit d'ajouter des **secrets** au dépôt GitHub (Settings › Secrets and variables › Actions › New repository secret). Tant qu'ils n'existent pas, rien ne change.

## macOS (Developer ID + notarisation)

Il faut un compte Apple Developer (payant, environ 99 $ par an).

| Secret | Contenu |
| --- | --- |
| `APPLE_CERTIFICATE` | certificat « Developer ID Application » exporté en `.p12`, puis converti en base64 |
| `APPLE_CERTIFICATE_PASSWORD` | mot de passe choisi lors de l'export du `.p12` |
| `APPLE_SIGNING_IDENTITY` | nom du certificat, par exemple `Developer ID Application: Nom (ABCDE12345)` |
| `APPLE_ID` | adresse du compte Apple Developer |
| `APPLE_PASSWORD` | mot de passe d'application (créé sur appleid.apple.com) |
| `APPLE_TEAM_ID` | identifiant d'équipe à 10 caractères |

## Windows (certificat de signature de code)

| Secret | Contenu |
| --- | --- |
| `WINDOWS_CERTIFICATE` | certificat `.pfx` converti en base64 |
| `WINDOWS_CERTIFICATE_PASSWORD` | mot de passe du `.pfx` |

Conversion en base64 sous Windows (PowerShell) :

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificat.pfx")) | Set-Clipboard
```

Remarque : de nombreux certificats Windows récents sont livrés sur une clé matérielle ou dans un service en ligne et ne s'exportent pas en `.pfx`. Dans ce cas, l'étape de signature Windows de `.github/workflows/build.yml` devra être adaptée au service choisi.
