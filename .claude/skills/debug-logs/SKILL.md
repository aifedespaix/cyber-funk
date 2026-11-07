---
name: debug-logs
description: Use when cyber-funk crashed, froze, desynchronised (image figée pendant que la musique continue), or behaved illogically; when you need the id/reference of a cue or event to improve; or to hunt a logic paradox in the show. Reads the session logs (logs/session-*) through tools/debuglog/dbg.py without loading them into context.
---

# Déboguer avec les journaux de session

**Règle : ne jamais lire `events.jsonl` en entier.** Une soirée fait des dizaines de milliers de lignes. On interroge
avec `tools/debuglog/dbg.py` (bibliothèque standard Python, sorties bornées à ~40 lignes). Tu peux enchaîner 3 à 5
requêtes courtes ; c'est toujours moins cher qu'un `Read`.

```bash
python tools/debuglog/dbg.py <commande> [options]      # --root logs (défaut), --session NOM (défaut : la dernière), --limit N
```

## Où sont les données

`logs/session-<AAAAMMJJ>-<HHMMSS>-<pid>/` (en release : `%APPDATA%/cyber-funk/sessions/`). Les 20 dernières sessions sont
gardées. `logs/LATEST.txt` nomme la plus récente.

| Fichier | Rôle |
| --- | --- |
| `session.json` | `status` = `running` ou `closed`. **`running` sur un processus mort = plantage** (aucune fermeture propre). |
| `heartbeat.json` | Dernier état connu, réécrit chaque seconde : image, horloge de simulation, pause, cue en cours, fps, audio. |
| `events.jsonl` | Un fait par ligne. |

Une ligne : `seq` (numéro continu : un trou = lignes perdues), `wall` (UTC ISO ms), `up` (ms depuis le lancement),
`sim` (ms de simulation, **pauses exclues**), `frame`, `lvl` (debug/info/warn/error), `ph` (`point`, `start`, `end` ;
`sid` relie un début à sa fin), `ev` (nom), `ref` (identifiant stable), `label` (texte lisible), `d` (détails).

`ref` des cues : `slot<N>/cue<M>` (rangs dans la feuille de conduite). C'est l'identifiant à donner pour dire « améliore ce
cue ». `label` donne « DJ · nom du cue ».

## Faits journalisés (`ev`)

`cue.active` (début à l'entrée, fin à la sortie, durée réelle) · `cue.called` · `slot.loaded` · `slot.preload` · `handover`
· `dj.show` · `dj.logo` · `title` · `toggle` · `action` (ordres clavier/MIDI, hors potards) · `world.trigger` ·
`sim.pause` / `sim.resume` / `sim.reset` · `perf.slow_frame` (image > 100 ms) · `audio.track` / `audio.underrun` /
`audio.device_error` · `watchdog.stall` / `watchdog.recovered` / `watchdog.restart` · `panic` (message, lieu, trace) ·
`log` (tout `warn!`/`error!` des bibliothèques, dont `wgpu`).

## Quelle question, quelle commande

| Question | Commande |
| --- | --- |
| Pourquoi ça a planté / figé ? | `crash` : diagnostic en une phrase (panique, gel, arrêt brutal, fermeture propre), indices (images lentes, messages graphiques), dernier battement, contexte autour du dernier incident |
| Vue d'ensemble d'une soirée | `summary` |
| Que s'est-il passé à tel moment ? | `timeline --around <sim ms> --window 3000` (ou `--up-around` pour le temps réel) ; filtres `--ev cue`, `--level warn,error`, `--skip-noise` |
| Quel est l'id du cue / événement que je veux améliorer ? | `find "nom ou fragment"` : références, durées, causes d'entrée |
| Quels faits durent trop / ne finissent jamais ? | `spans` |
| Y a-t-il un paradoxe de logique ? | `check` : deux cues actifs, horloge qui recule sans reset, activité pendant une pause, reprise sans pause, débuts sans fin, trous de `seq` |
| Quelles sessions existent ? | `sessions` (`PLANTAGE?` = jamais fermée) |

## Méthode d'enquête

1. `crash` d'abord. Lire le DIAGNOSTIC avant tout le reste.
2. Selon le diagnostic :
   - **PANIQUE** : le lieu (`fichier:ligne`) et la trace donnent le coupable ; ouvrir ce fichier à cette ligne seulement.
   - **GEL** (`watchdog.stall`) : la boucle principale a cessé de produire des images pendant que d'autres threads (audio)
     continuaient, c'est le symptôme « la musique joue, l'image est figée ». Regarder les indices : `perf.slow_frame`
     juste avant (carte graphique prise par une autre application), messages `wgpu` (périphérique ou surface perdus),
     `audio.device_error`. Puis `timeline --up-around <up du stall> --window 5000 --level warn,error`.
   - **ARRÊT BRUTAL** sans rien : plantage natif (pilote graphique, mémoire) ou processus tué. Chercher la dernière
     activité (`heartbeat.json`) et les `perf.slow_frame` qui précèdent ; croiser avec ce que l'utilisateur faisait.
3. Reproduire par le **Reset** (barre en haut de la fenêtre opérateur) : un show figé se relance sans quitter l'app, et
   `sim.reset` apparaît dans le journal.
4. Pour un **paradoxe**, lancer `check`, puis `timeline` autour de chaque numéro `#seq` cité.
5. Citer toujours `session`, `#seq` et `sim` dans le compte rendu : l'utilisateur et toi parlez du même fait.

## Ajouter un fait au journal

Pas de `tracing::info!` pour du métier : il n'est pas conservé dans `events.jsonl` (seuls `warn!`/`error!` le sont). Dans un
système Bevy : `Res<DebugLog>` puis `log.0.point(Level::Info, "mon.ev", Some("ref-stable"), Some("libellé"), json!({..}))`,
ou `begin`/`end` pour un fait qui dure (**toujours clore**, `spans` signale les oublis). Nom en `domaine.verbe`, `ref`
stable (jamais un compteur qui change), jamais dans un système chaud ni dans un callback audio (règle 3 : l'envoi alloue).
Un fait par changement d'état, pas un par image.

## Ce qui n'est pas capturé

Un plantage dans le pilote graphique tue le processus sans panique Rust : le journal ne montre alors que l'arrêt brutal et
les signes précurseurs (images lentes). Le contenu de l'écran n'est pas enregistré.
