# CyberCar — guide d'intégration web (à lire par l'agent qui branche la voiture dans l'app)

Ce document est le **contrat** entre la voiture (`src/CyberCar*.ts` + `export/car-*.glb`) et l'application web
(Three.js + TypeScript). Référence complète des nœuds/matériaux : `docs/CYBERCAR.md`.

## 0. Fichiers à copier dans l'app

| Fichier | Rôle |
|---|---|
| `export/car-high.glb`, `export/car-low.glb` | Modèles (servir en `/models/`, cache long, brotli) |
| `src/CyberCar.ts` | Contrôleur principal (point d'entrée) |
| `src/CyberCarPaint.ts` | Peinture (masque, métal, vernis, paillettes, irisation) — importé par `CyberCar.ts` |
| `src/CyberCarParticles.ts` | Étincelles / fumée — importé par `CyberCar.ts` |
| `src/CyberCarRain.ts` | Gouttes sur le pare-brise — importé par `CyberCar.ts` |
| `src/CyberCarFX.ts` | Bloom, environnement néon, route mouillée (optionnel, à utiliser côté scène) |

Dépendances : `three` 0.186.1 (version testée) et `@types/three`. Aucune autre.

## 1. Mise en route minimale

```ts
import { CyberCar } from './CyberCar';
import { createNeonFX, createWetRoad, makeNeonEnvironment } from './CyberCarFX';

const car = await CyberCar.load({ quality: 'auto' });       // 'auto' choisit high / low
scene.add(car.root);
scene.environment = makeNeonEnvironment(renderer);          // OBLIGATOIRE pour de beaux reflets
const fx = createNeonFX(renderer, scene, camera, car.quality);
car.setViewport(renderer.domElement.height, camera.fov);    // à refaire à chaque resize / changement de FOV

// boucle de rendu — car.update(dt) doit être appelé À CHAQUE FRAME (dt en secondes)
renderer.setAnimationLoop(() => {
  const dt = clock.getDelta();
  /* ... ta physique ... puis les set*() de la section 2 ... */
  car.update(dt);
  fx.render(dt);          // à la place de renderer.render(scene, camera)
});
```

**Règles à ne pas enfreindre**
1. `car.update(dt)` à chaque frame, sinon : clignotants, essuie-glaces, levier, suspension, particules, pluie, compteur et logo restent figés.
2. Les `set*()` sont **idempotents et bon marché** : appelle-les à chaque frame avec l'état courant, sans te soucier des changements.
3. Ne **fais jamais tourner un maillage** du GLB (`wheel_FL`…) : pilote les nœuds pivots (§7).
4. Le repère est celui de glTF : **+Y haut, la voiture regarde +Z, sa gauche est +X** (volant à gauche, conducteur à x = +0,35). Unités : mètres.
5. Position de la voiture : `car.root.position` / `rotation.y`. La suspension déplace un sous-groupe interne (`chassis`), jamais `root`.
6. Les particules vivent dans le **monde** (elles sont ajoutées à `car.root.parent`). Ajoute donc `car.root` à la scène **avant** le premier `update()`.

## 2. Contrat « état du jeu → appels API »

À appeler chaque frame (ou quand la valeur change) :

| État de ton jeu | Appel | Détail |
|---|---|---|
| Vitesse (km/h) | `setSpeed(kmh)` | Compteur + rotation des roues + base de la suspension auto |
| Régime moteur | `setRpm(rpm)`, `setMaxRpm(max)` | Compteur + léger tremblement de caisse |
| Rapport | `setGear('R' \| 'N' \| 1..6)` | Levier dans la grille en H + affichage |
| Braquage | `setSteering(-1..+1)` | **+1 = gauche**. Volant (±240°) + roues avant (±31°) |
| Accélérateur / frein / embrayage | `setPedals({throttle, brake, clutch})` | 0..1. `brake > 0.05` allume les feux stop |
| Phares | `setHeadlights(bool)`, `setHighBeams(bool)` | Les feux de route allument aussi les phares |
| Clignotants | `setBlinker('off' \| 'left' \| 'right' \| 'hazard')` | 1,5 Hz, avant + arrière, animés par `update()` |
| Pluie | `setRain(0..1)` | Gouttes + essuie-glaces auto (`setRain(x, false)` pour piloter les essuie-glaces à la main avec `setWipers`) |
| Dérapage / fumée | `setTireSmoke(0..1)` | Utilise ton taux de glissement des roues |
| Frottement sous la caisse | `setSparks(0..1, where?)` | `where`: `'front' \| 'rear' \| 'sills' \| 'all'` |
| Radio | `setRadioText(titre)`, `attachAnalyser(node)` | Titre défilant + barres audio |

### 2.1 Suspension / mouvement de caisse (IMPORTANT)

Tout est **interne** à `CyberCar` : le sous-groupe `chassis` (carrosserie + intérieur + lumières + caméra cockpit)
pivote autour d'un point à hauteur d'essieu, avec un ressort amorti (~1,5 Hz, un dépassement visible). Les 4 roues
restent hors du groupe (elles ne suivent pas le tangage, elles sont « au sol »).

Deux modes :

1. **Auto (défaut)** — rien à faire. L'accélération longitudinale est dérivée de `setSpeed()` (lissée) et
   l'accélération latérale de `setSteering()` : `a_lat = v² · tan(braquage) / empattement`, empattement 2,76 m.
2. **Manuel (recommandé si tu as une vraie physique)** — `car.setMotion({ longG, latG })`, en **g**.
   - `longG > 0` = on accélère → nez qui se lève, arrière qui s'écrase. `longG < 0` = freinage → nez qui plonge.
   - `latG > 0` = on tourne à **gauche** → la caisse roule vers la droite (côté gauche monte).
   - Valeurs utiles bornées à ±1,5 g. `setMotion(null)` revient en auto.
   - Mesuré : 1 g de freinage → 2,3° de tangage ; 0,8 g latéral → 2,3° de roulis.

Réglages : `setSuspensionAmount(k)` (0 = rigide, 1 = défaut, 3 = exagéré), `setEngineShake(k)`,
`bump(strength)` pour une secousse ponctuelle (bordure, saut, réception).

La voiture **ne se déplace pas** d'elle-même : elle reste en place et tourne les roues ; c'est ton monde qui défile
(ou toi qui déplaces `car.root`). Option `worldScrolls` (défaut `true`) : voir §6.

### 2.2 Phares

`setHeadlights(true)` : feux de croisement (projecteurs 0,5 rad, 55 m, + faisceau volumétrique de 16 m).
`setHighBeams(true)` : feux de route (0,3 rad, 100 m, intensité ×2,5, faisceau de 30 m).
Les projecteurs et faisceaux n'existent qu'en qualité `high` (options `spotLights`, `beams`). Sur `low`, seul
l'émissif des optiques change (aucun éclairage de la route).
Le faisceau est additif et sans profondeur : n'active pas de brouillard épais en même temps.

## 3. Logo arrière réactif au son (spécification)

Le logo arrière (`logo_rear`, matériau `logo_rear_neon`) est **plus lumineux** que les autres néons (émissif de base 10,
contre 4) et sa **lumière réagit à la musique**, ainsi que :
- l'échelle du logo (jusqu'à +5 %),
- la `PointLight` placée derrière lui (`anchor_rear_logo_light`),
- le bloom (via l'émissif).

### Comment il fonctionne

Énergie des **basses (20–150 Hz)** → niveau 0..1 → luminosité. Une détection de **beat** (basse > moyenne glissante
×1,28 + 0,06, et > 0,32, au plus 1 beat par 0,2 s) déclenche un « kick » de 0,2 s qui pousse le niveau à 0,9.
Attaque instantanée, relâchement ≈ 0,25 s. Sans audio : respiration lente (niveau 0,12 ± 0,1).

### Brancher la musique (3 options, de la plus simple à la plus fine)

```ts
// 1) Web Audio — le plus simple : la voiture lit elle-même l'analyseur à chaque update()
const ctx = new AudioContext();
const analyser = ctx.createAnalyser();
analyser.fftSize = 512;                       // 256..2048 ; plus grand = basses plus précises
ctx.createMediaElementSource(audioEl).connect(analyser).connect(ctx.destination);
car.attachAnalyser(analyser);                 // alimente aussi les barres du poste radio

// 2) Spectre fourni par toi (Howler, Tone.js, serveur…) : n'importe quelle longueur, valeurs 0..1 (ou 0..255 avec scale)
car.setAudioLevels(spectrum, 255);            // à appeler à chaque frame ; les basses sont les premiers ~5 % du tableau

// 3) Ton propre détecteur de beat / niveau
car.setLogoLevel(level01);                    // pilote directement le logo ; setLogoLevel(null) rend la main à l'audio
```

### API

| Appel | Effet |
|---|---|
| `setRearLogo({ color, intensity, boost })` | `color`: CSS (ou `null` = suit la couleur néon primaire, défaut) ; `intensity`: multiplicateur global (1) ; `boost`: force de la réaction (1,6 ; 0 = fixe) |
| `setLogoLevel(0..1 \| null)` | Pilotage manuel |
| `onBeat(cb)` | `cb(strength 0..1)` à chaque beat détecté ; renvoie une fonction de désinscription. **Utilise-le pour synchroniser le reste de l'app** (flash d'écran, caméra qui pulse, particules…) |
| `getAudioEnergy()` | `{ bass, level }` lus à la demande |

