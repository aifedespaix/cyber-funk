# CyberCar — référence (Three.js + TypeScript)

> **Agent d'intégration web : commence par `docs/INTEGRATION.md`** (contrat état-du-jeu → API, suspension, logo réactif au son, pluie, particules, budgets, **mode volant au §11**). Ce fichier-ci est la référence détaillée des nœuds et matériaux.

Modèle : voiture cyberpunk / synthwave, conduite à **gauche** (volant à x = +0,35).
Source Blender : `CyberCar_work.blend`. Contrôleur TS : `src/CyberCar.ts`.

## 1. Fichiers

| Fichier | Taille | Usage |
|---|---|---|
| `export/car-high.glb` | ~2,8 Mo | Appareils puissants : textures 2k WebP, ~85 k triangles |
| `export/car-low.glb` | ~1,0 Mo | Petits appareils / réseau lent : textures 1k WebP, ~40 k triangles |
| `export/car_raw.glb` | ~13 Mo | Export brut non compressé (référence, ne pas servir en prod) |

Les deux GLB utilisent **Meshopt** + **WebP**. Le loader doit avoir `MeshoptDecoder` (déjà fait dans `CyberCar.load`). Aucun fichier Draco/KTX2 n'est nécessaire.

Copie les GLB dans `public/models/` (ou passe `urls` à `CyberCar.load`).

## 2. Démarrage rapide

```ts
import { CyberCar } from './CyberCar';

const car = await CyberCar.load({ quality: 'auto', onProgress: (r) => console.log(r) });
scene.add(car.root);

// dans la boucle de rendu
car.update(clock.getDelta());
```

Prérequis : `three` ≥ 0.160, un `scene.environment` (PMREM / HDRI) pour que la peinture et les vitres reflètent la lumière, et idéalement du bloom (`UnrealBloomPass`) pour que les néons « brillent » vraiment.
La police des écrans est `Orbitron` (repli `Rajdhani`, puis monospace) : charge-la dans ta page pour le rendu voulu.

## 3. Repère et unités

- Three.js : **Y haut**, la voiture **regarde +Z**, son côté gauche est **+X**. Mètres.
- Empattement : essieu avant z = +1,59, arrière z = −1,17. Voie ≈ ±0,8.
- Rotation des roues : `rotation.x` (rouler). Braquage : `rotation.y` sur `steer_FL/FR`.

## 4. API `CyberCar`

### Cycle de vie
| Méthode | Effet |
|---|---|
| `CyberCar.load(options)` | Charge le GLB. `quality`: `'auto' \| 'high' \| 'low'`, `urls`, `spotLights`, `onProgress` |
| `update(dt)` | **À appeler à chaque frame** (roues, clignotants, essuie-glaces, levier, écrans) |
| `dispose()` | Libère géométries, matériaux, textures, canvas |
| `node(name)` / `material(name)` | Accès direct à un nœud / matériau du GLB |

### Conduite
| Méthode | Valeurs |
|---|---|
| `setSteering(v)` | −1 (droite) … +1 (gauche). Tourne volant + roues avant |
| `setSpeed(kmh)` | Chiffre sur le compteur + vitesse de rotation des roues (rayon 0,39 m) |
| `setRpm(rpm)` / `setMaxRpm(max)` | Chiffre + barre de régime (zone rouge = 15 % supérieurs). Défaut max 8000 |
| `setGear(g)` | `'R' \| 'N' \| 1…6` : déplace le levier dans la grille en H, affiche le rapport |
| `setPedals({throttle, brake, clutch})` | 0…1 chacun. `brake > 0.05` allume les feux stop |

### Couleurs
| Méthode | Effet |
|---|---|
| `setPaintColor(color)` | Recolore **uniquement la peinture violette** (voir limites). `resetPaint()` revient à l'original |
| `setNeonColors(primary, secondary?)` | Change en direct tous les néons + les couleurs des écrans |
| `setNeonIntensity(k)` | Multiplicateur d'intensité (défaut 0,45 ; 0 = éteint) ; règle aussi les PointLight néon |
| `setGlassOpacity(o)` | Opacité des vitres (défaut 0,28) |

