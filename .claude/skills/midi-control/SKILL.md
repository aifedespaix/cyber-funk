---
name: midi-control
description: Use when working on cf-control in cyber-funk - the Action vocabulary, input mapping, keyboard bindings, MIDI input (midir), MIDI learn and profiles, MIDI Clock tempo/phase, Ableton Link, OSC, tempo source arbitration.
---

# Contrôles : Actions, MIDI, tempo

## Principe

Toute entrée (clavier, note/CC MIDI, OSC, Link) est traduite en **`Action`** par une table de correspondance. Le reste
de l'app ne connaît que les `Action`. `Action` est défini dans `cf-control` (voir `docs/ARCHITECTURE.md` §2.4).

```rust
pub enum Action {
    Go, Back, GotoCue(u16), NextDj, PrevDj, ArmDj(u16),
    Hold(Switch), Blackout(Switch), FlashKill(Switch), Breathing(Switch), SafeLook,
    ForceDrop, SkipShot, ShowDj, RandomEvent, Ascension, Title(Arc<str>), Trigger(Arc<str>),
    Macro { index: u8, value: Level }, Master(Level), LayerFade { layer: Layer, value: Level },
    TapTempo, Resync, NudgePhase { beats: f32 },
    OperatorOverlay(Switch), Playback(Switch), // famille App
}
```

`Switch` = `On | Off | Toggle` ; `Level` = `Set(f32) | Shift(f32)`. `cf-control` ne garde aucun état : c'est `cf-show`
qui résout `Toggle` et `Shift` (source : J3-01). Les identifiants texte sont des `Arc<str>` : cloner une action n'alloue pas.

Valeurs continues normalisées 0..1.

## Mapping

- `Binding { source: InputSource, action: ActionTemplate, mode: Momentary | Toggle | Absolute | Relative }`.
- `InputSource` : `Key { code, mods }`, `MidiNote { port, channel, note }`, `MidiCc { port, channel, cc }`,
  `Osc { address }`.
- Profils RON (`assets/controllers/*.ron` livrés + profils dans le fichier de show) : un profil par contrôleur,
  attachable à l'événement ou au créneau d'un DJ. Ports identifiés par **nom** (les index changent au rebranchement).
- **MIDI learn** : l'UI met le moteur en mode écoute, le prochain message reçu devient la source du binding.
- Clavier par défaut = raccourcis de Sillage (`engine/show/operator.ts`) : D drop, N plan suivant, ←/→ cue
  précédent/suivant, 0-5 aller au cue, G / Maj+G DJ suivant/précédent, T afficher le DJ, A Ascension, E événement,
  X flashs, B respiration, O surcouche opérateur, Maj+Espace lecture. Lettres lues selon la disposition (AZERTY ok).

## MIDI (`midir`)

- Thread d'entrée géré par `midir` : le callback ne fait que parser et pousser dans un canal borné (pas d'allocation
  hors démarrage, pas de verrou bloquant). Horodater à la réception.
- Rebranchement à chaud : scruter les ports toutes les ~2 s hors callback, reconnecter par nom.
- CC 14 bits (MSB/LSB) supportés pour les potards haute résolution ; CC relatifs (encodeurs) supportés.

## MIDI Clock

- 24 impulsions (0xF8) par noire. Tempo = moyenne glissante filtrée des intervalles (rejeter les aberrations, filtre
  type médiane + passe-bas), affiché avec une décimale. Start (0xFA), Continue (0xFB), Stop (0xFC), Song Position
  (0xF2) pour la phase.
- La mesure (temps 1) n'est pas transmise par le clock : la caler sur Start/SPP, ou sur l'action `Resync`.

## Arbitrage des sources de tempo

Priorité : MIDI Clock / Link présents et stables → détection audio → tap tempo. Bascule sans saut visible (la phase
est conservée et rattrapée sur une mesure). La source active et sa confiance sont publiées dans `AudioFeatures`.
Perte de clock > 2 s → repli audio automatique.

## Logiciels de mix

Traktor, Rekordbox, Serato, VirtualDJ peuvent envoyer MIDI Clock et des notes sur des boutons (hot cues, effets).
Sous Windows, un port MIDI virtuel (loopMIDI) est nécessaire pour relier deux logiciels sur la même machine ; sous
macOS, le bus IAC. Documenter dans le guide opérateur, ne pas l'embarquer.

## Tests

Parseur et mapping testés sans matériel (messages MIDI en octets). Clock testé avec des horodatages synthétiques
(gigue, perte, changement de tempo). Un binaire d'exemple `examples/midi_monitor.rs` affiche les messages reçus.