Exemple « tout pulse sur le beat » :

```ts
const off = car.onBeat((s) => {
  vignette.style.opacity = String(0.4 + 0.6 * s);   // ton UI
  car.bump(0.15 * s);                              // petite secousse de caisse
});
```

À savoir : un `AudioContext` ne démarre qu'après un geste utilisateur (`ctx.resume()` dans un handler de clic).
Mesuré en test (kick à 120 BPM synthétique) : 11 beats détectés en 6 s (attendu 12), luminosité 0,6 → 2,2 (×3,7).

## 4. Peinture

- Un **masque** (texture embarquée dans le GLB, voir `tools/`) isole la carrosserie : `setPaintColor(c)` ne teinte que les
  panneaux, pas les pneus, optiques, bandes ni décalques. Le métal et les reflets sont conservés.
- `setPaintFinish('gloss' | 'metallic' | 'matte' | 'chrome' | 'candy' | 'holo')` ou un objet partiel
  `{ metalness, roughness, clearcoat, clearcoatRoughness, flake, iridescence, rim }`.
- `rim` = liseré de lumière néon (couleurs primaire/secondaire) sur la silhouette de la peinture : c'est lui qui donne le
  look « cyberpunk » même dans le noir. 0 = aucun, 0,3–0,5 = recommandé.
