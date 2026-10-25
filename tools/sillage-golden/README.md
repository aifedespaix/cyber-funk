# sillage-golden

Données de référence (*golden*) de l'analyse audio de Sillage, pour vérifier le portage Rust de `cf-analysis`
(J1-04 kicks et tempo, J1-05 sections, intensité et montées). Tâche J1-01, voir `docs/tasks/J1.md`.

Le script fait tourner **le code TypeScript d'origine de Sillage** sur les quatre pistes CC0 de `assets/music/` et
écrit un JSON par piste dans `crates/cf-analysis/tests/golden/`.

## Régénérer

Prérequis : `bun` (testé avec 1.4.2) et `ffmpeg` + `ffprobe` dans le `PATH` (testé avec 6.1.1). Aucune dépendance npm,
rien à installer avec `bun install`.

```bash
bun run tools/sillage-golden/index.ts           # régénère crates/cf-analysis/tests/golden/*.json
bun run tools/sillage-golden/index.ts --check   # recalcule et compare aux fichiers versionnés (code 1 si écart)

# Facultatif : vérifier aussi les copies octet par octet contre un clone local de Sillage.
SILLAGE_DIR=../aifedespaix.com/apps/main/src/components/sillage bun run tools/sillage-golden/index.ts --check
```

La sortie est déterministe : deux lancements sur la même machine donnent des fichiers identiques (aucune date, aucun
hasard, ordre des clés fixe). D'une machine à l'autre, une autre version de ffmpeg (décodeur MP3, rééchantillonneur)
ou du moteur JavaScript (`Math.cos`, `Math.log10` au dernier bit près) peut changer quelques octets du spectre ; le
champ `track.pcmSha256` permet de voir si le PCM décodé est le même.

## Version de Sillage

Dépôt `aifedespaix/aifedespaix.com`, branche `main`, commit **`fd344819121f292dd0646289a898c049b2e9d67f`** (`fd34481`).

| Copie conforme (dans `sillage/`) | Original | Blob git |
| --- | --- | --- |
| `sillage/engine/beat.ts` | `apps/main/src/components/sillage/engine/beat.ts` | `d5bae734eb70` |
| `sillage/engine/sections.ts` | `apps/main/src/components/sillage/engine/sections.ts` | `ac63a6b01b06` |
| `sillage/engine/show/build.ts` | `apps/main/src/components/sillage/engine/show/build.ts` | `d2d9946dc6d5` |
| `sillage/engine/show-fx-limiter.ts` | `apps/main/src/components/sillage/engine/show-fx-limiter.ts` | `d985586f0d2d` |

Ces trois fichiers ne sont **jamais modifiés** : `index.ts` recalcule leur empreinte git au lancement et s'arrête si
elle ne correspond plus au commit ci-dessus. Pour suivre une nouvelle version de Sillage : recopier les fichiers,
mettre à jour `SILLAGE_COMMIT` et `VENDORED` dans `index.ts`, régénérer, relire le diff des JSON.

Copie conforme hors de ce script : `sillage/engine/postfx-grade.ts` (blob `c4534db67ff2`), utilisée par le rendu de
référence du post-process (`postfx/`, J4-02, voir son `README.md`).

Bouchons (ce ne sont **pas** des copies) : `sillage/engine/live-palette.ts`, `sillage/engine/palette.ts` et
`stubs/three.ts` (branché par `paths` dans `tsconfig.json`). `sections.ts` en a besoin pour sa teinte d'ambiance
`mood`, qui lit le thème CSS du navigateur et ne fait pas partie des données de référence (elle relève de J4-08). Le
bouchon de `three` évite aussi que Bun installe tout seul une version arbitraire de `three` depuis le réseau.

## Ce que fait le script

1. **Décodage** (`decode.ts`) : ffmpeg décode le MP3 et le rééchantillonne à 48 kHz (rééchantillonneur `swr`), canaux
   d'origine conservés ; le mixage mono est fait ensuite comme celui de l'`AnalyserNode` : `(G + D) / 2` en float.
