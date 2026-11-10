# Monde « Hyper-Transit » et quartiers annexes : spécification de cadrage

> Document de conception (maths, tables, structures de données). Aucun code n'en découle encore :
> c'est la base de travail des futures fiches de tâche. Les flashs et stroboscopes décrits ici
> passent tous par le limiteur photosensible de `cf-show` (CLAUDE.md §1, règle 7).

## 0. Signaux audio communs

| Symbole | Définition | Plage |
|---|---|---|
| `B, M, H` | énergie des bandes 20–150 Hz, 150–2k Hz, 2k–16k Hz, normalisée par AGC | 0..1 |
| `env(x)` | `e += (x−e)·(1−exp(−Δt/τ))`, avec `τ=τ_a` si `x>e`, sinon `τ_r` | — |
| `k(t)` | impulsion kick `exp(−(t−t_k)/τ_k)`, `τ_k≈0.12 s` | 0..1 |
| `φ, φ_bar` | phase du temps et de la mesure ; `T_b = 60/BPM` | 0..1 |
| `F_x` | flux spectral positif de la bande x : `Σ max(0, S_n − S_{n−1})` | ≥0 |
| `z(x)` | z-score adaptatif `(x−μ)/σ`, μ et σ en EMA (τ≈8 s) | — |
| `E_f, E_s` | énergie globale rapide (τ=0.3 s) et lente (τ=4 s) | 0..1 |
| `h(id)` | hash stable par instance | 0..1 |

Déclencheur générique : bascule de Schmitt (`θ_on > θ_off`), quantification au prochain temps
(`φ→0`), puis cooldown en temps musicaux.

Onde de kick propagée, réutilisée partout (uniforme = 4 derniers horodatages `t_i`, zéro
historique CPU) :

`W(d,t) = Σ_i g_i · exp(−(t−t_i−d/c)/τ) · step(0, t−t_i−d/c)`, avec `d` la distance à la source
et `c` la célérité visuelle (80–200 m/s).

---

## 1. Quartier tunnel « HYPER-TRANSIT »

### 1.1 Intérieur

**Vitesse verrouillée au tempo** (les anneaux passent sur le temps) :

- `v* = n_r · L · BPM/60 · (1 + a_E·E_s)`. Exemple : `L=12 m`, `n_r=2`, 128 BPM donne 51,2 m/s.
- Lissage : `v̇ = (v*−v)/τ_v`, `τ_v≈1.5 s`.
- La distance est toujours intégrée (`s = ∫v dt`), jamais calculée par `t·v`, sinon un changement
  de BPM provoque un saut.
- Rattrapage de phase (PLL) : `v += k_p · wrap(L·n_r·(N_beat+φ) − s)`.

**Déformation spatiale** (vertex shader, `z` = distance à la caméra) :

- Courbure « curved-world » : `Δxy = κ(t) · z²`, avec `κ = κ0·(sin 2πφ_bar/4, cos 2πφ_bar/8)` et
  `κ0≈2e-4 m⁻¹`.
- Respiration : `R(θ,z,t) = R0 · [1 + a_K·W(z,t) + a_B·B·cos(mθ)·sin(k_z·z − ω_b·t)]`, avec
  `a_K≈0.08`, `m=n` côtés.
- Torsion : `θ' = θ + ω_tw·z + a_M·M·sin(k_z·z − 2πφ)`.
- Section polygonale morphable : `r_n(θ) = cos(π/n) / cos(mod(θ, 2π/n) − π/n)`, puis
  `r = mix(r_6, r_8, E_s)`.
- Coup de FOV : `fov = fov0 + Δ·k(t)`, `Δ ≤ 6°`.

**Néons directionnels** (chenillard le long de `z`) :

- `I(z) = pow(1 − fract((z − s·g)/Λ − φ), p) · (0.3 + 0.7·M)`, avec `Λ = 4L`, `p = 4..16`
  (netteté pilotée par H).
- `g = ±1` donne le sens, inversé à chaque changement de section.
- Couleur : `mix(cyan, magenta, fract(z/Λ_c + φ_bar))`.

**Stroboscope** (toujours en entrée du limiteur photosensible de `cf-show`) :

- `S(t) = step(fract(m·φ), duty)`, `duty = 0.15..0.5`, fréquence `f = m·BPM/60`.
- Plafond plein champ : `m ≤ floor(180/BPM)`, soit ≤ 3 flashs/s. À 128 BPM, `m=1` (2,13 Hz).
- Au-delà, le strobe devient spatial : anneaux alternés `step(.5, fract(idx/2))`, moins de 25 % de
  l'image, ou contraste `ΔL < 0.1`.

