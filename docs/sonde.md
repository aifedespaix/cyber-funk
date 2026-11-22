# La sonde de test (R2)

> Lot R2. Un petit serveur MCP (`cf-probe`) qui lance l'application, lui envoie des `Action`, capture l'image, lit son
> état et son journal, et mesure la synchronisation musique/image. Conception : `docs/superpowers/specs/2026-11-22-sonde-de-test-design.md`.

## Ce que c'est

Deux morceaux :

- **`cf-probe`** (`crates/cf-probe`) : le serveur MCP, qui parle MCP sur stdin/stdout. L'agent l'appelle comme n'importe
  quel outil.
- **La sonde dans l'application** (`crates/cyber-funk/src/probe/`, feature Cargo `probe`, drapeau `--probe`) : un
  canal TCP local, une requête JSON par ligne (`crates/cf-probe-proto`). Ses actions passent par le même chemin que le
  clavier (`ControlAction`) : le limiteur photosensible les voit toutes.

Garanties de sécurité :

- **Jamais dans un build de show.** Sans la feature `probe`, le code n'existe pas dans le binaire ; sans `--probe`,
  rien n'écoute.
- **Local seulement** : le port est ouvert sur `127.0.0.1`, choisi par le système, et annoncé dans `probe.json`
  (`<dossier de données>/cyber-funk/logs/probe.json`, soit `%APPDATA%\cyber-funk\logs` sous Windows).

## Installation

```bash
cargo build -p cf-probe                          # le serveur MCP
cargo build -p cyber-funk --features probe       # l'application AVEC la sonde
```

Puis déclarer le serveur dans Claude Code, pour ce projet seulement. Sous Git Bash, préfixer par `MSYS_NO_PATHCONV=1`
(sinon le chemin Windows est réécrit) :

```bash
MSYS_NO_PATHCONV=1 claude mcp add -s local cf-probe -- C:\Users\clape\Documents\dev\cyber-funk\target\debug\cf-probe.exe
```

Une activation prend effet à la prochaine session. `launch` lance le binaire **déjà compilé** (`target/debug` ou
`target/release`, le plus récent ; jamais `cargo run`, car tuer cargo ne tuerait pas l'application sous Windows). S'il
n'existe pas, ou s'il a été compilé sans la feature, l'outil échoue en disant de lancer
`cargo build -p cyber-funk --features probe`. La réponse de `launch` dit quel binaire est lancé et de quand il date :
il peut être périmé.

## Les outils

| Outil | Rôle | Exemple de paramètres |
|---|---|---|
| `launch` | Lance l'application avec `--probe`, attend `probe.json`, se connecte. Remplace `attach`. Si elle meurt au démarrage, rend la fin de stderr. | `{ "show": "assets/shows/demo.ron", "startup_timeout_s": 180 }` (tout est optionnel ; `fixed_step`, `software_render`, `extra_args` aussi) |
| `attach` | Se connecte à une application déjà lancée avec `--probe` (lit `probe.json`). | `{}` |
| `stop` | Tue l'application lancée par `launch` et ferme la connexion. `log` reste utilisable. | `{}` |
| `send_action` | Injecte une `Action` (JSON serde de `cf_control::Action`). | `{ "action": "Go" }` ou `{ "action": { "Cue": 3 } }` |
| `state` | Instantané : phase du show, cue, plan, `beat_phase`, `bar_phase`, `grid_beat`, `grid_trust`, `kick_count`, `kick_now`, fps, temps simulé. | `{}` |
| `capture` | Capture l'image du projecteur sous ce nom (dans `captures/probe/`). | `{ "name": "avant-drop.png" }` |
| `step` | Avance la simulation de N images (1 à 1200). | `{ "frames": 120 }` |
| `log` | Lit les journaux de session via `tools/debuglog/dbg.py` : `crash`, `summary`, `timeline`, `find`, `spans`, `check`, `sessions`. Fonctionne application morte. | `{ "subcommand": "crash" }`, `{ "subcommand": "timeline", "args": ["--around", "83500", "--window", "3000"], "limit": 80 }` |
| `measure_sync` | Mesure le retard kick détecté → pic de lumière de l'image (voir plus bas). | `{ "track": "Chase", "seconds": 30 }` |

Toute erreur devient un résultat d'outil en erreur, avec une phrase qui dit quoi faire (rappeler `launch`, application
morte, etc.) ; le serveur ne plante pas.

