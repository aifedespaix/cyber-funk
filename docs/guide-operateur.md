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

## 4. Préparer une soirée

Vue de préparation de la fenêtre opérateur (le nom exact de l'onglet n'a pas été vérifié). La structure est : un **événement** contient des **créneaux** (un par
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
| Diagnostic | F3 | fenêtre de mesures (images, mémoire) |

À retenir : **BLACKOUT, LOOK CALME, FLASHS et RESPIRATION** sont les commandes de sécurité. Les flashs passent toujours
par un limiteur (3 par seconde au plus, jamais de rouge plein écran) : même avec les flashs permis, le public ne reçoit
pas de stroboscope dangereux. En cas de doute dans la salle (personne gênée par la lumière), coupez FLASHS ou passez en
RESPIRATION.

Passation de DJ : armez le suivant (J) si l'ordre prévu change, puis G au moment où il prend les platines.

## 6. En cas de pépin

| Symptôme | Que faire |
| --- | --- |
| Image trop agressive, doute sur la lumière | FLASHS (X) ou RESPIRATION (B) ; si besoin LOOK CALME (L) |
| Il faut tout couper tout de suite | BLACKOUT (K, maintenu) ; relâcher rallume en fondu |
| Le show avance quand il ne faut pas | HOLD (H), puis GO à la main ; ou passer le créneau en Manuel (Ctrl+M) |
| Le monde ralentit, rien ne bouge en rythme | le son est perdu : vérifier câble et carte son, « Actualiser les périphériques » ; reprise automatique |
| Les visuels ne suivent plus le tempo | RESYNC (R) ; vérifier que le MIDI Clock est toujours envoyé ; TAP (P) en dernier recours |
| Le contrôleur ne répond plus | le clavier marche toujours ; rebrancher le contrôleur, il est repris seul |
| Un cue a le mauvais look | le fichier du look était illisible (voir le journal) : jouer GO, corriger après le set |
| L'application plante ou est fermée | relancer et rouvrir le show, puis GO jusqu'au bon cue (la reprise automatique de l'état n'est pas encore branchée dans l'application) |
| Image saccadée | F3 pour mesurer ; baisser la qualité n'est pas manuel : elle est adaptative |

Dans les versions livrées (pas en développement), le journal est écrit dans un fichier ; son dossier exact n'est pas documenté ici. Joignez-le à tout signalement.
Les pannes injectables et leurs tests sont décrits dans `docs/PANNES.md`.

## 7. Après le set

Fermez avec la croix de la fenêtre opérateur et confirmez. Les assignations de MIDI learn sont conservées ; le show et
les looks sont de simples fichiers : sauvegardez-les.
