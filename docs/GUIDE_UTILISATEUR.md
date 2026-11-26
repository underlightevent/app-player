# Guide utilisateur

Ce guide est complété à chaque phase. Les captures d'écran seront ajoutées quand l'interface sera stabilisée (fin de la phase 2) ; en attendant, chaque étape décrit précisément où cliquer.

> **Avant tout essai :** travaillez sur une **copie** de quelques dizaines de morceaux, jamais sur votre vraie bibliothèque, tant que la phase n'est pas validée.

## Installer l'application

Voir le [README](../README.md) : téléchargement, installation et ouverture malgré l'alerte de sécurité.

## Premier lancement

Trois écrans :
1. **Langue et thème.**
2. **Vos dossiers** : choisissez le dossier de votre musique (branchez d'abord votre disque externe). Le dossier d'arrivée (Inbox) est facultatif ; il sera traité en phase 2.
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

## Ajouter votre musique

- Bibliothèque vide : bouton **Ajouter un dossier**. Ou glissez un dossier depuis l'Explorateur / le Finder sur la fenêtre.
- L'indexation se fait en tâche de fond : vous pouvez travailler pendant ce temps. Le bouton des tâches (en haut à droite) montre la progression et permet d'annuler.
- Ensuite, l'application calcule une « empreinte » de chaque fichier (pour les doublons de la phase 3) ; c'est plus long et cela ne gêne rien.
- Les fichiers ajoutés, supprimés ou renommés en dehors de l'application sont repérés tout seuls.

## Disque externe

- Votre disque est reconnu même si sa lettre change (E: devient F:) ou si son point de montage change sur Mac.
- Disque débranché : un bandeau « Disque … déconnecté » s'affiche, les morceaux restent visibles en grisé et ne sont plus modifiables. Rebranchez-le : tout revient seul.
- Pour un disque utilisé sur Windows et sur Mac, le format **exFAT** est recommandé.

## Le tableau

- **Rechercher** : tapez dans la barre du haut (titre, artistes, album, genre, nom de fichier ; les accents sont ignorés).
- **Filtres rapides** au-dessus du tableau : format, sans pochette, fichiers à problème, note minimale. Un dossier choisi dans l'arborescence filtre aussi le tableau (puce « Dossier : … », cliquez dessus pour l'enlever).
- **Trier** : cliquez sur un en-tête (une deuxième fois pour inverser). Maj + clic ajoute une colonne de tri.
- **Colonnes** : clic droit sur les en-têtes pour en afficher ou masquer ; glissez un en-tête pour changer l'ordre ; tirez son bord droit pour la largeur. Tout est retenu.
- **Sélection** : clic, Ctrl + clic (ajouter), Maj + clic (plage), Ctrl+A (tout), flèches du clavier.
- Badges de la colonne État : hors ligne, manquant, illisible, cadenas, WAV, tags désaccordés.

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

> Attention : un morceau déjà importé dans Rekordbox et renommé ici y apparaîtra comme manquant. La protection automatique des morceaux de Rekordbox arrive en phase 4 ; d'ici là, renommez de préférence les morceaux pas encore importés.

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
- L'écran **Dossiers** (barre latérale) liste les dossiers suivis : ajouter, retirer (aucun fichier n'est touché), réindexer.
- **Ctrl+Maj+R** affiche le fichier sélectionné dans l'Explorateur ou le Finder.

## Recherche en ligne (phase 2)

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
- Choisissez votre dossier de téléchargements (Beatport, Bandcamp…) au premier lancement ou dans l'écran **Inbox**.
- Chaque nouveau fichier y est identifié, tagué, pourvu d'une pochette et renommé automatiquement **quand c'est sûr** ; sinon il passe par À valider.
- Quand les morceaux sont prêts : **Ranger…** puis cliquez sur le dossier de destination (le dernier utilisé est proposé).
- Un morceau traité dans l'Inbox avant son import dans Rekordbox n'y aura jamais de lien cassé.

## Palette de commandes
**Ctrl+K** (Cmd+K sur Mac) : tapez une action (« renommer », « nettoyer », « aller à »…) ou le nom d'un morceau, puis Entrée.

## Réglages

- **Apparence**, **Langue**, **Densité du tableau**, **Notation des tonalités**.
- **Recherche en ligne** : sources, clés AcoustID, contact, seuils, remplacement des valeurs existantes, renommage automatique, test de connexion, journal des appels.
- **Écriture des tags** : version dans le titre, décimales du BPM, une seule pochette par fichier, séparateur des artistes, taille des pochettes.
- **Base de données** : une copie est faite chaque jour (7 gardées). « Exporter la base… » pour en garder une copie où vous voulez ; « Importer une base… » la remettra en place au prochain démarrage.
- **Signaler un problème** : copie un rapport à coller dans votre message.