### 1.2 Extérieur

| Élément | Équation clé |
|---|---|
| Structures suspendues (câbles) | caténaire `y = a·cosh((x−x0)/a) + y0` ; balancement `Δ⊥ = A_s·sin(πu)·sin(ω_s·t + 2π·h)·(0.2+B)` |
| Pylônes HT (arcs) | `p(u) = mix(P0,P1,u) + n⊥·A_arc·sin(πu)·N2(u·f_a, floor(t·r_a))`, `r_a = 12 + 40·H` ; visible si `z(F_H) > 1.5` |
| Générateur plasma (tore) | rayon mineur `ρ(θ,ψ,t) = ρ0·[1 + a·B·sin(nθ − Ω·t) + b·W(0,t)]`, `Ω = 2π·BPM/60 · q`, `q ∈ {¼, ½, 1}` |
| Plasma (émissif) | FM : `g = (1 + m_B·B)·sin(2π·f_c·t + β·M·sin(2π·f_m·t))` ; texture `fbm(p + 2·fbm(p + t·u))` seuillée par `g` |
| Charge du générateur | `Q̇ = γ·B − Q/τ_Q` (`τ_Q≈6 s`) ; émissif `∝ Q²` ; décharge si `Q ≥ 1` |
| Horizon réactif (skyline-spectre) | `hauteur(x) = h0 + h1·S(f_x)`, `f_x = f_min·(f_max/f_min)^x` (échelle log), lissage `τ_r = 0.25 s` |
| Soleil à bandes | `mask = step(fract(y·n − t·v_s), 0.35 + 0.5·y)` ; rayon `R_s·(1 + 0.04·k)` |
| Grille de sol | `g = 1 − smoothstep(0, w, abs(fract(x) − .5))`, `w = fwidth(x)·1.5` ; défilement `−s/L_g` |

### 1.3 Cinq événements de quartier

| ID | Événement | Déclenchement | Enveloppe et effet | Cooldown |
|---|---|---|---|---|
| EV1 | **Cascade d'anneaux** | `z(F_B) > 2 ∧ B > 0.6` (attaque de kick) | `W(z,t)` sur l'émissif des anneaux, `c = 150 m/s`, `τ = 0.18 s` | ½ temps |
| EV2 | **Hyperdrive** | armement : `d(centroïde)/dt > 0` pendant ≥ 4 mesures `∧ B < 0.3` ; tir : `E_f − E_s > 0.35 ∧ B > 0.7` sur `φ_bar→0` | `n_r ×2`, `κ0 ×3`, étirement `z' = z·(1+2·e)`, `e = exp(−t/2T_bar)` | 16 mesures |
| EV3 | **Orage d'arcs** | `z(F_H) > 2.5` pendant ≥ 3 temps sur 4 | `N_arcs = ceil(6·H)` ; durée d'un arc `T_b/4` ; pylône choisi par `h(id, N_beat)` | 4 mesures |
| EV4 | **Apnée** | `E_s < 0.25` pendant ≥ 2 mesures (break) | extinction en rampe sur 1 mesure (`ΔL/s` borné) ; restent le plasma et l'horizon à 20 % | jusqu'à `E_s > 0.4` |
| EV5 | **Surcharge plasma** | `Q ≥ 1`, quantifié sur `φ_bar→0` | décharge : anneau de choc `r = c·t`, largeur `σ = 8 m`, `I = exp(−((d−ct)/σ)²)·exp(−t/τ)` ; `Q ← 0` | 8 mesures |

```json
{"id":"EV2","arm":{"all":[{"sig":"centroid_slope","op":">","v":0,"hold_bars":4},{"sig":"B","op":"<","v":0.3}]},
 "fire":{"all":[{"sig":"E_f-E_s","op":">","v":0.35},{"sig":"B","op":">","v":0.7}],"quantize":"bar"},
 "env":{"shape":"exp","tau_bars":2},"params":{"n_r_mul":2,"kappa_mul":3,"stretch":2},
 "cooldown_bars":16,"flash_limited":true}
```

---

## 2. Trois quartiers annexes et 30 archétypes

- **A — Monolithes** : mégastructures brutalistes, 200–600 m, béton, peu d'émissif. Réponse lente
  et massive.
- **B — Ruche** : slums techwear, 10–60 m, empilements, bâches, câbles. Réponse nerveuse et
  fragmentée.
- **C — Fonderie** : cœur industriel, tuyaux, cuves, torchères. Réponse pulsée et mécanique.