- Qualité `low` : pas de clear coat ni d'irisation (shader plus léger), le reste est identique.
- Pour de beaux reflets : `scene.environment` **doit** exister (`makeNeonEnvironment` ou un HDRI).

## 5. Pluie, étincelles, fumée, route mouillée

- **Pluie** : `setRain(x)`. 150 gouttes max en `high`, 60 en `low`. Les gouttes grossissent, glissent, et sont **vraiment
  effacées** par le balayage des essuie-glaces. Elles sèchent en ≈ 3 s à l'arrêt de la pluie. Un seul `setRain` suffit.
- **Étincelles** : `setSparks(0..1, where)`, couleur `setSparkColor()`. 320 particules max (100 en `low`).
- **Fumée** : `setTireSmoke(0..1, 'rear' | 'all')`, couleur `setSmokeColor()` (teinte-la avec ton néon pour un effet synthwave).
- **Taille des particules** : appelle `car.setViewport(hauteurPx, fovDeg)` à l'init **et à chaque resize** (`hauteurPx` =
  `renderer.domElement.height`, pixel ratio inclus). Sinon elles ont une mauvaise taille à l'écran.
- **Route mouillée** : `const road = createWetRoad(); road.mesh.position.y = -0.75; scene.add(road.mesh);`
  puis `road.setWetness(0..1)` (à lier à la pluie) et `road.scroll(dt, kmh)` chaque frame si le monde défile.
  Le sol des roues est à y = −0,75.

