# Guide de l'opérateur

Pour qui ne connaît pas l'application : de l'installation au dernier morceau. Deux fenêtres : le **projecteur** (ce que
voit le public) et l'**opérateur** (vos commandes, sur l'écran de l'ordinateur).

> **État de ce guide.** Les raccourcis, boutons, options et le comportement des replis viennent du code. Les
> manipulations propres à Traktor, Rekordbox, Serato, loopMIDI et au bus IAC sont décrites de mémoire, sans avoir été
> essayées avec ces logiciels : vérifiez les noms de menus dans votre version. Aucun contrôleur MIDI réel n'a encore
> servi à valider les profils fournis.

## 1. Installer

- **Windows** : lancez `cyber-funk_<version>_x64-setup.exe` (installation pour l'utilisateur courant, sans droits
  administrateur).
- **macOS** : ouvrez le `.dmg` et glissez l'application dans Applications. (Le paquet macOS n'est pas encore utilisable :
  voir `docs/distribution.md`.)
- **Linux** : lancez l'`.AppImage`, ou décompressez le `.tar.gz` (le binaire et le dossier `assets/` côte à côte).

L'application vérifie seule les mises à jour ; le panneau de mise à jour de la fenêtre opérateur dit s'il y en a une.
Ne mettez pas à jour juste avant un set.

Lancement : double-clic, ou en ligne de commande pour choisir un show :

```
cyber-funk --show assets/shows/demo.ron
```

Options utiles : `--windowed` (projecteur en fenêtre, pour répéter), `--projector-monitor <n ou nom>` (écran du
projecteur, prioritaire sur celui choisi dans Réglages), `--audio-source mic|line|demo`, `--no-midi`, `--no-operator`, `--log debug`. Un écran de chargement précède
le show : tout est préchargé à l'ouverture, **rien ne se charge pendant le set**.

## 2. Brancher le son

Dans le panneau audio de la fenêtre opérateur, choisissez la source puis le périphérique (« Actualiser les
périphériques » après un branchement).

| Source | Quand | Particularité |
| --- | --- | --- |
| **Démo** | répétition, essai sans DJ | pistes du dossier `assets/music/` (et `local/`), boutons Précédent / Lecture / Suivant |
| **Micro** | salle, sans accès à la table | gain automatique |
| **Ligne** | sortie de la table de mix vers une carte son | pas de gain automatique : réglez le niveau côté carte |

Réglez le niveau pour que l'indicateur « Kick » s'allume sur les grosses caisses sans rester allumé en permanence.
Si le périphérique est débranché, le monde ralentit en douceur ; rebranché, il reprend seul.

## 3. Brancher le MIDI

Deux usages indépendants : un **contrôleur** (pads et potards) et une **horloge MIDI** (le tempo du logiciel de mix).
Sans MIDI, le clavier fait tout. Un contrôleur débranché est repris seul au rebranchement, par son nom.

### Contrôleur

Deux profils sont fournis dans `assets/controllers/` : `keyboard-sillage` (clavier) et `generic-8knobs-8pads` (potards en
CC 70 à 77, pads en notes 36 à 43, canal 1). Si votre contrôleur envoie autre chose, utilisez le **MIDI learn** du panneau
de contrôles : choisissez une commande, touchez le pad ou le potard. Vos assignations sont gardées dans
`assets/local/user-controls.ron`.

### Horloge MIDI

Quand le logiciel de mix envoie son MIDI Clock, l'application suit son tempo (priorité à l'horloge, puis à l'audio).
S'il s'arrête plus de 2 s, elle repasse sur le tempo audio sans à-coup.

- **Windows** : créez un port virtuel avec loopMIDI (nom libre, par exemple « CyberFunk »). Dans le logiciel de mix,
  activez l'envoi du MIDI Clock vers ce port.
- **macOS** : Utilitaire Audio MIDI, fenêtre MIDI, double-clic sur « Bus IAC », cochez « Le périphérique est en
  ligne », ajoutez un port. Même réglage côté logiciel de mix.
- **Traktor, Rekordbox, Serato** : cherchez dans les préférences MIDI / Contrôleurs l'option d'envoi de l'horloge (« MIDI
  Clock », « Send clock », « Sync ») et choisissez le port virtuel. L'intitulé varie d'une version à l'autre.

Pour voir ce qui arrive : `cargo run -p cf-control --example midi_monitor` (développeurs).

