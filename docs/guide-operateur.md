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
projecteur), `--audio-source mic|line|demo`, `--no-midi`, `--no-operator`, `--log debug`. Un écran de chargement précède
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

## 3 bis. La fenêtre opérateur : onglets et barre de statut

**Onglets.** En haut de la fenêtre : **Réglages** (`Ctrl+1`), **Statistiques** (`Ctrl+2`), **Debug** (`Ctrl+3`). En
Live, le premier onglet s'appelle **Live** et montre le pupitre du § 5 ; l'onglet Debug n'existe pas en Live (ni bouton
ni raccourci). Les chiffres seuls restent ceux des cues : c'est pourquoi les onglets demandent `Ctrl`. Le dernier onglet
ouvert est rouvert au lancement suivant. `?` affiche l'aide des raccourcis.

**Barre de statut**, sous les onglets, toujours visible (y compris en Live) : source audio, images par seconde, tempo,
DJ en cours et temps restant d'après l'horaire prévu, monde **Auto / Manuel** (en Manuel, le monde ne lance plus rien de
lui-même ; vos clics passent toujours), nombre d'événements en cours, horloge de simulation, **Pause / Reprendre** et
**Reset**. Une valeur indisponible s'affiche « — ».

- **Passer au suivant** et **Reset** demandent une confirmation : le bouton de confirmation reste grisé un court instant
  (un double-clic ne confirme pas) et disparaît seul après 5 s, au changement d'onglet ou, pour le DJ, si le DJ a changé
  entre-temps.
- **■ Noir** (ou `Échap`) bascule le noir complet, dans tous les onglets. `Échap` ne fait pas le noir quand il sert à
  autre chose : fermer un dialogue, un menu ou une liste déroulante, ou pendant la saisie d'un texte.

## 4. Préparer une soirée

Onglet **Réglages** de la fenêtre opérateur, en mode Préparation. La structure est : un **événement** contient des **créneaux** (un par
DJ), chaque créneau contient une liste de **cues**, chaque cue joue un **look**.

1. Réglages de l'événement : nom, thème de couleurs, flashs (limités ou coupés), look de repli (« look calme »).
2. Un créneau par DJ : nom, thème propre éventuel, plage de tempo, sensibilité, mode (voir plus bas).
3. Les cues du créneau : look, façon d'entrer (coupe sur un temps fort, ou fondu), et règle d'avancée (après N secondes,
   N mesures, à une heure, ou à la main).
4. Enregistrez ; le fichier est un `.ron` lisible. Le **look calme** doit exister, sinon l'ouverture est refusée.

**Modes d'un créneau** : *Auto* (le show avance seul), *Semi* (il avance seul mais GO prend la main), *Manuel* (seul GO
fait avancer).

Avant le set : ouvrez le show, vérifiez que le chargement finit sans message d'erreur, puis faites une répétition
complète avec la source Démo. Un look illisible n'empêche pas l'ouverture : il est écarté et les cues concernés jouent le
look calme (le journal le dit).

## 4 bis. Réglages sauvegardés et Debug monde

**Réglages sauvegardés.** Le logiciel retient d'un lancement à l'autre la source audio, les périphériques du micro et de
la ligne, le niveau de qualité que vous avez fixé, la voiture choisie, les deux curseurs des façades (ci-dessous) et la présentation de la fenêtre Debug monde (sections repliées, familles ouvertes).
Le fichier est `settings.ron`, dans le dossier de configuration de l'utilisateur (sous Windows
`%APPDATA%\cyber-funk\settings.ron`). Pour tout remettre à zéro, fermez l'application et supprimez ce fichier. Un fichier
abîmé ne bloque jamais le démarrage : les valeurs par défaut sont reprises. L'option `--audio-source` de la ligne de
commande l'emporte sur la source retenue. En `--soak`, rien n'est lu ni écrit.