## Parcours types

**Voir l'effet d'une action** : `launch` → `send_action` → `step` (une vingtaine d'images) → `capture` → lire l'image
→ `state` pour les nombres (phase, plan, kicks).

**Après un plantage ou un gel** : un outil répond « l'application s'est arrêtée (code…) » avec la fin de stderr, puis
`log { "subcommand": "crash" }`, éventuellement `summary` et `timeline` (voir le skill `debug-logs`). `launch` repart
d'un `probe.json` neuf : on ne s'attache jamais à une session ancienne.

**Mesurer la synchro** : `launch` → `measure_sync { "track": "Chase", "seconds": 30 }` → lire le rapport.

## Mesurer la synchronisation

`measure_sync` choisit la piste (`track` : titre, « artiste - titre » ou fragment qui n'en désigne qu'une ; absent =
piste en cours), lance la lecture, enregistre une trace (un échantillon par image : temps simulé, horloge murale,
`kick_now`, `beat_phase`, `grid_trust`, luminance moyenne) pendant `seconds` (1 à 300), puis l'analyse
(`cf_probe_proto::analyze`).

**Par défaut en temps réel** : l'application garde sa cadence normale et le serveur attend `seconds` secondes murales
sans envoyer de `Step`. `fixed_step: true` avance par pas fixes : reproductible, mais le temps simulé n'est plus
celui de l'horloge audio, donc le verdict y est le plus souvent `Unreliable`.

### Lire le rapport

```json
{
  "kicks": 54,
  "mean_delay_ms": 0.0,
  "jitter_ms": 0.0,
  "phase_drift_ms_per_min": 0.0,
  "trust_rise_s": null,
  "verdict": "Ok",
  "sim_to_wall_ratio": 1.0001900722288477,
  "thresholds": { "max_delay_ms": 60.0, "max_jitter_ms": 25.0, "max_drift_ms_per_min": 40.0 },
  "seconds": 20,
  "samples": 1135,
  "notes": []
}
```

(Rapport du premier relevé, voir la fin de ce document. Quand des kicks sont mesurés, `mean_delay_ms`, `jitter_ms` et
`phase_drift_ms_per_min` sont des nombres.)

| Champ | Sens |
|---|---|
| `kicks` | Kicks dont un pic de lumière a été trouvé dans les 0,25 s suivantes (hausse d'au moins 0,02 sur 0..1). Les autres sont ignorés, jamais mesurés sur du bruit. |
| `mean_delay_ms` | Retard moyen entre le kick détecté et le pic de luminance de l'image. `null` sans kick mesuré. |
| `jitter_ms` | Écart-type des retards (au moins 2 kicks). |
| `phase_drift_ms_per_min` | Pente du retard au fil de la piste (au moins 3 kicks). Ce n'est **pas** une dérive de phase de la grille. |
| `trust_rise_s` | Temps avant que `grid_trust` dépasse 0,6 ; `null` s'il ne le fait jamais. |
| `sim_to_wall_ratio` | Temps simulé / temps mural. Doit valoir environ 1 en temps réel. |

Verdict, dans l'ordre de gravité, contre les seuils (provisoires) 60 ms / 25 ms / 40 ms par minute :

| Verdict | Condition |
|---|---|
| `NoData` | Aucun kick mesuré (silence, piste non lue, bloom coupé par la qualité adaptative, pas trop grossier). Aucun retard n'est inventé. |
| `TooLate` | Retard moyen > 60 ms. |
| `TooJittery` | Gigue > 25 ms. |
| `Drifting` | Pente du retard > 40 ms par minute en valeur absolue. |
| `Unreliable` | Les chiffres sont rendus mais `sim_to_wall_ratio` s'écarte de 1 de plus de 10 % : mesure non comparable à un retard musique→image. Jamais `Ok` dans ce cas. |
| `Ok` | Tous les seuils sont tenus. |

Les `notes` expliquent les cas limites : échantillons écartés, durée ramenée à 300 s, trace tronquée à 36 000
échantillons, mode `fixed_step`, absence de kick.

## Honnêteté : ce que la mesure ne dit pas

- **Borne basse.** Le retard mesuré va du kick détecté à la **luminance moyenne de l'image que le moteur commande**.
  Il ne voit ni l'émissif du monde, ni le temps GPU, ni la latence de l'affichage et du projecteur. La latence vue par
  le public est au moins celle-là, jamais moins.
- **Seuils provisoires.** 60 / 25 / 40 sont des valeurs de départ, à valider sur le matériel cible.
- **Variables d'environnement non vérifiées.** `fixed_step` pose `CF_PROBE_STEP_S=0.05` et `software_render` pose
  `WGPU_FORCE_FALLBACK_ADAPTER=1` ; aucune des deux n'a été éprouvée sur l'application réelle.
- **`capture` est asynchrone.** Le fichier est écrit quelques images **après** la réponse : attendre le fichier, ou
  faire d'abord un court `step`.
- **`measure_sync` n'est valide qu'en temps réel** (ou si `sim_to_wall_ratio` ≈ 1). Une cadence instable fausse la mesure :
  surveiller `fps` avec `state`.
- **Jamais dans un show** : la sonde n'existe que dans les builds `--features probe`.
- **Local seulement** (127.0.0.1). Aucun secret ne transite.
- Pas vérifié ici : carte son réelle, projecteur, GPU de la cible (voir le journal R2 de `docs/worldbuilding/CHANTIERS.md`).

## Premier relevé

Relevé du 2026-11-22, sur la machine de développement (build `dev` avec `--features probe`, fenêtre réelle, sortie
audio réelle, GPU non précisé). L'application a été pilotée par le protocole TCP de `cf-probe-proto` avec un petit
script jetable (non versionné), pas par le serveur MCP lui-même : mêmes messages (`SelectTrack`, `Playback On`,
`StartSyncRecording`, `StopSyncRecording`), même `analyze`.

- Piste `Chase` (Yongki), lecture lancée, 3 s d'attente, puis **20 s murales en temps réel** (1135 images, 17,4 ms
  médiane entre images, 25,8 ms au pire ; fps rapporté 48 au chargement puis 58).
- `sim_to_wall_ratio` = 1,0002 : la mesure est bien en temps réel.
- 54 fronts de `kick_now` (un toutes les 0,3 s environ), `grid_trust` encore à 0,15 en fin de trace.

Rapport rendu par `analyze` (version corrigée, voir plus bas) :

```json
{ "kicks": 54, "mean_delay_ms": 0.0, "jitter_ms": 0.0, "phase_drift_ms_per_min": 0.0,
  "trust_rise_s": null, "verdict": "Ok", "sim_to_wall_ratio": 1.0001900722288477, "notes": [] }
```

**Portée de ce 0 ms.** La luminance commandée (bloom + flash + sweep) saute de 0,454 à 0,55 à l'image même du
`kick_now`, puis décroît. Le retard de 0 ms veut dire : « la commande de luminance réagit dans l'image du kick »,
avec la résolution d'une image (environ 1/60 s à la cadence réelle, soit 17 ms ici). C'est une borne basse interne.
Cela ne dit rien de la latence audio → haut-parleur, ni GPU → écran, ni de la justesse de la grille (`grid_trust`
n'a pas franchi 0,6 : `trust_rise_s` est `null`). Les seuils 60/25/40 restent provisoires. Une seule piste, une seule
machine, un seul passage. Capture prise en fin de relevé : `docs/captures/R2/chase-20s.png`.

**Ce que le relevé a démasqué.** Une première version d'`analyze` rendait `NoData` sur cette même trace : elle
cherchait la montée de luminance après l'image du kick, alors qu'elle a lieu dans cette image. La sonde a ainsi prouvé
son utilité dès son premier usage réel ; le bug est corrigé au commit 23fc9f3 (la base de comparaison est l'image
avant le kick).

Autres constats de ce relevé :

- Aucune grille de référence n'existe pour les pistes de démo (`assets/music/` ne contient que les MP3 et les
  licences) : pas de fichier `*.grid.ron` ajouté, `grid_trust` n'a pas été jugé contre une vérité terrain.
- Un `Ping` envoyé tout de suite après l'apparition de `probe.json` a reçu `Error("délai")` : l'application n'avait pas
  fini de charger. Laisser quelques secondes après `launch` avant le premier outil (`launch` attend `probe.json`,
  pas la fin du chargement).
- Le nom de `capture` est assaini et reçoit `.png` : `r2-releve.png` est devenu `r2-releve_png.png`. Donner un nom sans
  extension.
- Un build `dev` avec la feature `probe` a pris 6 min 33 s à froid.