## 6. Voiture fixe ou voiture qui roule ?

Option `CyberCar.load({ worldScrolls })` :
- `true` (défaut) : la voiture reste sur place, le décor défile. Les particules reçoivent une dérive vers l'arrière égale à la
  vitesse, donc fumée et étincelles « sont laissées derrière ».
- `false` : tu déplaces `car.root` dans la scène. Les particules restent là où elles ont été émises (monde).

## 7. Nœuds à piloter à la main (si tu sors du contrôleur)

Pilote **toujours le pivot, jamais le maillage** (la compression recentre les maillages) :
`steer_FL/FR` (`rotation.y`), `spin_FL/FR/RL/RR` (`rotation.x`), `steering_pivot` (`rotation.y`),
`pivot_gear_lever` (`rotation.x/z`), `pivot_pedal_clutch/brake/accel` (`rotation.x`),
`wiperpivot_L/R` (rotation autour de l'axe local Y, **composée** avec la quaternion de repos).
Le groupe `chassis` (créé par `CyberCar`) contient tout sauf les roues.
Accès : `car.node(name)` et `car.material(name)`.

## 8. Budgets de performance et qualité

| | `high` | `low` |
|---|---|---|
| Fichier | 2,9 Mo | 1,1 Mo |
| Triangles | ~86 k | ~42 k |
| Textures | 2k WebP (+ masque 2k) | 1k WebP |
| Projecteurs / faisceaux / lumières néon | 2 / 2 / 5 | 0 / 0 / 0 |
| Clear coat / irisation | oui | non |
| Gouttes / étincelles / fumée (max) | 150 / 320 / 220 | 60 / 100 / 70 |
| Redessin canvas (compteur, radio, pluie) | ~15–30 Hz | ~12–15 Hz |

Conseils : `renderer.setPixelRatio(Math.min(devicePixelRatio, 2))` (1,5 sur `low`), pas d'ombres portées sur la voiture,
bloom désactivé automatiquement sur `low` (`createNeonFX` rend sans post-traitement).
Les décisions `quality: 'auto'` s'appuient sur `deviceMemory`, `hardwareConcurrency` et `connection` : sur iOS ces API
n'existent pas, donc `auto` y choisit `high` — **force `quality: 'low'` pour les iPhone / iPad** si besoin.

## 9. Ce qui n'a PAS été testé (à vérifier sur appareil réel)

- Fluidité (FPS) sur mobile, et le GLB `low` sur un vrai téléphone.
- Les barres audio et le logo avec une vraie musique (testé avec un signal synthétique).
- Les nouveaux éléments intérieurs en vue cockpit longue durée (ceintures, rétroviseur, console de pavillon).
- `setRearViewTexture()` (rétroviseur alimenté par une caméra arrière) : implémenté, non essayé.

## 10. Régénérer les GLB

`bash tools/build-glb.sh` (Python avec numpy + pillow, Node ; `npm i` dans `tools/` une fois). Ce script génère le masque
de peinture, l'injecte dans le GLB, puis fabrique `car-high.glb` et `car-low.glb`. **Ne remplace pas ce script par des
commandes `gltf-transform` seules** : le masque serait perdu et la peinture retomberait en mode dégradé (détection par teinte).

---

## 11. Mode volant (hover) — guide d'implémentation

Tout le mode volant est **déjà codé dans `CyberCar.ts`**. L'application n'a qu'à décider *quand* l'activer et à synchroniser
quelques éléments (caméra, son, gameplay). **Ne réimplémente pas l'animation** : appelle l'API ci-dessous.

### 11.1 Ce que fait la voiture

Chronologie (2,4 s par défaut ; en sens inverse à l'atterrissage) :

| Progression | Ce qui se passe |
|---|---|
| 0,00 → 0,40 | Les **4 roues basculent à plat** (face extérieure vers le sol), avec un léger dépassement (~6°), descendent de 22 cm et rentrent de 10 cm |
| 0,25 → 0,70 | La **lueur de répulseur monte** : pad lumineux sous chaque roue (segments qui tournent), colonne de lumière jusqu'au sol, ondulations au sol, grande flaque de lumière sous la voiture, poussière colorée soufflée au sol, lumière sous caisse et néons plus intenses |
| 0,35 → 0,52 | La caisse **s'affaisse** de quelques cm (anticipation) |
| 0,45 → 1,00 | La caisse **monte** à `height` (0,45 m par défaut) avec ressort et petit rebond |
| ensuite | Légère oscillation verticale et balancement lent ; roulis / tangage plus marqués ; les roues tournent comme des turbines |

Pendant le vol : la direction des roues avant est ignorée (le volant du cockpit tourne toujours), les étincelles et la fumée de
pneus sont coupées, les roues suivent la caisse (tangage, roulis, hauteur), le compteur affiche « HOVER », et la lueur
respire avec les basses si de la musique est branchée (`attachAnalyser`).

### 11.2 API

| Appel | Effet |
|---|---|
| `car.setHover(true / false, { duration? })` | Lance le déploiement / l'atterrissage. Idempotent. `duration` en secondes (≥ 0,4) |
| `car.isHovering()` | `true` dès que le mode a été demandé (même pendant la transition) |
| `car.getHoverState()` | `{ phase, progress, tuck, glow, height }`. `phase` : `'grounded'`, `'deploying'`, `'hovering'` ou `'retracting'` ; `height` = hauteur actuelle de la caisse (m) |
| `car.onHoverChange(cb)` | `cb('hovering')` quand le déploiement est fini, `cb('grounded')` quand l'atterrissage est fini. Renvoie une fonction de désinscription |
| `car.setHoverHeight(m)` | Hauteur cible (0,1 → 1,6 m, défaut 0,45). **Modifiable à chaque frame** (commande d'altitude) |
| `car.setHoverColor(color ou null)` | Couleur du répulseur. `null` = suit les couleurs néon (pads et colonnes en primaire, ondulations en secondaire) |
| `car.setHoverIntensity(k)` | Force de la lueur et des oscillations (défaut 1, 0 = aucune lueur) |
| `car.setGroundLevel(y)` | Altitude du sol en repère voiture (défaut **−0,75**) pour dessiner la lueur au sol. À changer seulement si ton sol n'est pas au niveau des pneus |

Rien d'autre n'est nécessaire : `update(dt)` fait tout.

### 11.3 Branchement minimal

```ts
// 1. Entrée : une touche, un bouton, un état de jeu…
function toggleHover(): void {
  car.setHover(!car.isHovering());
}

// 2. Réagir à la fin des transitions (gameplay, son, caméra)
const off = car.onHoverChange((phase) => {
  if (phase === 'hovering') { /* physique « volante » active, son de turbines stable… */ }
  if (phase === 'grounded') { /* retour à la physique « roues » */ }
});

// 3. Chaque frame : utilise l'état pour synchroniser le reste
const s = car.getHoverState();
camera.fov = 50 + 6 * s.glow;               // exemple : léger « zoom » à l'envol
audio.turbineVolume = s.glow;               // exemple : volume des turbines
car.update(dt);
```

### 11.4 Règles de gameplay à décider (côté app, pas dans la voiture)

La voiture ne contraint rien : c'est à l'app de poser les règles. Recommandé :

1. **Conditions d'activation** : par exemple à l'arrêt ou sous X km/h pour décoller. `setHover` peut être appelé n'importe quand,
   sans danger.
2. **Physique pendant le vol** : tant que `phase !== 'grounded'`, n'applique plus l'adhérence au sol (pas de dérapage, pas de
   frottement). Passe à ta physique volante à `'hovering'` (et pas dès la demande) pour que l'animation reste cohérente.
3. **Hauteur de caisse = ta hauteur de jeu** : si ton terrain varie, pilote `setHoverHeight()` à chaque frame, ou déplace
   `car.root.position.y`. Les deux se cumulent.
4. **Inclinaison** : continue d'envoyer `setMotion({ longG, latG })` (ou laisse le mode auto) ; en vol le roulis et le tangage
   sont ×1,6, ce qui donne l'impression de planer.
5. **Vitesse affichée** : `setSpeed()` continue de piloter le compteur, mais les roues ne tournent plus à la vitesse du sol
   (elles tournent comme des turbines).
6. **`worldScrolls`** : inchangé (voir §6). Si le décor défile, la poussière au sol dérive correctement ; si tu déplaces la
   voiture, les particules restent dans le monde.

### 11.5 Caméra et ambiance (recommandations, optionnelles)

- Vue extérieure : abaisse légèrement la caméra pendant `'deploying'` pour montrer les roues, puis remonte.
- Vue cockpit : la caméra est attachée à la caisse (`attachCameraToCockpit`), elle monte et se balance donc avec elle.
- Ajoute un petit shake de caméra à `progress ≈ 0,5` (le moment où la caisse commence à se soulever).
- Le bloom (`createNeonFX`) renforce beaucoup l'effet. Sans lui, la lueur est nettement moins spectaculaire.

### 11.6 Qualité et performance

| | `high` | `low` |
|---|---|---|
| Pad lumineux par roue | oui | oui (alpha ×2,2) |
| Colonne de lumière + ondulation au sol | oui (4 + 4 maillages) | **non** |
| Flaque de lumière sous la voiture | oui | oui (alpha ×2,2) |
| Poussière au sol | 30 particules/s max | 10 particules/s max |
| Lumière sous caisse renforcée | oui (si `neonLights`) | non (pas de PointLight) |

Tous les maillages de lueur sont **additifs**, sans écriture de profondeur. Sur `low`, l'effet reste lisible mais sobre, et sans
bloom il est discret : n'attends pas le même rendu que sur `high`.

### 11.7 Points d'attention (pièges)

1. **Les roues ne sont plus enfants de `car.root` directement** : chacune est dans un groupe `tuck_FL`, `tuck_FR`, `tuck_RL`,
   `tuck_RR` (positionné au centre de la roue). **Ne touche jamais à ces groupes** et ne fais pas tourner les nœuds
   `steer_*` / `spin_*` à la main : `update()` les réécrit à chaque frame.
2. `car.root` doit être **dans la scène avant** le premier `update()` (les particules s'ajoutent à `car.root.parent`).
3. `setGroundLevel` n'affecte que le dessin de la lueur. Il ne déplace pas la voiture.
4. Si tu appelles `setHover(true)` puis `setHover(false)` avant la fin, l'animation repart **depuis la progression actuelle**
   (pas de saut) : tu peux annuler un décollage en cours.
5. La poussière réutilise le pool de fumée : avec `CyberCar.load({ effects: false })` il n'y aura ni poussière ni fumée, mais le
   reste du mode volant fonctionne.
6. Le sol de ton app doit être **visible** sous la voiture (route ou plan). La lueur est dessinée à `groundLevel + 1 cm` : un sol
   plus bas ou transparent la rendra invisible.

### 11.8 Checklist de recette

- [ ] `setHover(true)` : les roues basculent d'abord, puis la lueur monte, puis la caisse se soulève avec un petit rebond.
- [ ] À `'hovering'`, `getHoverState().height` vaut environ `setHoverHeight` (0,45 par défaut).
- [ ] `setHover(false)` : la caisse redescend d'abord, la lueur s'éteint, puis les roues se remettent debout, exactement à leur place.
- [ ] Le braquage ne fait plus pivoter les roues en vol, et les restitue après l'atterrissage.
- [ ] `onHoverChange` est appelé une seule fois par transition.
- [ ] Aucune erreur console, en `high` et en `low`.
- [ ] Mesure les FPS en `high` avec bloom pendant le vol.

### 11.9 Ce qui n'a pas été testé

- Les FPS en vol sur appareil mobile.
- Le vol en vue cockpit sur la durée (vérifié seulement en vue extérieure).
- La réaction de la lueur à de la vraie musique.
- Les roues vues d'en dessous (le dessus et le côté ont été inspectés, pas le dessous de près).