| ID | Nom | Q. | Forme géométrique | Sockets (néons / antennes) | Impact audio |
|---|---|---|---|---|---|
| A01 | Stèle Zéro | A | prisme droit 1:1:8, chanfrein 45° | 4 arêtes néon verticales, 1 mât sommital | Bass |
| A02 | Ziggourat Kessler | A | 7 terrasses décroissantes (ratio 0,82) | liserés par terrasse (7), 2 antennes | Bass |
| A03 | Arche Tectonique | A | portique en Π, portée 180 m | sous-face linéaire, 2 pieds | Bass |
| A04 | Peigne | A | 6 lames parallèles sur socle | tranche de chaque lame (6) | Mid |
| A05 | Bloc Porte-à-Faux | A | deux boîtes décalées de 40 % | sous-face du porte-à-faux, 1 enseigne XL | Bass |
| A06 | Silo des Archives | A | cylindre cannelé (24 cannelures) | 24 rainures verticales | Mid |
| A07 | Brise-Soleil | A | dalle à grille d'alvéoles 12×40 | 480 alvéoles (matrice EQ) | Mid |
| A08 | Tour Vrillée | A | prisme carré torsadé de 90° sur H | 4 arêtes hélicoïdales, couronne | Mid |
| A09 | Pyramide Inverse | A | tronc de pyramide retourné | anneau de base, 4 arêtes | Bass |
| A10 | Mur-Barrage | A | paroi courbe (arc 60°), contreforts | 9 contreforts, crête linéaire | Bass |
| B01 | Pile de Conteneurs | B | 5–9 boîtes empilées, rotation ±8° | 1 enseigne par boîte, 2 paraboles | Mid |
| B02 | Capsule-Hôtel | B | grille 6×10 de capsules arrondies | 60 hublots, 1 totem | High |
| B03 | Échoppe Bâchée | B | boîte avec auvent en plan incliné | bordure d'auvent, 1 lanterne | High |
| B04 | Tour Parasite | B | fût mince avec 12 greffons latéraux | 12 greffons, 3 antennes fouet | High |
| B05 | Passerelle Marché | B | pont-caisson entre 2 fûts | garde-corps (2), 6 enseignes drapeau | Mid |
| B06 | Nid de Câbles | B | mât avec 16 caténaires rayonnantes | 16 guirlandes, 1 transfo | High |
| B07 | Bloc Clim | B | cube avec 30 unités extrudées | 30 ventilateurs (disques) | Mid |
| B08 | Escalier-Rue | B | hélice de dalles autour d'un noyau | nez de marche (N), 1 lampadaire | Mid |
| B09 | Citerne Squat | B | sphère sur trépied, cabanes en couronne | équateur, 5 cabanes | Bass |
| B10 | Façade Patchwork | B | plan subdivisé (BSP, 20–40 tôles) | 1 tôle sur 4 émissive, 2 paraboles | High |
| C01 | Tour de Refroidissement | C | hyperboloïde à une nappe | lèvre haute, 8 méridiens | Bass |
| C02 | Torchère | C | treillis élancé avec flamme (billboard) | flamme, 3 balises | High |
| C03 | Colonne de Distillation | C | cylindre avec 9 plateformes annulaires | 9 anneaux, échelle lumineuse | Mid |
| C04 | Cuves Jumelles | C | 2 sphères avec passerelle | équateurs (2), passerelle | Bass |
| C05 | Halle Fonderie | C | nef à sheds (toit en dents de scie ×8) | 8 verrières, porte-four | Bass |
| C06 | Pipe-Rack | C | portiques répétés avec 12 tubes | 12 tubes (flux animé) | Mid |
| C07 | Réacteur Tokamak | C | tore avec 16 bobines en D | 16 bobines, cœur plasma | Bass |
| C08 | Portique-Grue | C | portique en A avec chariot mobile | chariot, rail, 2 gyrophares | Mid |
| C09 | Transfo-Ruche | C | grille 4×4 d'isolateurs coniques | 16 pointes (arcs) | High |
| C10 | Cheminée Triple | C | 3 cylindres tronconiques liés | 3 couronnes, 9 balises | Mid |

### Maths de structure

**(1) Déplacement de vertex par bruit 3D** (sur A06, A10, C01) :

`p' = p + n · A · w(y) · B_env · N3(p·f + (0, −s_n·t, 0))`

- `w(y) = smoothstep(0, 0.15H, y)` garde la base ancrée.
- `A ≤ 0.02·H`, `f ≈ 1/40 m⁻¹`.
- Normale corrigée : `n' = normalize(n − A·(∇N − (∇N·n)·n))`.
- L'`Aabb` est élargi de `A_max` à la création. Aucun `NoFrustumCulling` n'est nécessaire.