## 3 bis. La fenêtre opérateur : trois onglets

**Onglets.** En haut : **Live** (`Ctrl+1`), **DJs** (`Ctrl+2`), **Debug** (`Ctrl+3`). Le dernier onglet ouvert est rouvert
au lancement suivant. À droite de la barre : le menu **⚙ Écran de rendu** et, quand une version plus récente existe, un
bouton coloré de mise à jour (grisé tant que le show joue : on n'installe jamais en plein set). `M` pose un repère dans
le journal de session, dans tous les onglets (sauf pendant la saisie d'un texte).

**Live** : trois colonnes.

- **À gauche, les DJ** : un clic sur un DJ passe la scène à ce DJ (son thème et son nom prennent la place). La pastille
  de couleur est la couleur du thème du DJ, suivie de son logo s'il en a un. **+ Nouveau DJ** ouvre l'onglet DJs.
- **Au centre**, l'état du show, les statistiques de base (tempo, section, images par seconde, niveau, source) et le gros
  bouton **Démarrer / Arrêter**. Pour l'instant il bascule l'état affiché ; la séquence 3D de départ et d'arrêt vient
  ensuite.
- **À droite, l'audio** : source (musique interne ou micro) avec le choix de la sortie ou de l'entrée juste à côté, l'interrupteur **MIDI**, puis la **playlist** : titre en cours,
  précédent / suivant, liste défilante avec la durée de chaque titre, durée totale et restante estimée, réordonner,
  ajouter des fichiers, retirer. Les titres illisibles sont signalés ⚠.

**DJs** : créer, dupliquer, supprimer, réordonner les DJ et régler nom, logo et thème. Les modifications sont
enregistrées avec la soirée et prises en compte **au prochain lancement** (le show est construit une seule fois au
démarrage).

**Debug** : deux vues, **Monde** (quartiers, événements, réglages) et **Statistiques** (ci-dessous). Les ordres au monde
sont refusés tant que le show joue.

**Debug › Statistiques** (il ne lance rien).

- En haut, quatre tuiles : images par seconde (et p99 du temps d'image), tempo détecté et confiance, événements joués
  depuis l'ouverture de la fenêtre, état du limiteur de flashs (**OK** ou **Coupés** par FLASHS ou RESPIRATION).
- Dessous, quatre cartes : énergie audio sur 60 s, bandes du spectre (basses, médiums, aigus), événements par famille,
  temps d'antenne par DJ (mesuré depuis l'ouverture). Une carte sans données l'écrit au lieu d'afficher des zéros.
- **Diagnostic détaillé et sessions de mesure** (repliable) : passes de rendu, entités, export CSV.
- **Journal de session en direct**, dans tout le bas de l'onglet : boutons de niveau (debug, info, warn, error ; au
  moins un reste allumé), filtre texte, « Garder 100 / 200 / 500 lignes », **Pause** (l'affichage se fige, le journal
  continue d'enregistrer ; **Reprendre** revient au direct), **Vider**. Cliquer le nom d'un événement (en magenta) filtre
  dessus. La liste reste collée en bas tant que vous ne remontez pas. Le compteur dit « lignes visibles / lignes
  gardées ».
- **Copier les 20 / 50 / 100 / 200 dernières** : **Copier pour un agent** copie les dernières lignes *visibles*
  (niveaux et filtre appliqués) avec un en-tête (session, date, niveaux, filtre, format) ; **Warn + erreurs** copie les
  dernières lignes warn et error quel que soit l'affichage. Collez le tout dans la conversation avec l'agent.
  « N lignes copiées » s'affiche deux secondes.
- **⧉ Copier le contexte complet** fait la même copie, avec en plus dans l'en-tête l'état de l'appli : événements actifs
  (avec leur avancement), quartier et transition, DJ et temps restant, automatisation, pause, version, mode. Le plan
  caméra n'est pas lisible : il s'écrit « inconnu ». **Marquer ce moment** (même geste que `M`) pose le repère depuis
  la barre du journal.
- Sans journal de session (disque refusé au démarrage), l'onglet affiche « Journal indisponible ».

## 4. Préparer une soirée

Onglet **Réglages** de la fenêtre opérateur, en mode Préparation. La structure est : un **événement** contient des **créneaux** (un par
DJ), chaque créneau contient une liste de **cues**, chaque cue joue un **look**.

1. Réglages de l'événement : nom, thème de couleurs, flashs (limités ou coupés), look de repli (« look calme »).
2. Un créneau par DJ : nom, thème propre éventuel, plage de tempo, sensibilité, mode (voir plus bas).
3. Les cues du créneau : look, façon d'entrer (coupe sur un temps fort, ou fondu), et règle d'avancée (après N secondes,
   N mesures, à une heure, ou à la main).
4. Enregistrez (`Ctrl+S`) ; le fichier est un `.ron` lisible. Le **look calme** doit exister, sinon l'ouverture est
   refusée. Un événement qui a des problèmes (liste rouge sous la barre) n'est pas enregistré : corrigez d'abord.

**Disposition de l'onglet.** En haut : le profil de soirée, la barre « Fichier » (import / export) et Annuler /
Rétablir (`Ctrl+Z`, `Ctrl+Maj+Z`). À gauche, la carte **DJs de la soirée** : un DJ par ligne (avec la durée prévue
si ses horaires sont remplis), le nombre de DJ et le total des minutes, « + Ajouter un DJ », « Dupliquer »,
« Supprimer » (refusé pour le dernier DJ : une soirée en garde toujours un), et « Réglages de l'événement ». Pour
réordonner : glisser une ligne, ou `Alt+↑` / `Alt+↓` sur la ligne qui a le focus (`Tab` pour l'y amener). Au centre,
la **fiche du DJ** choisi : nom, logo, thème, horaires, mode, profil audio, profils de contrôle, puis ses cues. Le pied
de la fiche dit « ● Modifications non enregistrées » ou « ✓ Enregistré » ; **Annuler** remet la fiche à sa version
enregistrée, **Enregistrer** (`Ctrl+S`) enregistre toute la soirée. Choisir un autre DJ alors que la fiche est modifiée
demande quoi faire : enregistrer, abandonner les modifications, changer sans enregistrer (elles restent en attente),
ou rester.

**Profils de soirée.** Un profil est un fichier d'événement rangé dans le dossier de configuration
(`%APPDATA%\cyber-funk\profiles\<nom>.ron` sous Windows, `~/.config/cyber-funk/profiles/` sous Linux). Le sélecteur
liste les profils lisibles par ordre alphabétique ; en choisir un l'ouvre et le rend actif. Pour en créer un, tapez son
nom puis **Nouveau** (soirée vide) ou **Dupliquer** (enregistre la soirée en cours, quelle qu'elle soit, sous ce nom :
c'est « enregistrer sous un profil »). **Supprimer** efface le fichier du profil actif, après confirmation : la soirée
reste à l'écran mais n'est plus enregistrée sur le disque (la barre d'état le dit ; le prochain `Ctrl+S` crée un
nouveau profil). Ouvrir ou créer un profil alors que la soirée est modifiée demande d'abord s'il faut l'enregistrer.
Les noms vides, avec `/ \ : * ? " < > |` ou `..`, finissant par un point, ou réservés par Windows (`CON`, `PRN`,
`AUX`, `NUL`, `COM1`…`COM9`, `LPT1`…`LPT9`, même suivis d'une extension) sont refusés.

**Où enregistre `Ctrl+S`.** Dans le profil actif s'il y en a un ; sinon dans le fichier d'où vient la soirée (ouvert
par `--show` ou **Importer**), qui est réécrit tel quel sans créer de profil ; sinon (soirée neuve, sans fichier) dans
un nouveau profil « Soirée » (ou « Soirée 2 »… si le nom est pris), qui devient actif.

**Import / export.** Tapez un chemin dans « Fichier » : **Importer** ouvre ce fichier d'événement dans l'édition ; il
devient la cible de `Ctrl+S` (la session n'a plus de profil actif, mais le profil retenu pour le prochain lancement ne
change pas). **Exporter** écrit une copie de la soirée dans ce fichier, sans changer de profil. Chaque écriture est
atomique et garde l'ancienne version en `.bak`.

**Reprise au relancement.** Au démarrage sans `--show`, cyber-funk **joue** et rouvre dans l'onglet Réglages la soirée
retenue : le profil actif, ou à défaut le dernier profil ouvert ou enregistré. Elle est chargée à l'ouverture comme
avec `--show` (même vérification, même préchargement) ; un fichier absent ou illisible est sauté (avertissement dans le
journal) et l'on démarre sans show, comme avant. `--show` garde la priorité et ne change pas la soirée retenue, ni
`--soak` ni `--no-operator` ne la lisent. Seuls vos gestes sur les profils (enregistrer, ouvrir, créer, supprimer) la
changent. Les modifications faites dans l'éditeur sont jouées **au prochain lancement** : rien ne se recharge pendant
un show. En quittant avec des modifications non enregistrées, le dialogue de sortie propose « Enregistrer et quitter ».

**Écran de sortie.** La carte « Écran de sortie du rendu » (colonne de droite de l'onglet Réglages ; dans le repli
« Écran de sortie du rendu » au-dessus des colonnes quand la fenêtre est étroite) liste les écrans branchés : numéro,
nom, résolution, fréquence, et « principal » / « projecteur ». L'écran qui porte le projecteur est en surbrillance.
Cliquer sur un autre écran y déplace **tout de suite** le projecteur (la fenêtre est déplacée, pas recréée : rien ne
se recharge) ; la fenêtre opérateur ne bouge pas. **Plein écran** passe le projecteur en plein écran ou en fenêtre
ordinaire (avec bordure et curseur, pour répéter) ; la case montre l'état réel du projecteur. Lancé avec
`--windowed`, le mode est imposé : la case est grisée. Les deux choix sont retenus pour le prochain lancement : l'écran
par son **nom** (pas son numéro, qui change quand on débranche un écran). Au démarrage, `--projector-monitor` passe
avant l'écran retenu, qui passe avant l'écran principal ; `--windowed` passe avant le plein écran retenu. Le nom
retenu doit correspondre **exactement** (sans tenir compte des majuscules) : « HDMI-1 » retenu n'ouvre jamais le show
sur « HDMI-10 » (seul `--projector-monitor` accepte un morceau de nom). Si l'écran retenu n'est pas branché ce
jour-là, le projecteur s'ouvre sur l'écran principal, le journal le dit une fois, et le réglage est **gardé** (il
servira quand l'écran reviendra). Un écran sans nom ne peut pas être retenu : il se choisit par son numéro avec
`--projector-monitor` (le numéro de l'option est donné au survol, il commence à 0) ; de même pour le second de deux
écrans du même modèle portant le même nom (seul le premier est atteignable par son nom). Avec un seul
écran, la carte l'affiche, sélectionné, avec « Un seul écran détecté ». Le bouton « Afficher le numéro sur chaque
écran (3 s) » est grisé (pas encore disponible : il faudrait une fenêtre par écran). Si la carte est utilisée en
mode Live, changer d'écran demande deux clics (le second confirme), pour qu'un clic de travers ne déplace pas la
projection devant le public ; aujourd'hui l'onglet Réglages n'est affiché qu'en Préparation.

**Modes d'un créneau** : *Auto* (le show avance seul), *Semi* (il avance seul mais GO prend la main), *Manuel* (seul GO
fait avancer).

Avant le set : ouvrez le show, vérifiez que le chargement finit sans message d'erreur, puis faites une répétition
complète avec la source Démo. Un look illisible n'empêche pas l'ouverture : il est écarté et les cues concernés jouent le
look calme (le journal le dit).

## 4 bis. Réglages sauvegardés et onglet Debug

**Réglages sauvegardés.** Le logiciel retient d'un lancement à l'autre la source audio, les périphériques du micro et de
la ligne, le niveau de qualité que vous avez fixé, la voiture choisie, les deux curseurs des façades (ci-dessous) les événements épinglés et la présentation de l'onglet Debug (sections repliées).
Le fichier est `settings.ron`, dans le dossier de configuration de l'utilisateur (sous Windows
`%APPDATA%\cyber-funk\settings.ron`). Pour tout remettre à zéro, fermez l'application et supprimez ce fichier. Un fichier
abîmé ne bloque jamais le démarrage : les valeurs par défaut sont reprises. L'option `--audio-source` de la ligne de
commande l'emporte sur la source retenue. En `--soak`, rien n'est lu ni écrit.

**Onglet Debug.** L'onglet **Debug** (`Ctrl+3`) n'existe qu'en Préparation : il disparaît en Live. Il sert à lancer à
la main les événements du monde et à voir ce qui tourne.

- **Barre du haut** : **Mode manuel** (la même bascule que la barre de statut : le monde ne lance plus rien de
  lui-même, seuls vos clics et le show lancent), la jauge **En cours** (nombre d'événements en cours, pleine à 8),
  la **recherche** (`/` y place le curseur, `Échap` la vide ; elle cherche dans le nom, l'identifiant et la famille,
  sans tenir compte des majuscules) et **Tout arrêter**. Attention : « Tout arrêter » vide seulement la liste des
  événements en cours ; le monde ne sait pas arrêter un événement en route, chacun se termine de lui-même.
- **Quatre colonnes** : Panneaux, Immeubles, Voitures, Quartiers. En haut de chaque colonne (deux tiers de la hauteur)
  les puces de lancement ; en bas (un tiers) « En cours » : nom, pourcentage et secondes restantes (« en cours » quand
  la durée est inconnue), tag `auto` quand le monde l'a lancé de lui-même, ✕ pour retirer la ligne, **Stop** pour vider
  la colonne (même remarque : la ligne disparaît, l'événement finit lui-même). Les cascades de la voiture et l'enseigne
  du DJ apparaissent dans « En cours » (Voitures, Panneaux) sans avoir de puce.
- **Puces** : un clic lance l'événement par le même chemin que le MIDI (le limiteur de flashs reste actif ; au plus 8
  ordres par image). Pendant l'événement la puce se remplit, `×N` s'affiche s'il tourne plusieurs fois. **Clic droit :
  épingler** (★, la puce remonte en tête de sa colonne ; retenu d'une session à l'autre). Les immeubles-vitrines
  s'allument tous pendant n'importe quel immeuble-vitrine : le monde ne dit pas lequel tourne.
- **Quartiers** (4ᵉ colonne) : destination, variante, transition, durée, **→ Changer de quartier** et **Geler** (voir
  ci-dessous).
- **Dock à droite** : **Plans caméra** n'offre que **Plan suivant** (comme `N`) et **Enseigne DJ** (comme `T`) ;
  choisir un plan précis ou sa transition d'entrée n'est pas encore pilotable. **Voiture** : seul **Ascension** marche
  (comme `A` : elle part au prochain temps fort, avec un show chargé) ; les autres boutons du pad (monter, descendre,
  gauche, droite, saltos, tonneaux, stabiliser) sont grisés, pas encore pilotables, et n'ont pas de touche. Plus bas,
  **Réglages monde** : les curseurs des façades et le choix de la voiture (dirty car ou CyberCar, instantané : les deux
  sont chargées à l'ouverture), en deux sections repliables retenues d'une session à l'autre.

L'ancienne fenêtre flottante « Debug monde » (`F4`) n'existe plus : son contenu est dans cet onglet.

**Tester un événement seul : le bac à sable.** Sous la barre du haut de l'onglet Debug :

- **🧹 Tout nettoyer** arrête tous les événements en cours, remet la timeline à zéro et passe en mode manuel : rien ne
  revient tout seul.
- **Quartier vide** : la route seule, sans immeubles, enseignes ni graffitis.
- **Voiture** : Arrêt, Rampant (10 %), Lent (30 %), Moitié, Normal, ou le curseur. Seule la voiture ralentit ; les
  événements gardent leur temps. Un immeuble naît devant la voiture : à l'arrêt, il apparaît mais la voiture ne vient
  pas à lui (caméra libre, ou vitesse lente).
- **⏸ Pause** : les événements cliqués pendant la pause sont **armés** (jusqu'à 8) et partent tous ensemble à la reprise.
- **Maj + clic** sur un événement le lance **seul** (nettoyage, mode manuel, puis l'événement).

**Cohabitation.** Un clic respecte les règles du monde : jamais deux structures en même temps, un seul événement de ciel,
deux événements au plus, et une structure attend (6 s au plus) si une arche est trop proche. Un événement refusé n'est
plus perdu : le bandeau **Journal** (en bas) dit pourquoi (« une structure occupe déjà la route », « une arche est trop
proche », « personne n'a pris la demande »…).

**Événements grisés.** `assets/districts/affinity.ron` dit dans quels quartiers chaque événement a sa place (rien = partout).
Hors de son quartier, une puce est grisée (`○`) : le tirage automatique ne la choisit pas, mais un clic la force pour juger
du rendu. Pour l'autoriser dans un quartier, modifier ce fichier (ou le demander à un agent).

**Timeline.** Le bandeau **Timeline** montre la séquence jouée de la seconde 0 à la fin du dernier événement : une piste par
colonne, une barre colorée nommée par événement (plus claire tant qu'il tourne), une tête de lecture. **↻ Rejouer** nettoie
puis relance les mêmes demandes aux mêmes instants ; **📋 Copier le rapport** met dans le presse-papiers ce qui a tourné,
quand, et ce qui a été refusé (avec la raison) : à coller tel quel dans une demande à un agent.

**Quartiers.** La colonne montre le quartier courant et sa variante de couleurs (« Banlieue, variante 2 »), le quartier
visé pendant une transition avec une barre de progression, la raison de la dernière décision de l'automatique (par
exemple « bloqué : drop imminent ») et les 5 derniers quartiers, du plus récent au plus ancien. Les commandes :

- **Destination**, **Variante**, **Transition** : le quartier à demander. Seul le dégradé existe pour l'instant ; les six
  autres transitions sont affichées barrées et jouées en dégradé (« (repli : dégradé) »).
- **Durée** (1 à 10 s) : longueur des prochaines transitions.
- **→ Changer de quartier** : lance tout de suite la transition vers le quartier choisi, sans attendre les règles de l'automatique
  (même pendant un drop ou une transition en cours, qu'il remplace). La ville change au loin, au bord de ce qui est
  construit (~1,2 km devant), et la couleur change quand la voiture y arrive : ~30 s à vitesse de croisière. Le
  limiteur de flashs reste actif.
- **Geler** : l'automatique ne change plus de quartier ; « Changer de quartier » marche toujours.

L'automatique ne change de quartier qu'avec un show chargé, et compte en temps audio : sans show, ou si l'horloge audio
n'avance pas (aucune source qui livre du son), il ne change rien ; seul « Changer de quartier » agit. Une transition décidée par
l'automatique ne lance jamais sa couleur en pleine section Peak, avec un drop imminent ou juste après un drop : elle
attend que la musique le permette (au plus 45 s une fois la voiture arrivée).

Rien de cette colonne n'est sauvegardé : à chaque lancement, l'automatique repart non gelé, sans quartier forcé, avec le
choix par défaut. Les déclencheurs `district:neon`, `district:suburb` et `district:residential` restent disponibles pour
le MIDI, mais n'ont pas de puce (la colonne les remplace).

En qualité basse (niveau 2 ou plus, le même seuil que les façades ci-dessous), les couleurs propres aux quartiers
s'effacent en 1,5 s, en fondu régulier, et la palette du thème revient telle quelle ; elles reviennent de même, en
1,5 s, dès que la qualité remonte. Les quartiers continuent de changer pendant ce temps (la ville elle-même), seule
leur couleur est mise de côté.

Limites connues : aucune vérification visuelle n'a encore été faite et le temps d'image avant/après n'a pas été mesuré.
Les couleurs des quartiers (`district_grades.ron`) comme la géométrie de la banlieue et du résidentiel sont un premier
réglage, à vérifier et ajuster à l'œil.

**Façades : deux curseurs.** Dans « Réglages monde » (dock de l'onglet Debug), de 0 à 1 (1 : plein effet, la valeur de départ) :

- **Suivi de la voiture** : force de la traîne de lumière que la voiture laisse sur les façades qu'elle longe. À 0, les
  façades ignorent la voiture.
- **Vie des fenêtres** : part des fenêtres qui ont leur vie propre (télé qui vacille, extinction lente, néon qui
  grésille). À 0, toutes les fenêtres restent fixes.

Les deux sont retenus dans les réglages sauvegardés. Ils ne contournent jamais le limiteur de flashs : le grésillement
rapide des néons reste soumis à lui.

**Repli en qualité basse.** Quand la qualité adaptative (ou celle que vous fixez) atteint le niveau 2 ou plus, les façades
perdent d'elles-mêmes ce qui coûte le plus : la traîne de la voiture, le scintillement rapide, la vie des fenêtres et les
écrans qui défilent (ils redeviennent de simples fenêtres). Le reste de la ville est inchangé. Les curseurs ne rétablissent
rien tant que la qualité reste basse ; ils reprennent leur effet dès que la qualité remonte.

## 4 ter. Graffitis et projecteurs holographiques

La ville porte des **graffitis muraux** (peints sur le bas des façades, surtout en banlieue) et des **projecteurs
holographiques** : une barre posée sur un toit ou au bord de la route qui projette un texte néon dans l'air. Trois
classes de taille : petit (barre de 2 m, lettres de 1 m), moyen (6 m, 3 m) et grand (20 m, 10 m) ; au plus douze
projecteurs en même temps, les plus proches de la voiture. Les textures sont cuites une seule fois, à l'ouverture
(étape « Graffitis (textures) » du chargement) : rien ne se charge pendant un show.

**Où sont les données.** Trois fichiers, à modifier sans toucher au code (relancer l'application pour les relire) :

- `assets/graffiti/words.ron` : les mots. Une entrée par ligne : `text` (le texte), `font` (nom du fichier de police sans
  extension), `style` (`Fill` plein, `Outline` contour, `Extrude` relief), `wall` (peint sur les murs), `holo`
  (projeté par un projecteur), `hues` (teintes des murs en degrés ; les hologrammes prennent la palette du show).
  **Ajouter un mot** : ajouter une ligne. Une entrée cassée est ignorée avec un avertissement dans le journal, les autres
  restent ; un fichier vide donne simplement une ville sans graffiti.
- `assets/graffiti/placements.ron` : les projecteurs posés à la main. `z_m` (distance depuis le départ, en m, positive),
  `side` (`-1` gauche, `1` droite), `height_m` (hauteur de la barre), `size` (`Small`, `Medium` ou `Large`), `word` (un
  mot de `words.ron` marqué `holo: true`) et `yaw_deg` (rotation autour de la verticale). **Ajouter un placement** :
  ajouter une ligne. Un placement hors route (distance négative), à hauteur négative ou dont le mot est inconnu est
  ignoré avec un avertissement ; un placement posé sur un projecteur automatique le remplace.
- `assets/fonts/` et `assets/fonts/LICENCES.md` : les polices. **Ajouter une police** : copier le fichier `.ttf` ou
  `.otf` dans `assets/fonts/` **et** ajouter sa ligne dans `LICENCES.md` (nom du fichier sans extension, source,
  licence, date). Une police absente, ou sans ligne de licence remplie (ou marquée `À COMPLÉTER`), fait sauter ses mots
  avec un avertissement : l'application démarre quand même. Ne versionnez que des polices dont la licence le permet.

**Réglages en Préparation (onglet Debug, `Ctrl+3`, section « Graffitis »).** Deux interrupteurs (graffitis muraux,
projecteurs), deux curseurs de densité (de 0 à 4 fois la densité livrée ; 0 coupe) et un **mot imposé** : tous les
projecteurs proches affichent ce mot, pratique pour juger une police ou un style. Ces réglages ne sont pas sauvegardés :
chaque session repart des densités livrées.

**Sécurité photosensible.** Un hologramme ne clignote jamais de lui-même : son instabilité (tranches décalées,
aberration de couleur) suit le niveau que le limiteur de flashs accorde (au plus trois changements par seconde). Quand
les flashs sont coupés, en mode respiration ou si le limiteur est indisponible, le texte reste stable et allumé.

**Qualité basse.** Les graffitis et projecteurs s'allègent dans cet ordre : d'abord les petits projecteurs lointains,
puis le halo flou, puis les graffitis muraux lointains. À la qualité minimale, seuls les projecteurs et les graffitis
proches restent. Rien ne disparaît d'un coup : les murs s'estompent avec la distance.

## 5. Jouer

Vue Live. Les gros boutons et leurs raccourcis (affichés sur chaque bouton) :

| Commande | Touche | Effet |
| --- | --- | --- |
| GO / BACK | Droite / Gauche | cue suivant / précédent |
| HOLD | H | fige l'avancée automatique (GO reste possible) |
| DJ suivant / précédent | G / Maj+G | passation vers un autre créneau |
| Armer le suivant | J | choisit le DJ visé par la prochaine passation |
| DROP | D | déclenche un drop tout de suite |
| PLAN SUIVANT | N | coupe au plan caméra suivant |
| LOGO DJ | T | plan sur l'enseigne du DJ |
| ÉVÉNEMENT / ASCENSION | E / A | événement aléatoire du monde / ascension |
| TITRE | C | affiche un carton-titre |
| **BLACKOUT** | K (maintenir) | noir complet tant que la touche est tenue |
| **LOOK CALME** | L | bascule sur le look de repli, avance figée ; GO en sort |
| FLASHS | X | coupe ou rétablit les flashs |
| RESPIRATION | B | mode doux, sans effet brusque |
| Master | PgUp / PgDn | intensité globale |
| Macros 1 à 8 | F5 à F12 (Maj = moins) | grandeurs propres au look en cours |
| TAP / RESYNC | P / R | tempo à la main / recalage de la phase |
| Phase − / + | , / . | décale la phase d'un cran |
| Mode | Ctrl+M | change le mode du créneau |

Toujours disponibles, au-dessus de la vue (barre de statut) : **■ Noir** / `Échap`, **Passer au suivant** (avec
confirmation), Pause / Reprendre, et le repère `M` du journal. Les mesures (images, mémoire) et le journal en direct sont
dans l'onglet **Statistiques** (`Ctrl+2`), lui aussi disponible en Live.

À retenir : **BLACKOUT, LOOK CALME, FLASHS et RESPIRATION** sont les commandes de sécurité. Les flashs passent toujours
par un limiteur (3 par seconde au plus, jamais de rouge plein écran) : même avec les flashs permis, le public ne reçoit
pas de stroboscope dangereux. En cas de doute dans la salle (personne gênée par la lumière), coupez FLASHS ou passez en
RESPIRATION.

Passation de DJ : armez le suivant (J) si l'ordre prévu change, puis G au moment où il prend les platines.

**Passation jouée en 3D.** Dans la vue Live, le bouton **DJ suivant ▶** lance la séquence complète (environ 45 s, jamais deux fois la même) : l'ancienne voiture ralentit pendant que les immeubles buguent de plus en plus puis s'enfoncent sous la route ; elle roule seule sur la route nue, la caméra passe devant elle et on voit arriver au loin la voiture du DJ suivant, qui se synchronise en roulant ; elle se range au centre pendant que l'ancienne se déporte sur le côté, puis l'ancienne se désynchronise (squelette, glitch, couleurs et lumières qui s'éteignent) ; enfin le portique affiche le nom du nouveau DJ et le quartier se reconstruit à ses couleurs. Côté, vitesse, point d'arrivée, variations de vitesse et cadrage de la caméra changent à chaque passation. Cliquer sur un DJ de la liste fait la même chose vers lui. Pendant une séquence (démarrage, arrêt ou passation) ces commandes sont grisées ou ignorées. Le bouton est grisé au dernier DJ. À l'arrêt, un clic sur un DJ le désigne comme celui par lequel repartira **Démarrer** ; en désigner le premier repart du premier.

## 6. En cas de pépin

| Symptôme | Que faire |
| --- | --- |
| Image trop agressive, doute sur la lumière | FLASHS (X) ou RESPIRATION (B) ; si besoin LOOK CALME (L) |
| Il faut tout couper tout de suite | BLACKOUT (K, maintenu ; relâcher rallume en fondu), ou Noir / `Échap` (bascule) |
| Le show avance quand il ne faut pas | HOLD (H), puis GO à la main ; ou passer le créneau en Manuel (Ctrl+M) |
| Le monde ralentit, rien ne bouge en rythme | le son est perdu : vérifier câble et carte son, « Actualiser les périphériques » ; reprise automatique |
| Les visuels ne suivent plus le tempo | RESYNC (R) ; vérifier que le MIDI Clock est toujours envoyé ; TAP (P) en dernier recours |
| Le contrôleur ne répond plus | le clavier marche toujours ; rebrancher le contrôleur, il est repris seul |
| Un cue a le mauvais look | le fichier du look était illisible (voir le journal) : jouer GO, corriger après le set |
| L'application plante ou est fermée | relancer et rouvrir le show, puis GO jusqu'au bon cue (la reprise automatique de l'état n'est pas encore branchée dans l'application) |
| Image saccadée | onglet Statistiques (`Ctrl+2`) pour mesurer ; baisser la qualité n'est pas manuel : elle est adaptative |

Dans les versions livrées (pas en développement), le journal est écrit dans un fichier ; son dossier exact n'est pas documenté ici. Joignez-le à tout signalement.
Sur le moment : `M` pour marquer l'instant, puis onglet Statistiques, **Warn + erreurs** ou **Copier pour un agent**, et
collez le texte dans votre signalement.
Les pannes injectables et leurs tests sont décrits dans `docs/PANNES.md`.

## 7. Après le set

Fermez avec la croix de la fenêtre opérateur et confirmez. Les assignations de MIDI learn sont conservées ; le show et
les looks sont de simples fichiers : sauvegardez-les.
