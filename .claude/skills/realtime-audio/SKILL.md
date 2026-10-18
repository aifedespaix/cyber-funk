---
name: realtime-audio
description: Use when working on cf-audio or cf-analysis in cyber-funk - audio capture/playback with cpal, decoding, ring buffers, the analysis thread, FFT, beat/section detection, AudioFeatures, latency compensation.
---

# Audio temps réel

## Les trois threads

```
callback cpal (temps réel)  ──rtrb──▶  thread d'analyse (pas fixe)  ──triple_buffer──▶  ECS (Res<AudioFeatures>)
                                                                   ──canal borné────▶  événements datés (Kick, Bar…)
```

### Callback cpal : règles absolues 🔴

- Aucune allocation (`Vec::push` qui grandit, `Box`, `String`, `format!`), aucun verrou (`Mutex`, `RwLock`), aucun
  `tracing`, aucune I/O, aucun appel système bloquant.
- Il copie des échantillons dans un `rtrb::Producer` (et, en lecture, les lit d'un `Consumer` alimenté par le
  décodeur). Si le ring est plein : on jette et on incrémente un compteur atomique (`AtomicU64`) lu ailleurs.
- Conversion de format (i16/u16/f32) et mixage en mono faits ici, sans allocation.

### Thread d'analyse

- Pas fixe : **512 échantillons à 48 kHz (≈ 10,7 ms)**, fenêtre FFT plus large glissante si besoin (Sillage :
  `AnalyserNode` fftSize et lissage 0,35, voir `audio.ts`).
- Buffers préalloués au démarrage (plans `realfft`, spectres, historiques).
- Produit un `AudioFeatures` complet à chaque pas, publié par `triple_buffer` (le lecteur prend toujours le plus
  récent, sans attente), et pousse les événements ponctuels horodatés (`Kick { t }`, `Onset`, `SectionChanged`) dans
  un canal borné (`crossbeam_channel::bounded`), `try_send` (jamais bloquant).

### Côté ECS

- Un système en `CfSet::Input` lit le triple buffer et vide le canal ; il convertit les horodatages audio en temps
  d'image en appliquant le **décalage de latence** réglable (ms) du projecteur.

## Horloges

- Temps audio = compteur d'échantillons (`u64`) converti en secondes `f64`. Ne jamais dériver le temps audio de
  l'horloge murale.
- Pistes de démo : la lecture fournit sa position exacte ; l'analyse hors ligne d'une piste (grille de temps) est mise
  en cache sur disque (clé = hash du fichier).

## `AudioFeatures` (contrat)

Reprend `BeatState` (`beat.ts`) + `SectionTracker` (`sections.ts`) : `kick` (impulsion), `bass`, `mid`, `high`, `loud`,
`pulse`, `count`, `beat` (compteur continu), `bpm: Option<f32>`, `section` (calm/groove/peak), `intensity`, plus
`tempo_source` (Clock/Link/Audio/Tap), `confidence` 0..1, `phase` dans la mesure, `bar`, `phrase`, `live` (signal
présent).

## Portage de l'analyse Sillage

- Sillage lit `getByteFrequencyData` d'un `AnalyserNode` Web Audio : fenêtre de Blackman, lissage temporel
  (`smoothingTimeConstant`), conversion en dB puis en octets entre `minDecibels` (-100) et `maxDecibels` (-30).
  **Émuler exactement ce pipeline** en Rust (module `analyser_emulation`) pour que les seuils de `beat.ts` restent
  valides, puis comparer aux données de référence (`tools/sillage-golden/`).
- Le gain automatique du micro (`BandGain` de `audio.ts`) se porte tel quel, par bande.
- Tolérances des tests golden : kicks ±1 pas d'analyse, BPM ±0,5, sections identiques hors ±1 s autour d'un
  changement.

## Tests et mesures

- Tests sur fichiers WAV/MP3 courts dans `crates/cf-analysis/tests/data/` (CC0) et signaux synthétiques (clics à
  tempo connu, bruit, silence).
- Bench `criterion` : un pas d'analyse doit coûter < 0,5 ms sur une machine de dev.
- Latence micro → image visée < 40 ms hors projecteur ; un mode diagnostic affiche le délai mesuré.