**Debug monde.** La fenêtre s'ouvre avec l'onglet **Debug** (`Ctrl+3`), en Préparation seulement : elle n'existe pas
en Live. Dans cet onglet, `F4` ou le bouton « Debug monde » la ferme ou la rouvre ; elle est masquée dans les autres
onglets. Elle permet de choisir la voiture (dirty car ou CyberCar, instantané : les deux sont chargées à
l'ouverture) et de lancer à la main les événements du monde, rangés par famille (trafic et ciel, immeubles-vitrines),
avec une recherche. Un événement lancé passe par le même chemin que le MIDI : le limiteur de flashs reste actif.
Tout le contenu défile (pratique sur un petit écran) et se range en quatre sections repliables : Façades, Voiture,
Quartiers, Événements. Les sections repliées et les familles ouvertes sont retenues d'une session à l'autre (les
anciennes familles retenues par une version précédente sont reprises sans rien faire).

**Quartiers.** La section montre le quartier courant et sa variante de couleurs (« Banlieue, variante 2 »), le quartier
visé pendant une transition avec une barre de progression, la raison de la dernière décision de l'automatique (par
exemple « bloqué : drop imminent ») et les 5 derniers quartiers, du plus récent au plus ancien. Les commandes :

- **Type**, **Variante**, **Transition** : le quartier à forcer. Seul le dégradé existe pour l'instant ; les six autres
  transitions sont proposées mais jouées en dégradé, ce que la fenêtre signale par « (repli : dégradé) ».
- **Durée du dégradé** (1 à 10 s) : longueur des prochaines transitions.
- **Forcer** : lance tout de suite la transition vers le quartier choisi, sans attendre les règles de l'automatique
  (même pendant un drop ou une transition en cours, qu'il remplace). La ville change au loin, au bord de ce qui est
  construit (~1,2 km devant), et la couleur change quand la voiture y arrive : ~30 s à vitesse de croisière. Le
  limiteur de flashs reste actif.
- **Geler l'automatique** : l'automatique ne change plus de quartier ; « Forcer » marche toujours.

L'automatique ne change de quartier qu'avec un show chargé, et compte en temps audio : sans show, ou si l'horloge audio
n'avance pas (aucune source qui livre du son), il ne change rien ; seul « Forcer » agit. Une transition décidée par
l'automatique ne lance jamais sa couleur en pleine section Peak, avec un drop imminent ou juste après un drop : elle
attend que la musique le permette (au plus 45 s une fois la voiture arrivée).

Rien de cette section n'est sauvegardé : à chaque lancement, l'automatique repart non gelé, sans quartier forcé, avec le
choix par défaut. Les déclencheurs `district:neon`, `district:suburb` et `district:residential` restent disponibles pour
le MIDI, mais ne sont pas répétés dans la liste des événements.

En qualité basse (niveau 2 ou plus, le même seuil que les façades ci-dessous), les couleurs propres aux quartiers
s'effacent en 1,5 s, en fondu régulier, et la palette du thème revient telle quelle ; elles reviennent de même, en
1,5 s, dès que la qualité remonte. Les quartiers continuent de changer pendant ce temps (la ville elle-même), seule
leur couleur est mise de côté.

Limites connues : aucune vérification visuelle n'a encore été faite et le temps d'image avant/après n'a pas été mesuré.
Les couleurs des quartiers (`district_grades.ron`) comme la géométrie de la banlieue et du résidentiel sont un premier
réglage, à vérifier et ajuster à l'œil.

**Façades : deux curseurs.** En haut de la fenêtre Debug monde, de 0 à 1 (1 : plein effet, la valeur de départ) :

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
confirmation), Pause / Reprendre. Les mesures (images, mémoire) sont dans l'onglet **Statistiques** (`Ctrl+2`).

À retenir : **BLACKOUT, LOOK CALME, FLASHS et RESPIRATION** sont les commandes de sécurité. Les flashs passent toujours
par un limiteur (3 par seconde au plus, jamais de rouge plein écran) : même avec les flashs permis, le public ne reçoit
pas de stroboscope dangereux. En cas de doute dans la salle (personne gênée par la lumière), coupez FLASHS ou passez en
RESPIRATION.

Passation de DJ : armez le suivant (J) si l'ordre prévu change, puis G au moment où il prend les platines.

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
Les pannes injectables et leurs tests sont décrits dans `docs/PANNES.md`.

## 7. Après le set

Fermez avec la croix de la fenêtre opérateur et confirmez. Les assignations de MIDI learn sont conservées ; le show et
les looks sont de simples fichiers : sauvegardez-les.
