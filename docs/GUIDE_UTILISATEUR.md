# Guide utilisateur

Ce guide explique chaque fonction pas à pas. Les captures montrent l'application avec des morceaux de démonstration.

**Sommaire :** [Installer](#installer-lapplication) · [Premier lancement](#premier-lancement) · [La fenêtre](#la-fenêtre) · [Ajouter votre musique](#ajouter-votre-musique) · [Le tableau](#le-tableau) · [Modifier les informations](#modifier-les-informations) · [Annuler](#annuler) · [Renommer](#renommer-les-fichiers) · [Nettoyer](#nettoyer-les-tags) · [Écouter](#écouter) · [Dossiers](#dossiers) · [Recherche en ligne](#recherche-en-ligne) · [Inbox](#inbox-dossier-darrivée) · [Doublons](#doublons) · [Quarantaine](#quarantaine) · [Santé de la bibliothèque](#santé-de-la-bibliothèque) · [Playlists et filtres](#playlists-et-filtres-intelligents) · [Rekordbox](#rekordbox) · [WAV et AIFF](#convertir-des-wav-en-aiff) · [Mises à jour](#mises-à-jour) · [Réglages](#réglages)

> **Avant tout essai :** travaillez sur une **copie** de quelques dizaines de morceaux, jamais sur votre vraie bibliothèque, tant que la phase n'est pas validée.

## Installer l'application

Voir le [README](../README.md) : téléchargement, installation et ouverture malgré l'alerte de sécurité.

## Premier lancement

![Premier lancement](images/premier-lancement.png)

Trois écrans :
1. **Langue et thème.**
2. **Vos dossiers** : choisissez le dossier de votre musique (branchez d'abord votre disque externe). Le dossier d'arrivée (Inbox) est facultatif.
3. **Recherche en ligne et Rekordbox** : explications et conseils.

Tout reste modifiable ensuite.

## La fenêtre

| Zone | Où | À quoi elle sert |
| --- | --- | --- |
| Barre latérale | à gauche | les écrans, et l'arborescence de vos dossiers sous « Dossiers ». Le bouton en haut la replie. |
| Barre du haut | en haut | rechercher (Ctrl+F), renommer (Ctrl+R), « Plus d'actions » (…), voir les tâches en cours, thème clair/sombre. |
| Centre | au milieu | le tableau de vos morceaux. |
| Panneau de détail | à droite | les informations du ou des morceaux sélectionnés, modifiables. **Ctrl+I** le masque ou l'affiche. |
| Lecteur | en bas | écouter, avec la forme d'onde cliquable. |

Sur Mac, **Cmd** remplace **Ctrl**.

![La fenêtre principale](images/bibliotheque.png)

Le thème sombre (bouton lune en haut à droite, ou Réglages › Apparence) :

![Thème sombre](images/theme-sombre.png)

## Ajouter votre musique

- Bibliothèque vide : bouton **Ajouter un dossier**. Ou glissez un dossier depuis l'Explorateur / le Finder sur la fenêtre.
- L'indexation se fait en tâche de fond : vous pouvez travailler pendant ce temps. Le bouton des tâches (en haut à droite) montre la progression et permet d'annuler.
- Ensuite, l'application calcule une « empreinte » de chaque fichier (elle sert aux doublons) ; c'est plus long et cela ne gêne rien.
- Les fichiers ajoutés, supprimés ou renommés en dehors de l'application sont repérés tout seuls.

## Disque externe

- Votre disque est reconnu même si sa lettre change (E: devient F:) ou si son point de montage change sur Mac.
- Disque débranché : un bandeau « Disque … déconnecté » s'affiche, les morceaux restent visibles en grisé et ne sont plus modifiables. Rebranchez-le : tout revient seul.
- Pour un disque utilisé sur Windows et sur Mac, le format **exFAT** est recommandé.

## Le tableau

- **Rechercher** : tapez dans la barre du haut (titre, artistes, album, genre, nom de fichier ; les accents sont ignorés).
- **Filtres rapides** au-dessus du tableau : format, sans pochette, fichiers à problème, note minimale. Un dossier choisi dans l'arborescence filtre aussi le tableau (puce « Dossier : … », cliquez dessus pour l'enlever).
- **Trier** : menu **Trier par** au-dessus du tableau (par exemple **Note : ★★★★★ d'abord**, puis par artiste et titre à note égale), ou cliquez sur un en-tête (une deuxième fois pour inverser ; la colonne **Note** met les mieux notés en premier dès le premier clic). Maj + clic ajoute une colonne de tri.
- **Colonnes** : clic droit sur les en-têtes pour en afficher ou masquer ; glissez un en-tête pour changer l'ordre ; tirez son bord droit pour la largeur. Tout est retenu.
- **Sélection** : clic, Ctrl + clic (ajouter), Maj + clic (plage), Ctrl+A (tout), flèches du clavier.
- Badges de la colonne État : hors ligne, manquant, illisible, cadenas, **RB** (le morceau est dans Rekordbox ; **RB ↻** : ses tags ont changé, à recharger dans Rekordbox), WAV, tags désaccordés.

## Modifier les informations

Sélectionnez un ou plusieurs morceaux, puis modifiez dans le panneau de détail :
- Un champ se valide avec **Entrée** ou en le quittant ; **Échap** annule. **F2** place le curseur dans le titre.
- **Plusieurs morceaux** : un champ aux valeurs différentes affiche « Valeurs multiples » et n'est changé que si vous y touchez.
- **Artistes** : chaque artiste est une étiquette. L'étoile marque l'artiste principal (cliquez sur l'étoile d'un autre pour le mettre en premier). Glissez les étiquettes pour changer l'ordre. × pour retirer, champ « Ajouter un artiste… » puis Entrée.
- **Note** : cliquez sur les étoiles, ou tapez **0 à 5** au clavier quand le tableau a le focus.
- **Tonalité** : tapez-la comme vous voulez (Am, A minor, 8A…), elle est reconnue. Affichage et écriture en Camelot par défaut (Réglages).
- **Cadenas** : tout champ modifié à la main est verrouillé. Aucune automatisation (nettoyage, recherche en ligne de la phase 2) ne le modifiera. Cliquez sur le cadenas pour déverrouiller.
- **Pochette** : survolez la pochette du panneau → « Choisir une image… », « Coller » (ou **Ctrl+V**), « Retirer ». Vous pouvez aussi glisser une image depuis l'Explorateur sur la fenêtre. L'image est mise au carré si elle l'est presque, réduite à 1000 pixels et enregistrée en JPEG.
- **Plus d'actions** (…) : échanger artiste et titre, copier les tags d'un morceau puis les coller sur d'autres.

Chaque modification affiche une notification avec **Annuler**.

### Ce que l'application garantit à chaque écriture
Le son n'est jamais modifié. L'application écrit dans une copie, relit cette copie (tags et son), puis remplace l'original. Au moindre problème, l'original reste intact et un message explique pourquoi. Les données des autres logiciels (Serato, Rekordbox, Beatport, commentaires…) sont conservées.

### Fichiers WAV et Rekordbox
Les WAV reçoivent deux blocs de tags identiques (RIFF INFO, lu par Rekordbox, et ID3). Rekordbox n'affiche que titre, artiste, album, genre et commentaire d'un WAV, et **pas la pochette**.

## Annuler

- **Ctrl+Z** annule la dernière action, **Ctrl+Maj+Z** la rétablit. Cela marche aussi après avoir fermé et rouvert l'application.
- Une action sur plusieurs morceaux s'annule en une fois.

## Renommer les fichiers

1. Sélectionnez les morceaux, puis **Renommer** (ou Ctrl+R).
2. Le modèle par défaut est `{artiste_principal} - {titre}[ ({version})]` : par exemple « Artiste A - Titre (Extended Mix).wav ». Cliquez sur une variable pour l'insérer. Ce qui est entre crochets disparaît si le champ est vide.
3. L'aperçu montre chaque nom avant/après. Les problèmes sont signalés : nom déjà pris, nom interdit par Windows, champ vide, chemin trop long pour Rekordbox sur Mac.
4. Choisissez quoi faire si deux fichiers visent le même nom, puis **Renommer N fichiers**. **Aucun fichier n'est jamais écrasé.**
5. « Enregistrer ce modèle sous un nom » pour le retrouver plus tard.

Le panneau de détail montre aussi le nom que donnerait le modèle (« Nom selon le modèle »).

> Un morceau déjà importé dans Rekordbox et renommé ici y apparaîtra comme manquant. Si vous avez importé votre collection Rekordbox (voir [Rekordbox](#rekordbox)), l'aperçu l'annonce en clair avant de renommer, et le journal des chemins vous aide à relocaliser les fichiers.

## Nettoyer les tags

**Plus d'actions › Nettoyer les tags…** : cochez les règles (mentions parasites, numéros de piste, « _ », « feat. » vers les artistes, version séparée du titre, casse des versions…) et ajoutez vos propres règles rechercher / remplacer. L'aperçu montre chaque changement avant d'appliquer. Les champs verrouillés ne sont jamais touchés.

## Tags depuis le nom de fichier

**Plus d'actions › Tags depuis le nom de fichier…** : pour les fichiers sans tags. Choisissez la forme des noms (`{artistes} - {titre}`, `{piste}. {artistes} - {titre}`…) ou écrivez la vôtre. Par défaut seuls les champs vides sont remplis.

## Écouter

- Double-clic ou **Entrée** sur un morceau pour le lancer. **Espace** : lecture / pause.
- Cliquez sur la **forme d'onde** pour aller à cet endroit. Flèches gauche/droite : reculer / avancer de 10 secondes. Maj + flèches : morceau précédent / suivant (dans l'ordre du tableau).
- La lecture continue même si vous modifiez le morceau en cours.

## Dossiers

- Dans la barre latérale, sous « Dossiers » : l'arborescence de vos dossiers, avec le nombre de morceaux. Cliquez sur un dossier pour n'afficher que ses morceaux.
- Survolez un dossier : nouveau sous-dossier, renommer, afficher dans l'Explorateur / le Finder.
- **Déplacer des morceaux** : glissez-les depuis le tableau sur un dossier. Un aperçu s'affiche ; rien n'est écrasé. Tout s'annule.
![Écran Dossiers](images/dossiers.png)

- L'écran **Dossiers** (barre latérale) liste les dossiers suivis : ajouter, retirer (aucun fichier n'est touché), réindexer.
- **Ctrl+Maj+R** affiche le fichier sélectionné dans l'Explorateur ou le Finder.

## Recherche en ligne

### Préparer (une fois)
1. **Réglages › Recherche en ligne**.
2. Créez vos deux clés AcoustID gratuites (les étapes sont écrites à l'écran) et collez-les, puis « Enregistrer ». Elles vont dans le trousseau de votre ordinateur. Sans clé, l'application cherche seulement par artiste et titre, et n'applique alors **jamais** rien automatiquement.
3. Renseignez le **contact** demandé par MusicBrainz (adresse d'un site ou e-mail).
4. « Tester la connexion ».

### Lancer une recherche
- Sélectionnez des morceaux puis **Rechercher en ligne** (ou **Ctrl+L**). Le témoin en haut à droite montre combien de recherches attendent.
- Hors connexion, les recherches attendent et partent seules au retour du réseau ; une notification le confirme.
- Selon la certitude :
  - **90 et plus** : les champs vides sont remplis et la pochette manquante ajoutée, automatiquement (rien n'est remplacé, rien de verrouillé n'est touché). Tout s'annule avec Ctrl+Z.
  - **60 à 89** : le morceau part dans **À valider**.
  - **moins de 60** : **Non identifiés**.

### À valider

![À valider](images/a-valider.png)

- À gauche les morceaux, à droite les propositions numérotées (score, source, détail : « empreinte 0,96 · titre identique · durée +1 s »).
- Pour chaque champ : valeur actuelle, valeur proposée, case à cocher. Choisissez la pochette dans la grille (dimensions et source affichées). Une pochette pas carrée ouvre l'éditeur de recadrage.
- Clavier : **Entrée** accepte, **Échap** rejette, **↑ ↓** changent de morceau, **1 à 9** choisissent une proposition.
- « Accepter tout l'album » accepte d'un coup tous les morceaux de la même sortie.
- En haut : le **taux d'identification** de tout ce qui a été recherché.

### Recherche manuelle
Dans le panneau de détail (un seul morceau sélectionné) : **Rechercher en ligne…**, tapez l'artiste et le titre, puis appliquez tout ou partie du résultat. Aussi disponible dans À valider pour les morceaux mal reconnus.

### Recadrer une pochette
Glissez le carré, tirez son coin pour l'agrandir, « Pivoter » si besoin, « Comparer avant / après », puis « Utiliser ce recadrage ».

## Inbox (dossier d'arrivée)

![Inbox](images/inbox.png)

- Choisissez votre dossier de téléchargements (Beatport, Bandcamp…) au premier lancement ou dans l'écran **Inbox**.
- Chaque nouveau fichier y est identifié, tagué, pourvu d'une pochette et renommé automatiquement **quand c'est sûr** ; sinon il passe par À valider.
- Quand les morceaux sont prêts : **Ranger…** puis cliquez sur le dossier de destination (le dernier utilisé est proposé).
- Un morceau traité dans l'Inbox avant son import dans Rekordbox n'y aura jamais de lien cassé.

## Doublons

![Écran Doublons](images/doublons.png)

L'application repère deux sortes de doublons :
- les **copies exactes** : le même son, même si les tags ou le nom diffèrent ;
- le **même morceau dans un autre format ou une autre qualité** (par exemple le WAV et le MP3 320) : le son est comparé sur votre ordinateur, sans internet, et les durées doivent être à 2 secondes près.

Deux **versions différentes** (Original Mix et Extended Mix, Radio Edit, remix) ne sont **jamais** regroupées.

1. Ouvrez **Doublons** dans la barre latérale, puis **Rechercher les doublons**. La recherche tourne en tâche de fond (elle peut être longue la première fois : chaque morceau concerné est écouté une fois).
2. Chaque groupe est une carte : les fichiers côte à côte, avec format, qualité, durée, taille, emplacement, tags et pochette. Le bouton ▶ permet d'écouter chaque fichier.
3. Le fichier conseillé porte le badge **À garder**. L'ordre : WAV, puis AIFF, puis FLAC (la plus haute résolution d'abord), puis les fichiers avec perte du plus haut débit au plus bas. Un fichier « suspect » (voir [Santé de la bibliothèque](#santé-de-la-bibliothèque)) passe derrière un vrai MP3 320. À égalité : les tags les plus complets, puis la pochette, puis la note. Cet ordre se change dans **Réglages › Doublons et qualité**.
4. **Garder celui-ci** pour choisir un autre fichier. Votre choix est retenu.
5. **Ce ne sont pas des doublons** : le groupe disparaît et ne sera plus proposé.
6. **Traiter…** (un groupe) ou **Traiter les N groupes…** (tous) : un aperçu montre ce qui sera reporté vers le fichier gardé (tags qui lui manquent, note la plus haute, pochette) et quels fichiers partiront en quarantaine. Confirmez.

Rien n'est effacé : les fichiers écartés vont en [quarantaine](#quarantaine). **Ctrl+Z** annule tout le traitement d'un coup (tags et fichiers).

## Quarantaine

![Écran Quarantaine](images/quarantaine.png)

- Les fichiers écartés (doublons, ou **… › Mettre en quarantaine** sur une sélection) sont **déplacés** dans un dossier `_Quarantaine`, en haut de votre dossier de musique, sur le même disque. Leur arborescence est gardée. Le déplacement est instantané et ne prend pas de place en plus.
- L'écran **Quarantaine** les liste avec leur emplacement d'origine et leur date d'entrée.
- **Restaurer** remet un fichier à sa place, identique. Cochez plusieurs fichiers pour les restaurer ensemble.
- **Vider la quarantaine…** envoie les fichiers dans la **corbeille** de Windows ou du Mac, après une confirmation qui indique leur nombre et leur taille. Vous pouvez encore les récupérer depuis la corbeille.
- Si un fichier est dans Rekordbox, une alerte s'affiche avant de le déplacer.

## Santé de la bibliothèque

![Santé de la bibliothèque](images/sante.png)

Une liste de compteurs : sans pochette, pochette trop petite, sans année, sans genre, sans note, à valider, non identifiés, doublons, qualité suspecte, débit faible, noms non conformes au modèle, chemins trop longs, caractères interdits, WAV désynchronisés, fichiers manquants, illisibles, abîmés ou tronqués.

- **Cliquez sur un compteur** : la bibliothèque s'ouvre sur la liste de ces morceaux (puce en haut du tableau ; cliquez dessus pour l'enlever).
- **Corriger** apparaît quand une correction existe, toujours avec aperçu : recherche en ligne, renommage (fenêtre habituelle), resynchronisation des WAV (liste puis confirmation), ou **Retrouver dans un dossier…** pour les fichiers manquants (déplacés en dehors de l'application : choisissez le dossier où ils sont, l'application les reconnaît par leur nom et leur taille, ou par le son lui-même, puis vous confirmez).
- **Lancer le contrôle qualité** : chaque fichier est lu en entier pour trouver :
  - les **faux sans perte** : un WAV ou un FLAC fabriqué à partir d'un MP3 garde la coupure nette des aigus du MP3 (vers 16 kHz pour un 128 kbit/s, 19 à 20 kHz pour un 320). Il est marqué « suspect » ;
  - les **fichiers gonflés** : un MP3 annoncé à 320 kbit/s mais issu d'un 128 ;
  - les fichiers **abîmés ou tronqués**.

  Rien n'est modifié. Dans le panneau de détail, la ligne **Contrôle qualité** donne le verdict, et **Spectrogramme** affiche l'image du son : une limite horizontale nette, au-dessus de laquelle tout est noir, trahit un fichier issu d'un MP3.
- Le seuil du **débit faible** (320 kbit/s par défaut) se règle dans **Réglages › Doublons et qualité**.

## Playlists et filtres intelligents

![Playlists et filtres](images/playlists.png)

- **Nouvelle playlist** : donnez un nom. Pour la remplir, sélectionnez des morceaux dans la bibliothèque puis **… › Ajouter à une playlist…** (vous pouvez aussi y créer une playlist).
- **Nouveau filtre intelligent** : il se remplit tout seul selon des règles, par exemple « Genre contient Techno » et « Note au moins 4 ». Choisissez « toutes les règles » ou « au moins une règle ».
- Cliquez sur une playlist pour voir ses morceaux dans la bibliothèque. Dans une playlist, **… › Retirer de « … »** enlève les morceaux sélectionnés.
- Les icônes de chaque ligne : renommer ou modifier le filtre, **exporter en M3U** (lisible par la plupart des logiciels de DJ et lecteurs), supprimer (les morceaux et les fichiers ne sont jamais touchés).
- **… › Exporter la sélection en M3U…** crée une playlist M3U à partir de n'importe quelle sélection.

## Rekordbox

L'application ne lit et n'écrit **jamais** la base interne de Rekordbox : elle utilise seulement le fichier XML officiel. Tout se passe dans **Réglages › Rekordbox**.

![Réglages, section Rekordbox](images/reglages.png)

### Savoir quels morceaux sont dans Rekordbox
1. Dans Rekordbox : menu **Fichier › Exporter la collection au format xml** (*Export Collection in xml format*), enregistrez le fichier.
2. Dans l'application : **Réglages › Rekordbox › Importer le XML de Rekordbox…**, choisissez ce fichier.
3. Les morceaux retrouvés portent le badge **RB** dans le tableau. Le nombre de morceaux retrouvés s'affiche.
4. Si l'import a plus de 7 jours, l'application propose de le refaire (à l'ouverture et dans les réglages).

### Mode protégé (activé par défaut)
- Un morceau badgé n'est **jamais renommé ni déplacé automatiquement** (par exemple par l'Inbox).
- Avant un renommage, un déplacement ou une mise en quarantaine à la main, une alerte claire s'affiche : « Ce morceau est dans Rekordbox : il y apparaîtra comme manquant. »
- Ses tags restent modifiables.
- Chaque renommage ou déplacement est noté dans le **journal des chemins**. **Exporter le journal (CSV)…** le donne sous forme de tableau (ancien chemin ; nouveau chemin), ouvrable dans Excel, pour relocaliser les fichiers dans Rekordbox.

### Après une modification de tags : « À recharger »
Rekordbox ne voit une modification de tags faite ailleurs qu'après sa commande **Reload Tag**. La liste **À recharger dans Rekordbox** recense les morceaux badgés dont les tags ont changé (badge **RB ↻**).
1. **Exporter en M3U8…** et enregistrez le fichier.
2. Dans Rekordbox, importez cette playlist (glissez le fichier M3U8 dans la zone des playlists, ou menu Fichier › Importer › Importer une playlist).
3. Ouvrez-la, sélectionnez tous ses morceaux (Ctrl+A), clic droit › **Reload Tag**.
4. De retour dans l'application : **Vider la liste**.

### Export XML vers Rekordbox (notes et playlists)
1. **Exporter vers Rekordbox (XML)…** (dans Réglages › Rekordbox ou dans Playlists et filtres).
2. Choisissez les morceaux (toute la bibliothèque, la sélection, ou seulement ceux des playlists) et les playlists à inclure. Les filtres intelligents sont exportés avec leurs morceaux du moment.
3. Enregistrez le fichier XML. Il contient pour chaque morceau : emplacement, titre, artistes, album, genre, année, tonalité, remixeur, label, version et note.
4. **Sauvegardez d'abord votre bibliothèque Rekordbox** (Fichier › Bibliothèque › Sauvegarder la bibliothèque).
5. Dans Rekordbox : **Préférences › Avancé › Base de données › rekordbox xml**, choisissez le fichier exporté. Affichez la rubrique « rekordbox xml » dans le panneau de gauche (Préférences › Affichage › Disposition, case « rekordbox xml » ; dans les anciennes versions, elle est dans le panneau Bridge).
6. Dépliez **rekordbox xml › Playlists › DJ Library Manager** : vos playlists y sont. Glissez-les dans vos playlists Rekordbox, ou clic droit › « Importer dans la collection ».

> À vérifier sur votre ordinateur (l'application n'a pas pu être testée avec Rekordbox) : pour un morceau **déjà présent** dans votre collection, Rekordbox peut demander s'il faut mettre à jour ses informations (dont la note) ou garder les siennes. Faites l'essai sur un ou deux morceaux après une sauvegarde.

## Convertir des WAV en AIFF

Rekordbox ne lit dans un WAV que 5 champs (titre, artiste, album, genre, commentaire) et **aucune pochette**. Un AIFF, lui, porte tous les tags et la pochette, et le son est le même.

1. Sélectionnez des WAV, puis **… › Convertir les WAV en AIFF…**.
2. L'aperçu liste les fichiers et prévient si un WAV est déjà dans Rekordbox : l'AIFF y sera un **nouveau** morceau (les repères, notes et playlists de Rekordbox ne suivent pas).
3. **Convertir**. Pour chaque fichier : l'AIFF est créé à côté du WAV, avec tous les tags et la pochette ; l'application vérifie que le son décodé est **identique échantillon par échantillon** ; sinon rien n'est gardé.
4. Les WAV d'origine partent en **quarantaine** : vérifiez les AIFF (écoutez-les, regardez-les dans Rekordbox), puis videz la quarantaine si tout va bien. **Ctrl+Z** annule la conversion (les AIFF partent en quarantaine, les WAV reviennent).

Rien n'est jamais converti automatiquement.

## Mises à jour

Au démarrage, l'application demande à GitHub le numéro de la dernière version (rien d'autre n'est envoyé). Si une version plus récente existe, une notification propose **Télécharger** : la page de téléchargement s'ouvre dans votre navigateur, installez la nouvelle version par-dessus l'ancienne (votre bibliothèque, vos réglages et votre historique sont gardés). **Réglages › Mises à jour** : désactiver cette vérification, ou **Vérifier maintenant**.

## Palette de commandes
**Ctrl+K** (Cmd+K sur Mac) : tapez une action (« renommer », « nettoyer », « aller à »…) ou le nom d'un morceau, puis Entrée.

## Réglages

- **Apparence**, **Langue**, **Densité du tableau**, **Notation des tonalités**.
- **Recherche en ligne** : sources, clés AcoustID, contact, seuils, remplacement des valeurs existantes, renommage automatique, test de connexion, journal des appels.
- **Écriture des tags** : titre écrit dans les fichiers (voir ci-dessous), décimales du BPM, une seule pochette par fichier, séparateur des artistes, taille des pochettes.

### Titre écrit dans les fichiers (ce que la platine affiche)
Sur une platine, une playlist exportée depuis Rekordbox montre souvent **seulement le titre**. Dans **Réglages › Écriture des tags › Titre écrit dans les fichiers**, choisissez ce qu'il contient :
- titre seul (« Strobe ») ;
- titre et version (« Strobe (Extended Mix) », le choix par défaut) ;
- artiste principal et titre (« Deadmau5 - Strobe ») ;
- artiste principal, titre et version (« Deadmau5 - Strobe (Extended Mix) »).

Dans l'application, le titre, la version et l'artiste restent séparés : seul le champ titre du fichier change. Le choix s'applique à chaque nouvelle écriture de tags. Pour les fichiers déjà présents : **Appliquer aux fichiers existants…** montre le nombre de morceaux et des exemples, puis réécrit seulement le champ titre (le son n'est pas touché ; **Ctrl+Z** remet les titres d'avant). Ensuite, dans Rekordbox, rechargez les tags avec la liste **À recharger** (voir [Rekordbox](#rekordbox)), puis réexportez vers votre clé USB.

- **Doublons et qualité** : ordre des formats sans perte pour choisir le fichier à garder, seuil du débit faible.
- **Rekordbox** : import du XML, mode protégé, liste « À recharger », journal des chemins, export XML (voir [Rekordbox](#rekordbox)).
- **Mises à jour** : vérification au démarrage, « Vérifier maintenant ».
- **Base de données** : une copie est faite chaque jour (7 gardées). « Exporter la base… » pour en garder une copie où vous voulez ; « Importer une base… » la remettra en place au prochain démarrage.
- **Signaler un problème** : copie un rapport à coller dans votre message.