**(2) Échelle par étage avec onde de kick** (empilements A02, B01, B02, C03) :

`S_xz(j) = 1 + a·W(j·h_ét, t)·(0.5 + 0.5·h(id))`, puis `y_j' = y_j + b·W(j·h_ét, t)`

- L'onde monte étage par étage à la célérité `c`.
- `a ≈ 0.06`, `b ≈ 0.3 m`.
- Tout passe par les données d'instance, sans allocation par image.

**(3) Morphing superquadrique et torsion** (A01, A08, A09) :

- Surface : `abs(x/a)^p + abs(y/b)^p + abs(z/c)^p = 1`, avec `p = mix(8, 2.5, env(M))` (du cube
  vers l'arrondi).
- Torsion : `(x,z)' = Rot(α0·(y/H)·(0.5 + M)) · (x,z)`.

**(4) Façade égaliseur** (A07, B02, B10) :

`lit(i,j) = step(j/N_ét, S(f_i))`, avec `f_i = f_min·(f_max/f_min)^(i/N_col)`. L'émissif retombe
par `τ_r = 0.2 s`.

```json
{"id":"A07","district":"A","shape":{"type":"slab","size":[90,320,24],"cells":[12,40]},
 "sockets":[{"kind":"cell_matrix","count":480,"layer":"emissive"}],
 "audio":{"band":"mid","fn":"eq_facade","tau_r":0.2},"deform":["kick_wave_floor"],"aabb_pad":0.5}
```

---

## 3. Catalogue de 30 pubs, enseignes et graffitis

### 3.1 Billboards holographiques 3D

| ID | Marque fictive | Concept visuel | Boucle (durée) | Réaction audio |
|---|---|---|---|---|
| H01 | **NEUROVA** (implants) | cerveau filaire, synapses en particules | rotation Y 360° (8 mesures) | synapses allumées par `F_H` |
| H02 | **KIRIN-9** (boisson) | canette avec anneaux de liquide en orbite | orbite, éclaboussure sur le 1 (4 mesures) | rayon des anneaux `∝ B` |
| H03 | **AXIOM Motors** | moto en nuage de points, traînées | balayage scanline bas→haut (2 mesures) | longueur des traînées `∝ v/v*` |
| H04 | **LUXE//VOID** (mode) | mannequin en tranches voxel | tranches décalées en X, recomposition (4 mesures) | décalage `= A·k(t)` |
| H05 | **ORBITAL Bank** | pièce-planète avec anneaux de chiffres | précession de l'axe (16 mesures) | vitesse des chiffres `∝ M` |
| H06 | **SYNTHÉA** (pharma) | double hélice avec gélules | montée en vis sans fin (8 mesures) | pas d'hélice `∝ 1 + 0.3·B` |
| H07 | **HOSHI Air** | drone-origami qui se plie | 4 poses clés interpolées (4 mesures) | changement de pose sur le temps |
| H08 | **MIRAGE VR** | visage masqué, fenêtre de parallaxe | faux volume par parallaxe (8 mesures) | profondeur `∝ E_s` |
| H09 | **TITAN Cement** | cube en vue éclatée (27 blocs) | éclatement et rassemblement (2 mesures) | écartement `= W(0,t)` |
| H10 | **ZÉNITH Énergie** | soleil filaire avec protubérances | pulsation radiale (1 mesure) | protubérances `∝ z(F_B)` |

Shader hologramme commun :
`α = fresnel^2 · (0.6 + 0.4·step(.5, fract(y·120 − t·3))) · (1 − glitch)`, avec
`glitch = step(0.97 − 0.1·H, h(floor(y·40), floor(t·24)))`.

### 3.2 Enseignes néo-synthwave

Motif : 16 pas par mesure (`1` = allumé), lu par `step_idx = floor(16·φ_bar)`. Ces motifs sont des
zones locales (bien moins de 25 % de l'image), mais passent quand même par le limiteur.

| ID | Texte (sens) | Logo vectoriel | Couleurs | Motif 16 pas | Note |
|---|---|---|---|---|---|
| N01 | 電脳 (cyber-cerveau) | circuit en spirale carrée | cyan / blanc | `1111111111111101` | micro-coupure de tube fatigué |
| N02 | 拉麺 (ramen) | bol avec 3 volutes | orange / rouge | `1000100010001000` | volutes en chenillard |
| N03 | 夜行 (voyage de nuit) | flèche double chevron | magenta | `1100110011001100` | chevrons décalés de 2 pas |
| N04 | 薬 (pharmacie) | croix dans un hexagone | vert menthe | `1111000011110000` | respiration `sin²(πφ_bar)` |
| N05 | 質屋 (prêteur sur gages) | 3 losanges | jaune ambre | `1010101011111111` | — |
| N06 | 闇市 (marché noir) | œil barré | violet | `1001001000100100` | motif 3-3-2 |
| N07 | 改造 (modification) | clé et éclair | cyan / magenta | `1111111100000000` | alternance bicolore par demi-mesure |
| N08 | 無限 (infini) | lemniscate tracée | rose | toujours allumée | point traceur `(cos τ, sin τ·cos τ)/(1+sin²τ)`, `τ = 2πφ_bar` |
| N09 | 酒場 (bar) | verre incliné | bleu / rose | `1110111011101110` | swing : pas impairs retardés de `T_b/12` |
| N10 | 眠らない街 (la ville sans sommeil) | skyline en 5 traits | blanc / cyan | allumage lettre par lettre | 1 caractère par temps, extinction sur le 1 |

Scintillement de tube, commun : `I = motif · (0.92 + 0.08·N1(t·50 + h)) · (1 + 0.25·k(t))`.

### 3.3 Graffitis procéduraux (shaders UV)

`uv ∈ [0,1]²`, `c = uv − .5`, `r = length(c)`, `θ = atan2(c.y, c.x)`.

| ID | Nom | Formule du motif UV | Réactivité audio |
|---|---|---|---|
| G01 | Truchet néon | `g = fract(uv·N)` retourné si `h(floor(uv·N)) > .5` ; `d = min(abs(length(g) − .5), abs(length(g − 1) − .5))` ; `I = smoothstep(w, 0, d)` | `w = 0.03 + 0.05·B` ; nouveau tirage de `h` toutes les 4 mesures |
| G02 | Laser-tag Lissajous | `P(τ) = 0.4·(sin(a·τ + δ), sin(b·τ))` ; `I = Σ_τ exp(−dist(c, P)²/σ²)` | `a:b ∈ {3:2, 5:4, 3:4}` par section ; `δ = 2πφ_bar` ; `σ ∝ H` |
| G03 | Fissures Voronoï | `I = smoothstep(w, 0, F2 − F1)` sur `uv·N` | sites déplacés par `0.3·sin(2πφ + 6.28·h)` ; `w ∝ k(t)` |
| G04 | Rosace polaire | `I = smoothstep(w, 0, abs(r − 0.4·abs(cos(kθ/2))))` | `k = 3 + floor(5·M)` ; rotation `θ += 2π·φ_bar/4` |
| G05 | Moiré bi-centre | `I = .5 + .5·sin(f·length(c − c1))·sin(f·length(c + c1))` | `c1 = 0.1·B·(cos t, sin t)` ; `f = 60 + 80·H` |
| G06 | Glitch en bandes | `uv.x += (h(floor(uv.y·N), floor(t·r)) − .5)·A`, puis échantillonnage du tag | `A = 0.2·z(F_H)⁺` ; `r = BPM/60·4` |
| G07 | Pochoir à coulures | `mask = smoothstep(θ − s, θ + s, sdf(uv) + 0.08·fbm(uv·(8, 1.5) + (0, −L)))` | longueur de coulure `L = ∫B dt mod 1` ; `s` (netteté) `∝ 1 − M` |
| G08 | Circuit hexagonal | coordonnées hexagonales `q = (uv.x·2/√3, uv.y − uv.x/√3)·N` ; arêtes `d = abs(max(abs(g.x), abs(g.y), abs(g.x+g.y)) − .5)` ; pistes actives si `h(cellule) < ρ` | `ρ = 0.2 + 0.6·E_s` ; impulsion le long des pistes `fract(dist − 2φ)` |
| G09 | Spirale logarithmique | `I = step(.5, fract(a·ln r + n·θ/2π − φ_bar))` | `n` (bras) `= 2 + floor(4·B)` ; `a ∝ 1 + M` |
| G10 | Tag oscilloscope | `I = smoothstep(w, 0, abs(c.y − 0.3·wave(uv.x)))`, avec `wave` = texture 1D de la forme d'onde | natif ; rémanence `I_acc = max(I, I_acc·exp(−Δt/0.15))` |

Vieillissement commun (peinture sur béton) :
`I_final = I · (0.6 + 0.4·fbm(uv·30)) · mix(1, 0.3, usure(h))`.

Émissif des tags laser (G02, G10) : il est borné en luminance avant le bloom, car ce sont des
sources de fort contraste.
