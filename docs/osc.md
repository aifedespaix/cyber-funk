# Entrée OSC

Feature `osc` de `cf-control` (désactivée par défaut) : `cargo build -p cf-control --features osc`. Le serveur
(`OscInput`) écoute un port UDP (`DEFAULT_OSC_PORT` = 9000) et traduit chaque message en `Action`. Les bundles sont
aplatis ; un datagramme qui n'est pas de l'OSC est compté (`stats().malformed`) puis ignoré.

Les rangs (cue, DJ, macro) comptent **à partir de 0**, comme dans les fichiers de show. Les valeurs continues sont
bornées à 0..1. Un « interrupteur » sans argument bascule ; avec un argument, 0 (ou faux) éteint, tout autre nombre
allume. Les arguments entiers, flottants, doubles et booléens sont acceptés partout où un nombre est attendu.

| Adresse | Argument | Action |
| --- | --- | --- |
| `/cf/go` | | cue suivant |
| `/cf/back` | | cue précédent |
| `/cf/cue` | rang entier | saute à ce cue |
| `/cf/hold` | interrupteur | fige l'avancée automatique |
| `/cf/dj/next`, `/cf/dj/prev` | | passation vers le suivant / précédent |
| `/cf/dj/arm` | rang entier | arme ce créneau |
| `/cf/dj/show` | | plan sur l'enseigne du DJ |
| `/cf/drop` | | drop immédiat |
| `/cf/shot/skip` | | plan caméra suivant |
| `/cf/event/random` | | événement aléatoire |
| `/cf/ascension` | | ascension |
| `/cf/master` | 0..1 | intensité globale |
| `/cf/macro/<i>` | 0..1 | macro `i` (0 à 7) |
| `/cf/layer/<nom>` | 0..1 | opacité de la couche : `background`, `world`, `foreground`, `overlay` |
| `/cf/blackout` | interrupteur | noir total |
| `/cf/safelook` | | look calme |
| `/cf/flashkill` | interrupteur | coupe les flashs |
| `/cf/breathing` | interrupteur | mode respiration |
| `/cf/tap` | | tap tempo (horodaté à la réception) |
| `/cf/resync` | | recale le temps 1 sur le prochain kick |
| `/cf/nudge` | temps (signé) | décale la phase |

Les ordres de sécurité (`blackout`, `safelook`, `flashkill`, `breathing`) passent par le même chemin que le clavier : le
limiteur photosensible reste actif dans tous les cas.

## Limites

- **Pas encore branché dans l'application** : le module est testé seul (boucle locale UDP) mais `ControlsPlugin` ne
  l'ouvre pas encore et aucune option de ligne de commande ne choisit le port.
- **Bindings OSC libres dans les profils** : non faits. Seules les adresses fixes ci-dessus existent.
- Pas d'authentification : n'exposez le port que sur un réseau de confiance (préférez `127.0.0.1`).
