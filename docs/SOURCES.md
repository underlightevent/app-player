# Sources en ligne

Conditions d'utilisation de chaque source, lues le 26 novembre 2026 sur les pages officielles, et ce que l'application en fait. L'usage est personnel aujourd'hui, mais l'application pourra être partagée plus tard : les points qui posent problème pour un partage ou un usage commercial sont signalés.

Résumé :

| Source | Activée par défaut | Pourquoi |
| --- | --- | --- |
| MusicBrainz | oui | gratuite pour un usage non commercial, données de base dans le domaine public |
| Cover Art Archive | oui | pas de limite de débit ; images protégées par le droit d'auteur de leurs propriétaires |
| AcoustID | oui, si une clé est saisie | gratuite pour un usage non commercial ; une clé utilisateur sera obligatoire au 1er janvier 2027 |
| Deezer | **non** | ses conditions interdisent toute copie locale du contenu et tout usage commercial |
| iTunes Search | **non** | ses conditions réservent les pochettes à la promotion de l'iTunes Store |

## MusicBrainz

- Pages : [Rate Limiting](https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting), [API](https://musicbrainz.org/doc/MusicBrainz_API).
- Accès : sans clé. En-tête **User-Agent obligatoire** de la forme « Nom/version ( adresse de contact ) ». Sans contact identifiable, les requêtes peuvent être bloquées.
- Débit : **une requête par seconde au plus** (« never make more than ONE call per second »).
- Usage : « Non-commercial use of this web service is free ». Un usage commercial demande un accord avec la MetaBrainz Foundation.
- Données : les données de base de MusicBrainz sont publiées sous CC0 (domaine public). *La page de licence n'a pas été relue lors de cette vérification.*
- Dans l'application : file d'attente à 1 requête/s, en-tête « dj-library-manager/<version> ( contact ) ». **Le contact est un réglage** (voir « Points à confirmer »). Points d'accès : `/ws/2/recording/<MBID>?inc=artist-credits+releases+isrcs+genres`, `/ws/2/recording?query=…`, `/ws/2/isrc/<isrc>`.

## Cover Art Archive

- Pages : [coverartarchive.org](https://coverartarchive.org/), [API](https://musicbrainz.org/doc/Cover_Art_Archive/API).
- Accès : sans clé ; « There are currently no rate limiting rules in place ». Le service peut répondre 503 en cas d'abus : l'application reste raisonnable (1 requête/s, comme MusicBrainz).
- Droits : « All images are copyrighted by their respective copyright owners. » Intégrer une pochette dans ses propres fichiers pour un usage personnel est l'usage courant (MusicBrainz Picard le fait) ; **redistribuer** les images ne l'est pas.
- Dans l'application : `/release/<MBID>/front-1200` (ou `-500`) puis traitement local (carré, 1000 px, JPEG).

## AcoustID

- Page : [Web service](https://acoustid.org/webservice).
- Accès : `https://api.acoustid.org/v2/lookup` avec `client` (clé d'application), `duration`, `fingerprint`, `meta=recordings+releases+isrcs`.
- **Clé utilisateur** : facultative aujourd'hui, **obligatoire à partir du 1er janvier 2027** (« from 1 January 2027 they require one »). L'application prévoit donc deux clés : la clé d'application et la clé utilisateur (chacun obtient la sienne gratuitement sur acoustid.org).
- Débit : « Do not make more than 3 requests per second ». L'application se limite à 2 par seconde.
- Usage : « provided for free for non-commercial use only ». Usage commercial : offre payante (acoustid.biz).
- Empreintes : calculées sur l'ordinateur par rusty-chromaprint (licence MIT). Seule l'empreinte (une suite de nombres) et la durée sont envoyées, jamais le fichier audio.

## Deezer

- Pages : [Guidelines](https://developers.deezer.com/guidelines), [Terms of use](https://developers.deezer.com/termsofuse) (mises à jour le 27 octobre 2026).
- Usage : « Private non-commercial use » ; le développeur ne doit tirer aucun revenu des services ou du contenu.
- **Copie locale interdite** : les conditions interdisent de faire « any local copy of any Content ». Écrire une pochette Deezer dans un fichier, ou garder ses réponses en cache 30 jours comme le prévoit le cahier des charges, revient à faire une copie locale.
- Débit : aucun chiffre officiel publié (les conditions renvoient à des quotas fixés ailleurs). Chiffre couramment observé : 50 requêtes par 5 secondes ; l'application se limite à 5 par seconde.
- Dans l'application : **désactivée par défaut**. Si l'utilisateur l'active malgré tout (usage privé, à ses risques), elle sert à la correspondance artiste/titre, à l'album, à l'année et au BPM.

## iTunes Search (Apple)

- Page : [Search API](https://performance-partners.apple.com/search-api).
- Débit : « approximately 20 calls per minute ». L'application se limite à 1 requête toutes les 3 secondes.
- Usage : le contenu (dont les pochettes) ne peut servir qu'à **promouvoir** le contenu de l'iTunes Store (« for the purposes of promoting the subject of the Promo Content »), près d'un badge « Download on iTunes ». Récupérer des pochettes pour taguer ses propres fichiers ne fait pas partie des usages prévus.
- Dans l'application : **désactivée par défaut**, même logique que Deezer.

## Points à confirmer par l'utilisateur

1. **Contact du User-Agent MusicBrainz** : réglage « Contact » (Réglages › Recherche en ligne), vide par défaut. L'application n'envoie aucune information sur l'utilisateur ou le projet sans qu'il l'ait saisie. Proposition : l'adresse publique du dépôt GitHub du projet. Tant que le champ est vide, MusicBrainz reste utilisable mais peut ralentir ou refuser les requêtes.
2. **Deezer et iTunes** : laisser désactivés (recommandé), ou les activer pour un usage strictement personnel en connaissance de cause. Sans eux, la musique électronique pointue sera moins souvent identifiée.
3. **Clés AcoustID** : créer une clé d'application et une clé utilisateur (gratuites).

## Veille musicale (phase 5)

Toutes désactivées par défaut (Réglages › Veille musicale). Seuls le pays, le genre ou le nom du label recherchés sont envoyés.

| Source | Usage | Accès | Cache | Remarques |
| --- | --- | --- | --- | --- |
| Apple Music (flux publics « Marketing Tools ») | classements par pays, morceaux et albums | sans clé : `rss.marketingtools.apple.com/api/v2/<pays>/music/most-played/<10 à 100>/songs.json` (ou `albums.json`) | 12 h | flux destinés à promouvoir Apple Music : l'application affiche les titres et renvoie vers la page Apple Music ; pas de flux public des nouveautés par pays (essai du 27/11/2026 : 404) |
| Deezer | classements par genre, liste des genres | sans clé : `/chart/<genre>/tracks`, `/genre` | **aucun** | mêmes conditions que pour la recherche en ligne : pas de copie locale ni d'usage commercial ; pays déduit par Deezer de la connexion |
| MusicBrainz | nouvelles sorties par label ou par pays | sans clé : `/ws/2/release?query=date:[…] AND label:"…"` ou `country:XX` | 24 h | 1 requête/s, même en-tête User-Agent que la recherche en ligne ; base bénévole, moins complète pour les petits labels |
| Spotify (compte lié, optionnel) | ajouter des titres à une playlist du compte | OAuth 2 PKCE avec l'identifiant client de l'utilisateur ; `/search`, `/me/playlists`, `/playlists/<id>/items` | aucun | mode développement : propriétaire Premium, 5 comptes au plus (règles du 11/02/2026) ; seuls les titres choisis sont envoyés |

Non pris en charge (vérifié le 27/11/2026) : Deezer pour les playlists (création d'applications fermée), Apple Music (MusicKit exige l'abonnement développeur payant), Beatport et Shazam (pas d'accès public), Tidal (API récente à étudier).
