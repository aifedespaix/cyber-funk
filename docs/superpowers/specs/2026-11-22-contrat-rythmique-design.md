# Contrat rythmique (lot R1) : design

> Statut : livré (2026-11-22). Corrections faites à l'exécution : voir la fin du document. Lot R1 du plan `docs/worldbuilding/CHANTIERS.md`.

## Intention

Le spectateur doit sentir le rythme de la musique dans l'image, comme une danse unique : tout bouge ensemble, juste, et
rien ne danse à contretemps. Aujourd'hui chaque effet décide seul de sa réaction au son.

État constaté (2026-11-22) :

- ~90 fichiers de `cf-world-neon`, `cf-cinema` et `cf-render` lisent `kick`, `pulse` ou `count` directement
  (trafic 33, plans de caméra 19, landmarks 17, enseignes 11, voiture 9…) ; ~60 lisent `phase`, `bar` ou `beat`.
- La matrice de routage (`cf-render/src/audio/routing`, `FilteredSignals`) est la porte prévue, mais seule
  l'atmosphère la lit.
- Le réalisateur coupe déjà sur la mesure et la phrase (`cf-cinema/src/cutter.rs`, `cues.rs`). Ce n'est pas le trou.
- `AudioFeatures` publie déjà `phase`, `bar`, `phrase`, `confidence`, `tempo_source` (arbitrage MIDI Clock, Link, grille
  de démo, audio, tap). La compensation de latence existe (`audio/latency.rs`, jusqu'à 500 ms).

## Buts, non-buts, anti-buts

**Buts**

- Une seule porte d'entrée du rythme pour les effets visuels : `FilteredSignals` et la matrice de routage.
- Trois couches distinctes (voir plus bas) et une règle de confiance unique.
- Aucun effet ne danse à contretemps quand la grille est douteuse.
- Une migration par modules, vérifiable à chaque étape.

**Non-buts**

- Pas de nouveaux effets visuels (C5 à C9).
- Pas de refonte de la détection de tempo ni de l'arbitre (`cf-analysis`).
- Pas de nouveau système de lecture du son à côté de la matrice.

**Anti-buts** (un échec même si tout compile)

- Une grille fausse qui fait danser l'image à contretemps.
- Un mouvement qui saute ou se fige quand la confiance change.
- Un flash qui contourne le limiteur (règle 🔴 7).
- Une allocation par image ou un verrou dans le thread audio (règles 🔴 3 et 4).

## Contraintes

- Règles 🔴 de `CLAUDE.md` : `cf-analysis`, `cf-control`, `cf-audio`, `cf-show` sans Bevy ; limiteur de flashs ; pas
  d'allocation par image ; fichiers de 400 lignes au plus.
- Les anciens champs d'`AudioFeatures` restent (règle de C1) ; la migration change qui les lit, pas leur existence.
- Un « écart à Sillage assumé » met à jour les références de parité dans le même commit (règle 7 de CHANTIERS).

## Le contrat

### Trois couches

| Couche | Rôle | Source | Dépend de la grille ? |
| --- | --- | --- | --- |
| **Mouvement** | Le corps de la danse : balancement, tangage de la caméra, respiration des lumières, traînées, défilement. | `Phase`, `Bar` (phase de la mesure) | Oui, pondéré par la confiance |
| **Accents** | Impacts brefs : coup de kick, caisse claire, charleston. | `Punch`, `Transient`, kick, onde de kick | Non : toujours calés sur le son réel |
| **Énergie** | Le climat : ampleur, couleur, densité visuelle Δ, drops, sections. | `Slow`, intensité, section | Non |

Règle de la matrice existante conservée : plus la cible est massive, plus sa constante de temps est longue ; seuls kick
et caisse claire déplacent de la géométrie ; un transitoire ne déplace jamais de géométrie.

### Règle de confiance

Un seul nombre `grid_trust` de 0 à 1, calculé une fois (côté `cf-render/audio`) à partir de `confidence` et de
`tempo_source` (horloge MIDI ou Link : 1 ; grille de démo : haut ; audio : variable ; aucun tempo : 0), puis lissé avec une
montée lente et une descente un peu plus rapide.

- Il pondère uniquement la couche **Mouvement**. À `grid_trust` bas, l'amplitude du mouvement se détend en douceur
  (pas de saut, pas de gel de la caméra).
- Les **Accents** et l'**Énergie** ne l'utilisent pas : ils restent justes sans grille.
- Aucun effet ne lit `confidence` ou `tempo_source` lui-même.

### Porte d'entrée

- Les effets lisent `FilteredSignals` (et les cibles de la matrice), jamais `AudioFeatures` directement.
- Le routage par défaut (`default_routing.ron`) gagne les routes de phase et de mesure qui manquent.
- Les `Route` utilisent déjà `quantize` (`Beat`, `HalfBeat`, `Sixteenth`, `Bar`) : on s'en sert pour le mouvement.

## Approches envisagées

1. **Mouvement lié à la phase, accents liés au son, confiance en un nombre (choisie).** Les trois couches ci-dessus.
   Fluide et anticipatrice quand la grille est sûre ; sûre aussi quand elle ne l'est pas.
2. **Tout suivre le son détecté.** Plus simple, rien ne dépend de la grille. Mais l'image arrive après le kick et saute
   au moindre faux positif : pas de sensation de danse. Écartée.
3. **Tout caler sur la grille.** Très précis quand le tempo est sûr, mais une grille fausse fait danser à contretemps.
   Écartée comme règle unique ; elle reste vraie pour la couche Mouvement, avec la confiance.

Choix confirmé par l'utilisateur dans la conversation du 2026-11-22.

## Ordre de migration

1. **Contrat et socle** : `grid_trust`, routes de phase et de mesure, doc du contrat (qui lit quoi), tests de formule
   (descente et montée sans saut, indépendance du taux d'images).
2. **Cinéma et trafic** (les plus gros consommateurs : plans de caméra, trafic).
3. **Landmarks, enseignes, voiture, pilote**.
4. **Le reste** (façades, météo, ciel, route…).
5. Un test de non-régression par module migré : plus aucune lecture directe de `kick`, `pulse`, `count` hors de la
   matrice (vérifiable par une recherche automatisée dans la CI).

Chaque étape est un commit qui compile ; le journal de `CHANTIERS.md` dit où reprendre.

## Ce qui est laissé au réalisateur de la tâche

- Le calcul exact de `grid_trust` et ses constantes de temps (valeurs de départ, à régler avec la sonde de R2).
- Quelles routes de phase ajouter par module, et leur gain.
- Le découpage des PR en commits à l'intérieur d'une étape.
- Les cas particuliers de modules où un accent doit rester lié au son sans passer par la matrice (à justifier par un
  commentaire).

## Corrections faites à l'exécution

- La migration touchait une quarantaine de fichiers « ponts » (ceux qui nomment `AudioFeatures` et lisent un champ de
  grille ou d'accent), pas ~90 lectures : le décompte initial comptait des champs sans lien (`.phase` d'une
  animation, `.pulse` d'un état local).
- Aucune route de phase ou de mesure n'a été ajoutée au routage par défaut : le Mouvement lit
  `FilteredSignals::beat_phase` / `bar_phase` / `grid_beat`, déjà publiés, pondérés par `movement()`.
- `FilteredSignals` publie en plus les accents du son réel (`kick_now`, `kick_count`, `kick_beat`, `transient_accent`,
  `onset_accent`, `hat_accent`) pour que les ponts n'aient qu'une porte.
- Le contrat est décrit dans `docs/rythme.md`.

## Vérification

- Tests de formule sans matériel (`proptest`, `insta`) pour `grid_trust` et les nouvelles routes.
- Recherche automatisée : aucune lecture directe de `kick`/`pulse`/`count` hors de la matrice et de ses tests.
- Test de synchronisation musique/image sur piste à grille connue : livré par R2 (la sonde), pas par R1.
- À juger sur le matériel cible : la sensation de danse, en particulier à la bascule de confiance.
