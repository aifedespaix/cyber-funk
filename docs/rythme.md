# Le contrat rythmique : qui lit quoi

> Lot R1. Chaque effet visuel lit le rythme par **une seule porte** : `FilteredSignals`
> (`cf-render/src/audio/routing`). Il ne lit plus directement les champs de grille ou d'accent d'`AudioFeatures`.
> Le test `crates/cf-render/tests/rhythm_contract.rs` (liste `rhythm_allowlist.txt`) l'impose : la liste ne peut que rétrécir.

## Trois couches

| Couche | Rôle | Où la lire | Dépend de la grille ? |
|---|---|---|---|
| **Mouvement** | Le corps de la danse : balancement, hochement, respiration. | `signals.beat_phase`, `signals.bar_phase`, `signals.grid_beat`, pondérés par `signals.movement(amplitude)` | Oui, pondéré par `grid_trust` |
| **Accents** | Impacts brefs : kick, pouls, charleston, attaque, transitoire. | `kick_now`, `kick_pulse()`, `kick_count`, `kick_beat`, `transient_accent`, `onset_accent`, `hat_accent`, `kick_wave` | Non : calés sur le son réel |
| **Énergie** | Le climat : basses, médiums, aigus, intensité, section, tempo. | `AudioFeatures` (`bass`, `mid`, `high`, `loud`, `intensity`, `section`, `bpm`) ou les cibles de la matrice | Non |

L'énergie reste lue dans `AudioFeatures` : elle ne dépend pas de la grille et n'a pas besoin du contrat.

## La confiance dans la grille

`signals.grid_trust` (0 à 1) est calculé une fois, dans `routing/trust.rs`, depuis la source de tempo et la confiance :

| Source | Cible |
|---|---|
| Horloge MIDI, Link | 1 |
| Grille de démo | max(confiance, 0,8) |
| Audio, tap | confiance |
| Aucun signal | 0 |

Elle est lissée : montée lente (τ = 2,0 s), descente plus rapide (τ = 0,8 s), indépendante du taux d'images.
**Seule la couche Mouvement l'utilise.** Aucun effet ne lit `confidence` ou `tempo_source` lui-même.
Valeurs de départ, à régler avec la sonde de R2 sur le matériel cible.

## Horloge de danse

`signals.dance_beat()` : le temps continu de la grille (`grid_beat`, ancré sur les mesures, ne recule jamais) quand `grid_trust` dépasse 0,6 (hystérésis : on la quitte sous 0,4, sans saut ni recul),
sinon le nombre de kicks. Une mesure commence quand `floor(dance_beat / 4)` change.

## Ajouter un effet

1. Un geste calé sur le temps (balancement, respiration) : `signals.movement(amplitude) × f(signals.beat_phase)`.
2. Un coup sec (flash, impact) : `signals.kick_pulse()` ou `signals.kick_now`, **toujours via le limiteur** (règle 7).
3. Un climat : `AudioFeatures`, ou une cible de la matrice (`signals.get(Target::…)`).
4. Dans un système Bevy, prendre `Option<Res<FilteredSignals>>` (ou `RhythmInputs`, qui regroupe énergie et signaux).

## Exceptions du cliquet

Le test est une heuristique de texte : un fichier qui nomme `AudioFeatures` et lit `.pulse`, `.phase`… est signalé,
même si le champ est le sien. Ces faux positifs portent leur raison dans `rhythm_allowlist.txt`. `controls/bridge.rs`
est le producteur de la grille.

## Non fait ici

- Les coupes `Beat/Bar/Phrase` du réalisateur (`cf-cinema` `CueTracker`) comptent encore les kicks. Les recaler sur la
  grille attend la sonde de R2 et C9.
- Le réglage fin de `grid_trust` et de `KICK_PULSE_TAU_S` (0,11 s) se juge sur le matériel cible.
