---
name: show-model
description: Use when working on cf-show in cyber-funk - the Event/DjSlot/Cue/Look data model, RON show files, the show director state machine, gates, drop conductor, freshness, ShowState, advance rules and Auto/Semi/Manual modes, DJ handover.
---

# Modèle du show

Référence complète : `docs/ARCHITECTURE.md` §2. Ce skill fixe les règles d'implémentation.

## Hiérarchie

```
Event (fichier .cfshow en RON)
├── settings : nom, politique de flashs, sorties, décalage de latence, profils de contrôle communs
├── looks : bibliothèque locale (en plus de assets/looks/*.ron)
└── slots : Vec<DjSlot>
    └── DjSlot : identité (nom, logo, thème), profil audio, profil de contrôle, mode (Auto/Semi/Manual), cues: Vec<Cue>
        └── Cue : look (référence + surcharges), entrée (Cut | Fade{bars} | OnDrop | OnPhrase),
                  déclencheurs ponctuels, avancée (Manual | Bars(n) | OnDrop | At(heure) | After(durée))
```

## Règles

- **Format versionné** : champ `version: u32` en tête de chaque fichier ; migrations explicites (fonction par version)
  plutôt que `#[serde(default)]` partout. Un fichier inconnu ou invalide donne une erreur lisible (chemin du champ,
  ligne), jamais une panique.
- **Références par identifiant stable** (`LookId(String)`, slug), jamais par index.
- **Look = données** : portes (`ShowGates` de Sillage, mêmes champs), effets, couches, palette, macros. Les champs
  numériques glissent d'un look à l'autre pendant l'entrée du cue (comme `GateBlender` de `gates.ts`) ; les listes
  basculent à mi-transition.
- Les **6 niveaux de Sillage** (`engine/show/acts.ts`) deviennent `assets/looks/niveau-0-pre-intro.ron` …
  `niveau-5-cosmos.ron`, et la montée automatique de Sillage (`LEVEL_STARTS` en fraction de la durée) devient un modèle
  de liste de cues en mode Auto (`assets/shows/montee-auto.ron`).
- **Macros** : `Macro { name, drives: [(chemin_de_param, min, max)] }`. Les chemins sont validés au chargement contre la
  liste des paramètres pilotables (enum, pas de chaîne libre à l'exécution).

## Chef du spectacle (`ShowDirector`)

- Pur Rust, sans Bevy. Entrées par image : `dt`, temps, `AudioFeatures`, actions reçues. Sortie : `ShowState` mis à jour
  sur place + liste d'événements (`ShowEvent`) pour l'UI.
- Garde le contrat de `ShowState` de Sillage (`engine/show/types.ts`) : `level` devient `cue`, et on ajoute `slot`,
  `cue_progress`, `next_cue_in`, `mode`, `armed_slot`, `macros`, `master`.
- Avancée : en Semi, l'avancée automatique ne part que si personne n'a appuyé sur GO ; un changement dû « attend le
  prochain drop » jusqu'à `MAX_LATE = 90 s`, puis la phrase suivante, puis immédiatement (logique de Sillage).
- `Hold` fige toute avancée automatique.
- Passation DJ : `ArmDj(n)` prépare (préchargement), `NextDj` joue la transition (fondu de palette sur une mesure, logo,
  enseigne) puis charge le créneau suivant (profils audio et contrôle compris).
- Drop : `DropConductor` (portage de `drop.ts` + `build.ts`) prépare pendant la montée, frappe sur l'image du drop.
- Fraîcheur (`freshness.ts`) commune à toutes les familles (`shot:`, `stunt:`, `event:`, `landmark:`).
- Tout aléatoire passe par le RNG injecté du directeur (graine dans l'état sauvegardé : une reprise rejoue pareil).

## Tests

- Simulation sans rendu : un set entier rejoué en accéléré (pas de 1/60 s, `AudioFeatures` synthétiques avec drops et
  breaks), trace des changements de cue/portes/drops comparée par instantané `insta`.
- Propriétés : les nombres des portes ne sautent jamais de plus d'un pas d'interpolation par image ; un fichier
  sérialisé puis relu est identique.