2. **Émulation de l'`AnalyserNode`** (`analyser.ts`) avec les réglages de Sillage (`engine/audio.ts`) : `fftSize` 2048,
   `smoothingTimeConstant` 0,35, `minDecibels` -100, `maxDecibels` -30, fenêtre de Blackman (alpha 0,16), module
   `|X[k]| / fftSize`, lissage stocké en float, dB en float, octet tronqué. Calcul aligné sur
   `realtime_analyser.cc` de Chromium (lu le 2026-10-19). Contrôle : un sinus d'amplitude 0,01 centré sur un bin lit
   169 (-53,6 dB), comme le prévoit la formule.
3. **Boucle à pas fixe de 60 i/s** (800 échantillons à 48 kHz, exactement 1/60 s), dans l'ordre de `sillage-scene.ts` :
   `BeatDetector.update(spectre, 48000, 2048, now, dt)` → `SectionTracker.update(beat, dt)` →
   `BuildDetector.update(sections.intensity, dt)` (appelé par `DropConductor`, chaque image, après les sections).
   Détecteurs neufs pour chaque piste, comme après `reset()` au changement de titre.

**Convention de pas** : le pas `i` (de 1 à `steps`) analyse la fenêtre qui se termine à l'échantillon `i × 800`
(exclu), avec `now = i / 60` s et `dt = 1/60` s. Avant le début de la piste, la fenêtre est complétée par des zéros
(tampon d'un analyseur neuf). Un kick au pas `i` est donc daté `i / 60` s dans la piste.

## Format des fichiers JSON

| Champ | Contenu |
| --- | --- |
| `sillage` | dépôt, commit et empreintes des fichiers d'origine utilisés |
| `track` | fichier, SHA-256 du MP3 et du PCM mono 48 kHz (float32 LE), fréquence et canaux d'origine, durée |
| `analysis` | paramètres : pas, réglages de l'analyseur, échantillonnage, décimales |
| `summary` | nombre de kicks, BPM tenu le plus longtemps (`steadyBpm`) et les cinq plus longs (`bpmSeconds`), BPM final, temps passé dans chaque section, nombre de changements de section et de montées |
| `kicks` | **tous** les pas où `BeatState.kick` est vrai |
| `bpm` | **tous** les changements de `BeatState.bpm` : `{ step, bpm }` (`bpm` peut valoir `null`) |
| `sections` | état initial au pas 0 (`groove`) puis **tous** les changements renvoyés par `SectionTracker.update` |
| `builds` | **tous** les pas où `BuildDetector.update` renvoie vrai |
| `samples` | `intensity`, `bass`, `mid`, `high`, `loud` toutes les 0,25 s : `samples.x[k]` est la valeur après le pas `(k + 1) × 15`, arrondie à 5 décimales |
| `spectra` | spectre en octets complet (1024 bins) aux pas 600, 3600, 7200, 10800 et 14400 (s'ils existent), pour valider l'émulation de l'analyseur bin par bin |

Les événements (kicks, BPM, sections, montées) sont complets ; seules les grandeurs continues sont échantillonnées,
pour garder des fichiers de 35 à 60 ko.

## Limites connues

- Le navigateur lit le spectre au rythme de `requestAnimationFrame`, avec une gigue et une latence de sortie ; ici le
  pas est exact. Le tampon de l'analyseur avance par blocs de 128 échantillons (quantum de rendu) dans un navigateur ;
  ici la fenêtre se termine exactement au pas, plus simple à reproduire en Rust.
- FFT en double ici, en float dans Chromium : un octet du spectre peut différer d'une unité près d'une frontière de
  troncature.
- À 60 i/s, les intervalles entre kicks sont des multiples de 1/60 s : le BPM de Sillage (médiane des intervalles,
  arrondie) ne prend que certaines valeurs (… 120, 124, 129, 133, 138, 144, 150 …). C'est le comportement de Sillage à
  60 i/s, pas un défaut du script.
- Le gain automatique du micro (`BandGain` de `audio.ts`) n'est pas exercé : les pistes sont lues comme des fichiers.

## Bruit blanc (J1-04)

`noise.ts` fait passer un bruit blanc stationnaire (`mulberry32(12345)`, 30 s, amplitude crête 0,3) dans la même chaîne
(émulation de l'analyseur puis `beat.ts` d'origine, pas de 800 échantillons) et affiche les kicks sur la sortie
standard. `beat.ts` y détecte des kicks et en tire un faux tempo (144) ; la liste est copiée dans
`crates/cf-analysis/tests/synthetic.rs` (`white_noise_matches_sillage`), qui régénère le même bruit en Rust :

```bash
bun run tools/sillage-golden/noise.ts
```

## Limiteur photosensible (J2-06)

`limiter.ts` rejoue un scénario déterministe (1500 opérations : demandes de flash, pas de temps, changements de
réglage, plus des couleurs pour `defuseRed`) sur la copie conforme `sillage/engine/show-fx-limiter.ts` et écrit le JSON
dans `crates/cf-show/tests/golden/limiter.json`, relu par `crates/cf-show/tests/limiter_golden.rs` :

```bash
bun run tools/sillage-golden/limiter.ts > crates/cf-show/tests/golden/limiter.json
```

Le script vérifie lui-même l'empreinte git de la copie (comme `index.ts` pour les autres), indépendamment d'`index.ts`.

## Niveaux et fondu des portes (J2-02)

`gates.ts` fait tourner les copies conformes `sillage/engine/show/acts.ts` et `gates.ts` (`GateBlender`, `Variation`) :
portes de chaque niveau à mix 0, 0,5 et 1, 600 images de glissement (dt variables, sauts de niveau, `snap`), 1200 images
de `Variation` avec un LCG de graine fixe. Le JSON est relu par `crates/cf-show/tests/gates_golden.rs`, qui compare aussi
les looks `assets/looks/niveau-*.ron` et `assets/shows/montee-auto.ron`.

```bash
bun run tools/sillage-golden/gates.ts > crates/cf-show/tests/golden/gates.json
```

Le script vérifie l'empreinte git des deux copies (`acts.ts` `18629ce241a7`, `gates.ts` `b7a840f8449f`, commit fd34481).

## Route et plan de route (J5-01)

`road/index.ts` fait tourner les copies conformes `road/sillage/engine/route-plan.ts`, `flight/floors.ts`, `bridges.ts`
et `random.ts` (commit fd34481 ; empreintes `c3ac51c46218`, `20894b6d8a7b`, `7991adee6546`, `0a576ed902d7`) : 240
scénarios du planificateur du pilote (graine fixe, 98 plans non vides dont 27 avec esquive), positions et tirages des 260
premières arches, emprises. Le JSON est relu par `crates/cf-world-neon` (`route::planner`, `route::bridges`). `bridges.ts`
a besoin de `three`, `building-material` et `palette` : ce sont des bouchons (`road/stubs/`, `road/sillage/engine/`), jamais
appelés, seules les fonctions pures de `bridges.ts` tournent.

```bash
bun run tools/sillage-golden/road/index.ts           # régénère crates/cf-world-neon/tests/golden/route.json
bun run tools/sillage-golden/road/index.ts --check   # recalcule et compare
```

Le rendu côte à côte de la route (`road-render/`) est décrit dans son `README.md`.

## Enseignes (J7-01)

`signs/` fait tourner les fichiers d'origine des enseignes (`formats.ts`, `show-signs.ts`, `ads.ts`, `art-icons.ts`,
`random.ts`, copiés tels quels, empreintes vérifiées) et écrit `crates/cf-world-neon/tests/golden/signs.json` : le jeu de
cartes de la route (1 216 emplacements), les slogans de départ de chaque publicité, les annonces du DJ, les icônes
choisies par slogan et la forme du catalogue. Bouchons (pas des copies) : `signs/sillage/engine/palette.ts` et
`signs/sillage/engine/show/types.ts` (types seuls, effacés à l'exécution).

```bash
bun run tools/sillage-golden/signs/index.ts           # régénère signs.json
bun run tools/sillage-golden/signs/index.ts --check   # recalcule et compare (code 1 si écart)
```