`primary` : sous-caisse, anneaux de roues, liserés tableau de bord, plancher.
`secondary` : bande latérale des portes, essuie-glaces, accents, jauge.

### Lumières
| Méthode | Effet |
|---|---|
| `setHeadlights(on)` | Les 6 barres des phares + 2 `SpotLight`. Feux arrière en veilleuse quand allumés |
| `setBraking(on)` | Feux stop (aussi piloté par `setPedals`) |
| `setBlinker(mode)` | `'off' \| 'left' \| 'right' \| 'hazard'`, 1,5 Hz : angles avant + angles arrière |

### Essuie-glaces
`setWipers('off' | 'intermittent' | 'slow' | 'fast')` ou `setRain(0..1)` (mappe l'intensité de pluie sur un mode).

### Autoradio
| Méthode | Effet |
|---|---|
| `setRadioText(title)` | Titre défilant (chaîne vide = « NO SIGNAL »). Défile seulement si le texte dépasse l'écran |
| `attachAnalyser(analyser)` | **Le plus simple** : donne ton `AnalyserNode` Web Audio, les barres suivent le beat seules |
| `setAudioLevels(levels, scale=1)` | Mode manuel : tableau de n'importe quelle longueur (`scale=255` pour `getByteFrequencyData`) |

```ts
const ctx = new AudioContext();
const analyser = ctx.createAnalyser();
analyser.fftSize = 256;
ctx.createMediaElementSource(audioEl).connect(analyser).connect(ctx.destination);
car.attachAnalyser(analyser);
car.setRadioText('Kavinsky - Nightcall');
```

### Caméra cockpit
`car.attachCameraToCockpit(camera)` : place la caméra au point de vue du conducteur, regardant devant. Pour regarder autour, tourne la caméra (rotation.y de départ = π).

## 5. Référence des nœuds (noms Blender = noms Three.js)

**Règle d'or : tout ce qui tourne se pilote par un nœud « pivot » vide, jamais par le maillage.**
La compression (`meshopt`) recentre chaque *maillage* et déplace le décalage dans la transformation de son nœud : faire tourner un maillage le ferait pivoter autour de son centre. Les nœuds vides ne sont pas modifiés, c'est pourquoi chaque pièce mobile a un parent vide placé exactement sur son axe. `CyberCar` le fait déjà ; si tu animes toi-même, utilise ces nœuds.

| Nœud à piloter | Pièce | Rotation |
|---|---|---|
| `steer_FL`, `steer_FR` | Pivots de braquage (parents des roues avant) | `rotation.y` |
| `spin_FL/FR/RL/RR` | Rouler (centre de l'essieu) | `rotation.x` |
| `steering_pivot` | Volant (dans `steering_column`, inclinée) | `rotation.y` |
| `pivot_gear_lever` | Levier 6 vitesses (pivot à la base) | `rotation.x` (avant/arrière), `rotation.z` (gauche/droite) |
| `pivot_pedal_clutch/brake/accel` | Pédales (pivot en haut), g. → d. | `rotation.x` négatif = appuyer |
| `wiperpivot_L`, `wiperpivot_R` | Essuie-glaces **néon** longs (68 cm, matériau `neon_secondary`), pivot à la base du pare-brise | tourner autour de **son axe local Y**, à composer avec l'orientation de repos : `q = qRepos * Qy(angle)` |

Autres nœuds :

| Nœud | Rôle |
|---|---|
| `body` | Carrosserie, **y compris les vrais feux** (voir §6) |
| `cockpit_static` | Tableau de bord (avec **néons tout autour** : lèvre basse, extrémités, bord haut), console, 2 sièges, grille de levier |
| `glass_rear` | Vitres arrière : 2 vitres latérales + hayon (matériau `glass`) |
| `neon_body` | Néons extérieurs : sous-caisse, **bas de caisse latéraux**, bande de portes, plancher, toit, **contour complet du pare-brise** |
| `screen_speed`, `screen_radio` | Écrans (canvas dessinés en JS) |
| `logo_front_badge`, `logo_front_ring`, `logo_front` | Emblème avant : badge **rond** sombre + anneau néon (sans cadre), logo chromé en relief |
| `logo_rear_plate`, `logo_rear` | Emblème arrière : logo **néon** en relief, matériau dédié `logo_rear_neon` (émissif 10) qui réagit à la musique (voir INTEGRATION.md §3) |
| `logo_wheel`, `logo_wheel_disc`, `logo_wheel_ring` | Logo du volant : logo cyan (`neon_primary`) sur disque sombre + anneau magenta (`neon_secondary`) |
| `interior_extra` | Porte-gobelets, ceintures de sécurité, console de pavillon, rétroviseur intérieur |
| `screen_mirror` | Écran du rétroviseur (canvas, ou `setRearViewTexture`) |
| `rain_windshield` | Copie du pare-brise (5 mm au-dessus) qui reçoit les gouttes de pluie (matériau `rain_glass`) |
| `gear_lever` | Levier refait : soufflet, tige, bague néon, pommeau (enfant de `pivot_gear_lever`) |
| `tuck_FL/FR/RL/RR` | **Groupes ajoutés au chargement** (absents du GLB) : ils contiennent chaque roue et la basculent à plat en mode volant. Ne pas animer à la main |
| `hover_fx` | Groupe ajouté au chargement : lueurs du mode volant (colonnes, ondulations, flaque de lumière) |
| `anchor_headlight_L/R` | Positions des projecteurs |
| `anchor_underglow`, `anchor_cabin_light`, `anchor_dash_light`, `anchor_front_logo_light`, `anchor_rear_logo_light` | Positions des `PointLight` néon |
| `cam_cockpit` | Œil du conducteur |

## 6. Matériaux et lumières

Les feux ne sont **pas** des objets ajoutés : ce sont des zones de la carrosserie, avec leur propre matériau.

| Matériau | Zone réelle | Piloté par |
|---|---|---|
| `light_head` | **Les 3 barres diagonales** de chaque côté de la calandre (phares) | `setHeadlights` |
| `signal_FL`, `signal_FR` | **Angles avant** (bouts de calandre) | `setBlinker` |
| `light_brake` | **Les 3 barres rouges** du panneau arrière (pleines) | `setBraking` / pédale de frein |
| `signal_RL`, `signal_RR` | **Fenêtres encastrées des angles** du pare-chocs arrière | `setBlinker` |
| `neon_primary`, `neon_secondary` | Néons (sous-caisse, roues, tableau de bord, logos…) | `setNeonColors` |
| `logo_chrome` | Logos avant/arrière (métal poli) | — (reflète l'environnement) |
| `body_paint` | Texture d'origine + **masque de peinture** (texture d'occlusion) ; remplacé à l'exécution par un `MeshPhysicalMaterial` | `setPaintColor`, `setPaintFinish` |
| `logo_rear_neon` | Logo arrière | `setRearLogo`, `setLogoLevel`, audio |
| `rain_glass` | Surface des gouttes | `setRain` |
| `glass` | Pare-brise, vitres de porte, vitres arrière | `setGlassOpacity` |
| `screen_speed`, `screen_radio` | Émissif = `CanvasTexture` | `setSpeed`, `setRpm`, `setRadioText`… |
| `int_dark`, `int_carbon` | Intérieur | — |

### Qui fait quoi : GLB ou Three.js ?
- **Dans le GLB** : la géométrie, les zones de feux, et une couleur/intensité *émissive* par matériau.
- **Dans Three.js** (ce n'est pas dans le fichier) : le **halo lumineux** (bloom), les **reflets** (carte d'environnement) et la **lumière projetée** (phares, sous-caisse). Fournis dans le code :
  - `CyberCar` ajoute lui-même 2 `SpotLight` (phares) et 5 `PointLight` néon sur la qualité `high`, posés sur les `anchor_*` : sous-caisse, habitacle, tableau de bord, logo avant, logo arrière. Elles suivent `setNeonColors` / `setNeonIntensity`.
  - `src/CyberCarFX.ts` : `createNeonFX()` (bloom qui ne s'applique qu'aux pixels très lumineux, donc pas à la peinture) et `makeNeonEnvironment()` (environnement synthwave pour les reflets sur la peinture, le chrome et le verre).

```ts
import { createNeonFX, makeNeonEnvironment } from './CyberCarFX';

scene.environment = makeNeonEnvironment(renderer, '#00e5ff', '#ff0d73');
const fx = createNeonFX(renderer, scene, camera, car.quality);
// boucle : car.update(dt); fx.render(dt);   // au lieu de renderer.render(...)
// resize : fx.setSize(w, h);   réglage : fx.setBloomStrength(0.9);
```
Sur `low`, `createNeonFX` ne fait aucun post-traitement (rendu direct).

## 7. Optimisation (petits appareils / réseau lent)

- `quality: 'auto'` → `low` si `saveData`, réseau ≤ 3G, ≤ 4 Go de RAM ou ≤ 4 cœurs ; sinon `high`.
- `low` : 1,0 Mo, textures 1k, ~40 k triangles, **pas de SpotLight ni de PointLight néon ni de bloom**, 16 barres audio, canvas à 15 Hz, sans halo (`shadowBlur`).
- Les écrans ne se redessinent que si une valeur change (compteur ≤ 15 Hz).
- Conseils côté appli : `renderer.setPixelRatio(Math.min(devicePixelRatio, 2))` (1,5 sur `low`), pas de bloom sur `low`, pas d'ombres portées sur la voiture.
- Fichiers à servir en **brotli/gzip** et en cache long (`immutable`) ; précharge `car-low.glb` d'abord si besoin, `car-high.glb` ensuite.

## 8. Limites connues (honnêtes)

1. **Couleur de la peinture** : le masque est généré automatiquement depuis les zones violettes de la texture (`tools/make_paint_mask.py`, ~14 % de la texture). Il est propre sur les grands panneaux, mais certaines pièces à textures peu saturées ou très usées peuvent rester non teintées, et le bord de quelques petites pièces peut montrer un liseré violet. Un masque peint à la main dans Blender serait plus fidèle.
2. **Vitres arrière** : le modèle d'origine n'avait pas de verre à l'arrière ; `glass_rear` est un ajout simple (plans). Un gros cylindre du modèle dépasse devant la vitre latérale arrière, c'est voulu par le modèle d'origine.
3. **Sièges et tableau de bord** : créés à la main, formes anguleuses. Les sièges s'inclinent maintenant vers l'arrière.
4. **Platine du logo arrière** (néon, avec son liseré) : elle recouvre les deux grilles de ventilation « XX » du modèle d'origine (elles ne sont pas supprimées ; supprime `logo_rear_plate` et `logo_rear` pour les retrouver).
5. **Clignotants / feux stop** : couleurs fixes (orange, rouge), non pilotées par `setNeonColors`.
6. **Sens de balayage des essuie-glaces** : vérifié visuellement (ils montent depuis le bas du pare-brise).
7. **Sens du levier** (non vérifié visuellement) : s'il s'incline du mauvais côté, inverse le signe de `rotation.z` dans `updateLever`.
8. **Clignotants arrière** : lentilles orange (`signal_plate_L/R`, matériaux `signal_RL/RR`) posées sur la face du pare-chocs, en plus de la fenêtre encastrée. Elles s'allument et clignotent avec `setBlinker`.
9. **Non testé** : barres audio de la radio avec un vrai son, GLB `low` sur un vrai mobile, performances réelles des effets.

## 9. Régénérer les GLB

1. Ouvrir `CyberCar_work.blend`, File ▸ Export ▸ glTF 2.0 (`.glb`, +Y up, matériaux exportés, pas d'animations).
2. `npx @gltf-transform/cli resize in.glb a.glb --width 2048 --height 2048`
3. `… webp a.glb b.glb --quality 90` puis `… meshopt b.glb car-high.glb --level high`
4. Variante `low` : `resize 1024`, `simplify --ratio 0.4 --error 0.002`, `webp --quality 80`, `meshopt`.
