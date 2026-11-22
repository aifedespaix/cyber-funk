# R2 · Sonde de test (MCP) : design

Date : 2026-11-22. Chantier R2 de `docs/worldbuilding/CHANTIERS.md`. Dépend de R1 (`docs/rythme.md`).

## Intention

Donner à un agent (et à l'humain qui regarde) des outils MCP pour **lancer, piloter, observer et mesurer** cyber-funk à
chaud, sans écran ni oreille : envoyer une `Action`, capturer l'image, lire l'état et le journal, et vérifier chiffres
à l'appui que l'image est calée sur le son. Le but est de faciliter le développement et le test, pas de piloter un show.
La sonde sert d'abord à régler ce que R1 n'a pas pu juger (`grid_trust`, `KICK_PULSE_TAU_S`) et débloque C5+C6, C7, C9.

## Buts

- Un serveur MCP (stdio) qui lance l'app ou s'attache à une app déjà ouverte.
- Envoyer n'importe quelle `Action` (même chemin que le clavier : `ControlAction`).
- Capturer une image du projecteur à la demande.
- Lire l'état courant (show, signaux filtrés, `grid_trust`, plan de caméra, cadence) et le journal de session.
- Mesurer la synchronisation musique/image sur une piste de démo à grille connue, en millisecondes, avec seuils.
- Survivre à un plantage de l'app : le MCP reste vivant et peut lire le journal laissé.

## Hors périmètre

- Branchement de l'entrée OSC dans l'app (`docs/osc.md` : non branché) : le canal JSON suffit ici.
- Recaler les coupes `Beat/Bar/Phrase` de `CueTracker` sur la grille (C9).
- Régler définitivement les constantes de R1 : R2 livre l'outil de mesure et une première mesure, pas le réglage final.
- Contrôle à distance sur le réseau : tout reste sur 127.0.0.1.

## Anti-buts

- La sonde ne doit exister dans **aucun build de show** : feature Cargo `probe`, absente par défaut et de la
  distribution.
- Elle ne doit jamais contourner le limiteur photosensible (règle 7) ni les règles 3 et 4 du temps réel.
- Aucune dépendance de Bevy dans le serveur MCP ni dans le protocole.

## Contraintes

- Règles de `CLAUDE.md` : pas de `unwrap`/`panic` hors tests, fichiers sous 400 lignes, commentaires en français, aucun
  secret. Nouvelles dépendances à la dernière stable (vérifier `rmcp` et son API avec Context7 avant de l'écrire).
- Les crates `cf-audio`, `cf-analysis`, `cf-control`, `cf-show` restent sans Bevy. `Action` est déjà `Serialize`/
  `Deserialize` : elle voyage telle quelle.
- Le canal n'écoute que le bouclage local ; un port aléatoire est écrit dans un fichier d'état pour `attach`.

## Approches considérées

| | Principe | Pour | Contre |
|---|---|---|---|
| **A (retenue)** | Binaire `cf-probe` (MCP stdio) + `ProbePlugin` dans l'app, canal JSON TCP local | Survit aux plantages, lance ou s'attache, un seul langage | Deux morceaux à maintenir |
| B | MCP embarqué dans l'app (HTTP) | Un seul morceau | Meurt avec l'app, inutile pour un plantage |
| C | Wrapper Python + canal JSON | Rapide à écrire | Second langage, SDK MCP Python en plus |

Choix de l'utilisateur : **A**.

## Architecture

```
agent ──MCP stdio──▶ cf-probe ──TCP 127.0.0.1 (JSON par ligne)──▶ ProbePlugin ──▶ app Bevy
                        │  lance / surveille le processus
                        └─ lit logs/session-* (même lecture que tools/debuglog/dbg.py)
```

- **Crate `cf-probe-proto`** (sans Bevy) : les types de requête/réponse (`ProbeRequest`, `ProbeReply`, `StateSnapshot`,
  `SyncReport`) en serde. Partagé par le serveur et le plugin, testé seul.
- **Crate `cf-probe`** (binaire, sans Bevy) : serveur MCP (`rmcp`, stdio). Gère le cycle de vie du processus enfant.
- **`ProbePlugin`** : dans `crates/cyber-funk/src/probe/`, derrière `feature = "probe"`. Écoute en local, traduit les
  requêtes en messages Bevy (`ControlAction`, demande de capture) et répond depuis des instantanés. Le thread réseau ne
  touche jamais le monde Bevy : il échange par canaux (`std::sync::mpsc` ou `crossbeam`), les systèmes Bevy
  consomment à chaque image sans bloquer.

## Outils MCP

| Outil | Effet |
|---|---|
| `launch` | Lance l'app (options : démo/piste, `--show`, pas de temps fixe, rendu logiciel) et attend le canal prêt |
| `attach` / `stop` | S'attache à une app ouverte ; arrête ou détache |
| `send_action` | Envoie une `Action` (JSON serde) ; renvoie l'accusé et l'état avant/après |
| `capture` | Capture du projecteur dans `captures/` ; renvoie le chemin (et l'image si demandée) |
| `state` | `ShowState`, cue, plan de caméra, `FilteredSignals`, `grid_trust`, `dance_beat`, qualité, fps |
| `log` | Journal de session, avec les filtres de `dbg.py` ; fonctionne app morte |
| `step` | En pas de temps fixe : avance de N images (déterminisme, comme `--rehearse`) |
| `measure_sync` | Lance le test de synchronisation et renvoie un `SyncReport` |

Les outils en lecture (`state`, `log`, `capture`) sont sans effet de bord ; `send_action` et `launch` sont les seuls qui
agissent.

## Test de synchronisation

- Piste de démo à grille connue (`assets/music`, licence libre ; la grille de référence est un fichier à côté).
- Pendant la lecture, le plugin enregistre **image par image** : instant (horloge audio), kicks détectés, `kick_pulse`,
  `beat_phase`, `grid_beat`, `grid_trust`, et la **réponse visible** (luminance moyenne d'une zone ou du post-process,
  lue côté CPU sur la capture réduite ou sur une valeur uniforme).
- Le rapport donne : retard moyen kick → pic visible (ms), gigue (écart-type), dérive de phase sur la piste, temps pour
  que `grid_trust` dépasse 0,6, et un verdict contre des seuils nommés (constantes, à valider avec le premier relevé).
- Reproductible : pas de temps fixe, rendu logiciel possible. Sur machine sans carte son, la piste est lue par le
  pipeline de démo existant, pas par le matériel.

## Tests de R2

- `cf-probe-proto` : aller-retour serde de tous les messages ; `Action` complète (proptest).
- `ProbePlugin` : test d'intégration sur une `App` minimale (requête → message `ControlAction`, instantané cohérent,
  requête malformée ignorée et comptée, jamais de panic).
- `cf-probe` : serveur contre une fausse app (un petit écouteur TCP), cycle de vie (lancement, plantage simulé, lecture
  du journal après la mort).
- `measure_sync` : test sur un signal synthétique à retard connu (le rapport doit le retrouver à quelques ms près).
- Ce qui n'est pas vérifiable ici (carte son, écran réel) est dit dans la PR.

## Laissé au constructeur

- Choix du transport interne entre le thread réseau et Bevy, du format exact du fichier d'état, du codage de l'image
  renvoyée par `capture` (chemin seul par défaut).
- Découpage en tâches et en fichiers (sous 400 lignes), nom exact des constantes de seuil.
- Lecture de la luminance (capture réduite ou valeur uniforme du post-process) : le moins coûteux qui reste fiable.
