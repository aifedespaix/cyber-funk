---
name: live-safety
description: Use for anything that affects reliability or audience safety during a live cyber-funk show - photosensitive flash limiter (WCAG 2.3.1), panic/blackout/safe look, source loss fallbacks, autosave and crash resume, asset/shader preloading, soak tests.
---

# Sécurité en live

Un plantage ou un flash dangereux devant 500 personnes n'a pas de « on corrige au prochain commit ».

## Limiteur photosensible 🔴

Portage de `engine/show-fx-limiter.ts`. Garanties, testées par propriétés (`proptest`) :

- au plus **3 flashs par seconde glissante** ;
- aucune variation de luminance plus rapide que `MIN_FADE = 0,08 s` (montée comme descente) ;
- pic de flash plein écran ≤ `MAX_FLASH_PEAK = 0,4`, vague de changement ≤ `MAX_SWEEP_PEAK = 0,14` ;
- écart minimal 0,25 s entre deux départs ;
- **jamais de rouge saturé plein écran** (`defuseRed`) ;
- rien du tout quand les flashs sont coupés (`FlashKill`) ou en mode respiration.

Tout effet de luminance plein écran (flash, strobe, vague, blackout qui se rallume) passe par lui, **en dernier**,
quelle que soit la demande (look, macro, opérateur).

## Replis

- `Blackout` : fondu vers le noir en ≥ `MIN_FADE`, sortie par fondu aussi.
- `SafeLook` : look calme toujours préchargé ; utilisé automatiquement si un look échoue au chargement.
- Perte du MIDI Clock > 2 s → tempo audio ; perte de l'audio → le monde ralentit en douceur (`live = false`).
- Perte d'un contrôleur MIDI : reconnexion automatique par nom, le clavier reste toujours actif.
- Panique d'un système (bug) : attrapée au niveau du système quand c'est possible, journalisée, le show continue
  avec le système désactivé plutôt que de quitter.

## Sauvegarde et reprise

- État courant (créneau, cue, temps écoulé, macros, graine RNG, DJ armé) écrit toutes les 5 s dans le dossier de
  données, **écriture atomique** (fichier temporaire + `rename`).
- Au démarrage, si une sauvegarde de moins de 15 min existe (fenêtre de Sillage), proposer la reprise en un clic.

## Préchargement

À l'ouverture d'un show : charger tous les assets référencés par toutes les listes de cues, compiler tous les
pipelines (scène de préchauffe hors écran), générer les textures procédurales (enseignes, glyphes). Afficher la
progression. **Rien ne se charge pendant le show.** Un asset manquant est signalé à l'ouverture, pas au moment du cue.

## Tests d'endurance

- Simulation accélérée de 6 h (logique seule) : mémoire stable, pas de dérive d'horloge, compteurs bornés.
- Test réel de 6 h avec rendu : courbe mémoire CPU/GPU plate après le premier quart d'heure, aucune image > 50 ms
  hors changement de DJ.
