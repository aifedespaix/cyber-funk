# Refonte de la fenêtre opérateur — conception (projet 1 sur 3)

Date : 2026-11-08. Statut : **brouillon à valider**.

## Contexte et intention

Le pilotage actuel est jugé illisible : trop de panneaux (pads, sécurité, modes Préparation/Live, statistiques),
espace mal employé, onglet Debug caché derrière un bouton étrange. Le principe directeur donné par l'utilisateur :
**en live tout est procédural et automatique**. L'opérateur ne change à la main que trois choses : le DJ en scène, la
source audio, et le démarrage / l'arrêt du show.

Ce document couvre le projet 1 uniquement : la fenêtre opérateur. Hors périmètre ici (projets suivants, avec leur
propre conception) :

- **Projet 2** : séquence de démarrage / arrêt dans le monde 3D (scène d'attente, voiture, apparition et
  désynchronisation Matrix des immeubles, caméra d'attente). Ce projet 1 n'expose que le **bouton** et l'**état**
  (`Attente`, `Démarrage`, `En cours`, `Arrêt`) ; tant que le projet 2 n'existe pas, le bouton pilote un état
  provisoire sans effet visuel.
- **Projet 3** : transition visuelle au changement de DJ (thème, nom sur la voiture). Ce projet 1 expose le **clic
  sur un DJ** ; l'effet est celui qui existe déjà, amélioré au projet 3.

## Buts

- Une vue Live en **trois colonnes** lisible d'un coup d'œil, sans défilement du cadre principal.
- Changer de DJ en **un clic** sur sa carte.
- Choisir la source audio (**Musiques** / **Micro**) et activer ou non le **MIDI**.
- Une **vraie playlist gérée** en mode Musiques.
- Un onglet **DJs** pour créer, modifier, supprimer des DJ.
- Un onglet **Debug** cliquable normalement, au même rang que les autres.
- Le badge de **mise à jour** dans la barre d'onglets.

## Non-buts et anti-buts

- Plus de pads, de panneau de sécurité, de panneau de « tenues » (holds), de cue manuel, de modes Auto/Semi/Manuel,
  de mode Préparation distinct. Ce qui n'est plus affiché est **retiré du code** de `cf-operator`, pas masqué
  (règle du dépôt : pas de code mort). Les actions clavier correspondantes dans `cf-control` ne sont pas touchées par
  ce projet.
- Ne pas casser les garanties du dépôt : tout flash reste soumis au limiteur photosensible (`cf-show`), aucun
  chargement pendant le show, `cf-audio` / `cf-show` restent sans Bevy.
- Échec = une interface qui garde des commandes « au cas où » : la simplicité prime.

## Disposition

Barre d'onglets : **Live | DJs | Debug**, et à droite un badge coloré « Mise à jour X disponible · Installer »
visible seulement si une mise à jour existe (réutilise `UpdateStatus` / `InstallUpdate`).

### Onglet Live — trois colonnes

**Gauche (~22 %) : DJ.** Une carte par DJ : pastille de couleur (couleur principale de son thème), logo miniature
s'il existe, nom. La carte du DJ en scène est mise en avant. Un clic sur une autre carte déclenche le changement de
DJ. Bouton « + Nouveau DJ » (ouvre l'onglet DJs sur un DJ vierge).

**Centre : état et pilotage.** Carte « État du show » (`En attente` / `Démarrage…` / `En cours · <DJ>` /
`Arrêt…`, durée écoulée), un grand bouton **▶ DÉMARRER** (vert) ou **■ ARRÊTER** (rouge) selon l'état, inactif
pendant les phases transitoires. Dessous, stats en grandes tuiles : BPM, section, FPS, niveau audio (barres).

**Droite (~30 %) : audio.**
- Ligne « Source » : bascule **Musiques | Micro**, à côté le choix de la **sortie audio** (périphérique), puis
  l'interrupteur **MIDI**.
- Mode **Micro** : sélecteur de l'**entrée audio** (périphérique) ; le lecteur et la playlist disparaissent.
- Mode **Musiques** : pas de sélecteur d'entrée ; lecteur (titre, artiste, licence, progression, précédent /
  lecture-pause / suivant, options Aléatoire et Boucle) puis playlist.
- **Playlist** : liste **défilante**, réordonnable par glisser-déposer, titre en cours surligné, double-clic pour
  lancer, croix pour retirer, durée par titre, **durée totale** et **durée restante estimée** en pied. Bouton
  « + Ajouter » : ajoute des fichiers du disque (sélecteur de fichiers).

Repli : sous une largeur minimale, la colonne DJ passe en bandeau horizontal au-dessus.

### Onglet DJs

Liste des DJ à gauche, formulaire à droite : **nom**, **couleur**, **logo** (fichier image), **texte affiché sur la
voiture**, **thème visuel** (choix parmi les thèmes existants). Actions : créer, dupliquer, supprimer (avec
confirmation), réordonner. Les modifications sont enregistrées dans l'événement actif (fichier RON) ; la validation
existante de `cf-show` s'applique (longueur du nom, etc.). Le DJ en scène peut être édité : la modification s'applique
à la prochaine apparition de son identité, sans coupure.

### Onglet Debug

Contenu actuel de l'onglet Debug du monde, sans le bouton de bascule : un onglet comme les autres, toujours
présent, y compris en live. (Décision utilisateur : cliquable, plus de verrou.)

## Contraintes techniques

- Reste dans `cf-operator` (egui via `bevy_egui`) ; la logique de playlist vit dans `cf-audio` (`Playlist` existe
  déjà : `next_track`, `prev_track`, `select`, `PlayMode`). Le **réordonnancement, l'ajout, le retrait et les
  durées** s'ajoutent à `Playlist` / `Track` côté `cf-audio`, testés sans Bevy.
- Aucune allocation par image dans les systèmes chauds de l'UI (règle 4) : la liste des pistes est mise en cache et
  reconstruite seulement quand elle change.
- Aucun `unwrap` / `expect` ; erreurs de fichiers (logo introuvable, audio illisible) affichées dans l'interface et
  journalisées, jamais de panique.
- Fichiers de moins de 400 lignes : découper `live/` en un module par colonne (`dj_column`, `center`,
  `audio_column`, `playlist_view`).
- Commentaires et docs en français.
- Tests : logique de playlist (propriétés : réordonner conserve les pistes, durée totale = somme), état du bouton
  selon l'état du show, édition de DJ (aller-retour RON). Visuel : captures `Xvfb` si possible, sinon dit dans la PR.

## Décisions à moi (à relire)

- « Sortie » = périphérique de sortie audio. Le choix d'**écran de rendu** existant est conservé dans un petit
  réglage secondaire (menu en haut à droite), pas dans la vue Live.
- Les pistes viennent de `assets/music/` et `assets/music/local/` ; l'ajout depuis le disque ajoute à la playlist de
  la session (non copié dans `assets/`), donc sans question de licence.
- Aléatoire et Boucle sont proposés mais optionnels.
- Les statistiques détaillées (graphes, journal) de l'ancien onglet Stats migrent dans l'onglet Debug ; les tuiles
  Live n'en gardent que quatre.
- La disparition des modes Auto/Semi/Manuel suppose que le comportement **automatique** devient le seul : à
  confirmer à l'implémentation en lisant `cf-show::director`.
- Le « Mode Préparation » disparaît ; l'édition des DJ passe par l'onglet DJs, sans mode séparé.

## Précisions après relecture (mes choix, à relire)

- **Audio à chaud** : déjà supporté (`AudioCommand::SelectSource`, `SelectDevice`, `Demo*`). Ce projet est donc de
  l'interface, pas de la plomberie audio. Passer de Micro à Musiques reprend la playlist là où elle était (pause).
- **Enregistrement des DJ** : sauvegarde automatique à chaque modification validée, dans le fichier du show actif.
  Sans show chargé, ou fichier en lecture seule : les DJ restent en mémoire, un bandeau le dit, aucun crash.
- **Modèle DJ** : `DjIdentity` a déjà `name`, `logo`, `theme`. Le « texte sur la voiture » est le `name` (un seul
  champ, pas de doublon). La couleur affichée dans la liste est celle du thème ; pas de nouveau champ de schéma
  (décision de l'utilisateur : la couleur du DJ, c'est son thème).
- **États du show provisoires** (avant le projet 2) : `Attente` ↔ `En cours` bascule immédiatement, sans minuterie ;
  l'état vit dans une ressource de l'opérateur côté binaire, pas dans `cf-show`. Le directeur et l'analyse audio
  tournent toujours ; le clic sur un DJ fonctionne dans tous les états.
- **Playlist** : durées lues à l'ajout, hors du thread audio ; formats = ceux déjà acceptés par `is_audio_file` ;
  fichier illisible = grisé avec la raison, sauté à la lecture ; liste de la session mémorisée entre deux lancements
  dans les réglages de l'opérateur.
- **Interrupteur MIDI** : coupe l'écoute du contrôleur seulement (horloge MIDI et profils inchangés) ; mémorisé.
- **Menu secondaire** : écran de rendu uniquement ; rien d'autre n'y migre.
- **Onglet Stats supprimé** ; ses graphes et journal vont dans Debug. `last_tab` : `settings`/`stats` → `live`.
- **Modes Auto/Semi/Manuel** : si le directeur garde un concept de mode, l'automatique devient le seul chemin et le
  reste est supprimé de `cf-show` s'il n'a plus d'appelant ; les `Action` de `cf-control` restent, sans interface.

## Reste au constructeur

Choix des widgets egui pour le glisser-déposer, mise en page exacte et couleurs, nommage des modules, migration des
préférences enregistrées (`last_tab`) vers les trois nouveaux onglets.
