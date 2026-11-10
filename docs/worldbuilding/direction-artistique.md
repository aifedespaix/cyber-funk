# Direction artistique et qualité de rendu : analyse critique et cible (phase 3)

> Troisième document de cadrage, après `hyper-transit.md` (phase 1) et `hyper-transit-phase2.md` (phase 2), dont
> il reprend les notations (`B, M, H`, `SB, PB, LM, HM, TR`, `env`, `k(t)`, `φ`, `T_b`, `E_f`, `E_s`, `h(id)`, `W`).
> Le plan de développement qui fusionne les trois documents est `docs/worldbuilding/CHANTIERS.md`.
>
> **Base de l'analyse.** Lecture du code de rendu (`cf-render/postfx`, `palette`, `layers`, `cf-world-neon/road`,
> `facade`, `weather`, `cf-cinema`) et des captures de `docs/captures/` (rendu logiciel, 960×540). L'application n'a
> pas été lancée sur le matériel cible : les constats sur l'image sont à confirmer sur le projecteur.
> Les constantes sont des valeurs de départ. **[L]** = passe par le limiteur photosensible de `cf-show`.

---

## 0. Constat sur l'existant

| # | Constat | Cause dans le code | Remède | Chantier |
|---|---|---|---|---|
| 1 | Toutes les fenêtres ont la même intensité et quatre teintes à la fois (cyan, orange, blanc, violet) : pas de dominante, pas de point focal | `facade.wesl` tire teinte et allumage par fenêtre sans hiérarchie d'intensité | paliers d'émissif E1–E4, règle 60/30/10, fraction allumée pilotée par la densité `Δ` (§1.1, §4.2) | C4 |
| 2 | Les tours lointaines ont le même contraste que les proches : la ville se lit comme un décor plat | pas de perspective atmosphérique ; la brume est faite de plans (`mist`, `haze`), pas d'un milieu | brouillard analytique par fragment, commun à tous les shaders du monde (§3.2) | C3 |
| 3 | La route occupe un tiers de l'image en noir presque uni | reflets analytiques présents (`lamp_reflection`, `facade_reflection`) mais faibles hors pluie, aucun micro-détail | flaques, étirement des reflets, grain d'asphalte, flaque de lumière sous le sujet (§2.2) | C4 |
| 4 | Volumes sans relief : faces unies, pas de contre-jour, pas d'occlusion | tout le monde est émissif non éclairé, un seul pavé unité par pièce | ombrage par face, dégradé vertical d'occlusion, liseré de contre-jour (§2.1) | C4 |
| 5 | Le halo des néons est court et de même teinte que le cœur | bloom calé sur Sillage (seuil 0,85, `Bloom::NATURAL`), tube sans profil cœur/halo | cœur blanc et halo saturé dans le shader, queue du bloom allongée par la brume (§3.1) | C2 |
| 6 | Grain uniforme, identique dans les noirs et les blancs, motif `sin` | `grade.wgsl` : `fract(sin(dot(...)))` additif | grain pondéré par la luminance, hachage entier, cadence 24 Hz (§3.1) | C2 |
| 7 | Aberration chromatique sur toute l'image | `grade.wgsl` : `shift = c·aberration·(0.4 + 2·r²)` | aberration nulle au centre, bornée aux hautes lumières (§3.1) | C2 |
| 8 | Aucune persistance : un néon qui file ne laisse pas de trace | pas de tampon d'historique | rétroaction masquée par la luminance (§3.1) | C2 |
| 9 | Une seule dynamique de bande (`bass`, `mid`, `high`) et un pouls : la ville pulse en bloc | `AudioFeatures` à 3 bandes | sous-bandes et matrice de routage (phase 2 §2) | C1 |
| 10 | Le contraste temporel dépend seulement des portes du look | pas de grandeur unique de densité visuelle | densité `Δ` dérivée de `intensity` et `Section` (§4.2) | C1 |

Ce qui est déjà solide et qu'il faut garder : le compositeur à quatre couches en HDR, la palette à onze rôles avec
fondu de thème, le limiteur, le réalisateur calé sur les temps, les reflets analytiques de la route (lampes et
grille de fenêtres), les leviers de qualité.

---

## 1. Direction artistique et identité visuelle

### 1.1 Éthos et palette

**Éthos : « nuit humide, lumière rare ».** Le noir est la matière première. La lumière est chère, donc lisible :
une image calme contient peu de sources, toutes de la même famille de teinte ; le climax se mesure à l'écart avec
ce calme, pas à sa valeur absolue.

**Règle 60 / 30 / 10 / 2**, en surface d'image, sur les onze rôles de `ScenePalette` :

| Part | Rôles | Luminance relative `Y` (affichée) | Usage |
|---|---|---|---|
| 60 % | `ink`, `velvet` | `< 0.05` | fonds, béton, route, ciel |
| 30 % | dominante : `mint`, `azure` (et `violet` en transition) | `0.05–0.5` | fenêtres, liserés, brume éclairée |
| 10 % | accent : `rose`, `amber` | `0.3–0.8` | sujet, enseignes clés, feux, événements |
| ≤ 2 % | `ice` | `> 0.8` | cœur des tubes, lasers, pic de drop |
| événement | `alarm`, `lime`, `sun` | — | jamais en décor : police, alerte, soleil |

- `Y = 0.2126·R + 0.7152·G + 0.0722·B`. Le bleu pur pèse 7 % : une forme qui doit se lire de loin est cyan
  (`mint`), jamais bleu ou violet saturé.
- Harmonie : accent à `150°–180°` de la dominante sur le cercle des teintes. Une troisième famille n'apparaît que
  portée par un événement, et moins de 4 mesures.
- **Critère mesurable** (test sur capture) : au moins 70 % des pixels émissifs (`Y > 0.2`) ont une teinte à moins de
  25° de la dominante ou de l'accent ; `ice` couvre moins de 2 % de l'image hors drop.

**Paliers d'émissif** (valeurs de scène, avant bloom ; seuil du bloom `θ_b`) :

| Palier | Valeur HDR | Rayonne ? | Qui |
|---|---|---|---|
| E0 | 0 | non | béton, tôle, asphalte |
| E1 | `0.2–0.7·θ_b` | non | fenêtres, marquages, graffitis au repos |
| E2 | `1.5–3·θ_b` | halo court | enseignes, liserés, feux |
| E3 | `5–10·θ_b` | halo long, cœur blanc | tubes au premier plan, lasers, plasma |
| E4 | `> 10·θ_b` **[L]** | sature | frappe de drop, décharge (EV5), moins d'une mesure |

Règle : une surface ne monte que d'un palier sous l'effet de la musique. Le passage E1 → E3 d'une fenêtre est
interdit, c'est lui qui écrase la hiérarchie.

**Fenêtres** (remède du constat 1), par immeuble de graine `h` :

- fraction allumée `p = 0.18 + 0.42·Δ` (densité du §4.2) ;
- intensité log-normale `I = E1·exp(0.6·n)`, `n` gaussien tiré du hachage, bornée à `0.9·θ_b` ;
- teinte : 80 % dominante, 15 % accent chaud, 5 % `ice` ; un immeuble sur six est « monochrome accent » ;
- groupes : les fenêtres s'allument par étages ou colonnes entières (`h(étage) < p`), pas en confettis.

**Lisibilité à grande échelle** (projection, mur de LED, salle à 20 m) :

| Règle | Valeur |
|---|---|
| Trois valeurs lisibles en plissant les yeux | noir, dominante moyenne, accent clair |
| Épaisseur minimale d'un trait | 3 px à 1080p (`w ≥ 1.5·fwidth`) ; aucun trait de 1 px (moiré sur LED) |
| Fréquence spatiale maximale d'un motif | 1 cycle pour 6 px ; au-delà, fondu vers la moyenne par `fwidth` |
| Plages de gris moyen | à éviter : le noir relevé d'un projecteur les délave. Soit `Y < 0.05`, soit `Y > 0.2` |
| Texte et logos | hauteur ≥ 4 % de l'image, contraste ≥ 7:1 sur leur fond |
| Zone sûre | sujet et texte dans les 80 % centraux |

### 1.2 Hiérarchie des plans

Cinq plans, du fond vers l'œil. Les trois premiers vivent dans les couches `Background` et `World`, le dernier
dans `Foreground`.

| Plan | Contenu | Distance | Luminance | Saturation | Détail | Vitesse apparente | Brume `1 − T` |
|---|---|---|---|---|---|---|---|
| P0 Ciel | dôme, astres, aurores | ∞ | `0.02–0.15` | 0.5 | très bas | quasi nulle | — |
| P1 Horizon | silhouettes, skyline-spectre | > 600 m | silhouette **plus sombre** que la brume derrière | 0.4 | contour seul | lente | 0.7–0.95 |
| P2 Ville médiane | tours, enseignes, câbles | 60–600 m | `0.05–0.5` | 0.8 | moyen | moyenne | 0.2–0.7 |
| P3 Scène | route, sujet, trafic, façades proches | 0–60 m | pleine plage | 1.0 | fin | rapide | 0–0.2 |
| P4 Avant-plan | pluie, poussières, gouttes d'objectif, amorces floues | < 3 m | `0.1–0.4`, translucide | 0.6 | flou | très rapide | 0 |

Règles de profondeur :

1. **Séparation de valeur** : deux plans voisins diffèrent d'au moins `0.12` en luminance moyenne affichée.
2. **Contre-jour par la brume** : la brume lointaine est plus claire que les tours qui la précèdent. La silhouette
   découpe la ville, pas l'éclairage des façades.
3. **Perspective atmosphérique** par fragment : `C' = C·T + C_brume·(1 − T)`, saturation `×(0.4 + 0.6·T)`,
   contraste local `×T` (formules complètes au §3.2).
4. **Amorce** : un élément de P4 ou de P3 proche coupe le bord du cadre dans au moins un plan sur trois (pylône,
   câble, enseigne floue). C'est le repère d'échelle le moins cher.
5. **Parallaxe** : la vitesse apparente croît d'un facteur ≥ 2 d'un plan au suivant ; un plan sans mouvement
   propre reçoit une dérive (brume, poussières).
6. **Le sujet a le contraste local le plus fort de l'image.** Une flaque de lumière le suit :
   `L_sujet = c_accent·exp(−r²/σ²)`, `σ = 6 m`. Rien d'autre n'atteint E3 dans un rayon de 15 % de l'image autour
   de lui.
7. **Lignes de fuite** : bords de route, câbles et liserés convergent vers le point de fuite. Aucun élément
   lumineux de P2 n'est horizontal et plein cadre.

---

## 2. Rendu, éclairage et matériaux

### 2.1 Stratégie d'éclairage

Le monde est émissif et non éclairé (seule la voiture a de vraies lumières). L'éclairage est donc **écrit dans
les shaders**, à partir d'un petit bloc d'uniformes commun `SceneLight` :

```
SceneLight { dir_key: vec3, c_key: rgb, c_fill: rgb, dir_rim: vec3, c_rim: rgb, fog: (ρ0, H_f, g), c_fog: rgb }
```

| Rôle | Source motivée | Formule dans le shader | Rapport |
|---|---|---|---|
| Lumière principale | le côté de rue dominant (un côté est plus lumineux : asymétrie `1 : 0.4`, tirée par quartier) | `k = (n·dir_key)⁺` ; ombrage par face `s = 0.45 + 0.55·k` | principale : remplissage = `8:1` au calme, `16:1` au sommet |
| Remplissage | la brume éclairée (`velvet` teinté de dominante) | `c_fill·(0.5 + 0.5·n.y)` (hémisphère) | — |
| Contre-jour | la brume lointaine et le ciel, dans l'axe de la route | `rim = (1 − n·v)^4·(dir_rim·(−v))⁺·c_rim` | visible sur les arêtes de toit et le sujet |
| Volumétrique | faisceaux, tubes proches, phares | diffusion de la phase 2 §1.3 | — |

Relief sans calcul d'ombre :

- **Occlusion verticale** des façades : `ao = mix(0.35, 1, smoothstep(0, 0.25·H, y))`. Le pied des tours plonge
  dans le noir et se fond dans la brume.
- **Ombre de contact** sous les véhicules : `1 − 0.7·exp(−r²/σ²)`, `σ` = demi-largeur du véhicule.
- **Arêtes** : chanfrein lumineux de toit, `E2`, largeur `1.5·fwidth`, sur 1 immeuble sur 3.
- **Halo de néon sur la façade porteuse** : phase 2 §1.2 (`L_halo`), sans lumière dynamique.

### 2.2 Matériaux et surfaces

Les shaders du monde ne sont pas le PBR de Bevy. « Métal » et « rugosité » désignent ici les paramètres du lobe
de reflet analytique (largeur, teinte du reflet).

| Surface | Albédo `Y` | Métal | Rugosité `α` | Micro-détail |
|---|---|---|---|---|
| Asphalte sec | 0.04 | 0 | 0.85 | grain `fbm(p·8)`, joints, rapiéçages (`h(cellule 4 m)`) |
| Asphalte mouillé | 0.015 | 0 | 0.04–0.25 | flaques, ondes (phase 2 §1.1) |
| Béton brut | 0.12–0.25 | 0 | 0.9 | coulures verticales `fbm(x·20, y·1.5)`, joints de banche tous les 3 m |
| Verre de façade | 0.02 | 0 | 0.05 | reflet du ciel et de la rue d'en face, fresnel `F0 = 0.04` |
| Tôle peinte | 0.08 | 0 | 0.5 | rayures, usure des bords (distance au bord de la face) |
| Métal nu (pylônes, câbles) | 0.5 | 1 | 0.35 | reflet teinté par l'albédo, liseré de contre-jour marqué |
| Chrome (véhicules) | 0.9 | 1 | 0.08 | existant (`car_paint`, `car_glass`) |
| Tube néon | — | — | — | profil cœur/halo ci-dessous |

**Micro-détail procédural**, commun (fonction WESL partagée) : `α' = α + 0.15·(fbm(p·8) − 0.5)`,
`albédo' = albédo·(0.8 + 0.4·fbm(p·3))`. Il s'efface avec la distance par `fwidth` : aucun scintillement au loin.

**Tube néon**, avec `d` la distance à l'axe et `r` le rayon :

```
cœur  = smoothstep(r, 0.5·r, d)
C     = I·( mix(c, blanc, cœur²) · cœur  +  c · exp(−d/ℓ) · 0.35 )        ℓ = 4·r
```

Le cœur vire au blanc, le halo reste saturé. C'est ce qui donne un néon « propre » sans brûler l'image : le blanc
ne couvre que le cœur.

**Reflets.** La route garde et enrichit ses reflets analytiques (`lamp_reflection`, `facade_reflection`) :
flaques, rugosité réactive, étirement vertical et aberration du reflet viennent de la phase 2 §1.1. Le SSR complet
de la phase 2 est **reporté** : les couches sont rendues dans des cibles séparées et le monde n'est pas éclairé,
le gain ne justifie pas la passe. Il reste une option du palier haut, à réévaluer après C4.

**Fausse profondeur des fenêtres** : voir l'idée de rupture n° 1 (§5).

---

## 3. Post-traitement et atmosphère

### 3.1 Chaîne et calibrage

Ordre des passes (les ajouts sont en gras) :

```
couches HDR composées → **persistance** → bloom → étalonnage [ aberration ciblée · **halation** · gouttes · vignette ]
                      → tonemapping → **grain** → dither → surcouche 2D
```

Le flash du spectacle **[L]** s'ajoute après la persistance, pour qu'elle ne prolonge jamais un flash.

**Contrat d'exposition.** Blanc de scène = 1,0. Moyenne d'image visée : `Ȳ = 0.06` au calme, `0.12` au sommet,
`≤ 0.25` pendant une frappe. Le bloom apporte au plus 25 % de `Ȳ`. Ces trois nombres sont testés sur une capture
de référence.

**Bloom** (on garde le `Bloom` de Bevy, on change ses réglages) :

| Paramètre | Actuel | Cible | Raison |
|---|---|---|---|
| Seuil | 0.85 | 1.0 | E1 ne rayonne plus du tout |
| Genou | 0.2 | 0.5 | pas de scintillement des arêtes fines |
| Queue | `NATURAL` | pondération des basses fréquences relevée avec la brume : `q = 0.5 + 0.3·ρ_brume` (phase 2 §1.2) | halo long par temps humide, court par temps sec |
| Pulsation | `0.3 + 0.2·pouls` | inchangée **[L]** | — |

Les réglages actuels sont calés sur les références de Sillage (`tools/sillage-golden/postfx/`). S'en écarter est
une décision de direction artistique : les tests et références concernés sont mis à jour dans le même commit, et
le commit le dit.

**Tonemapping sans brûler les blancs.** Principe : appliquer la courbe à la luminance, puis désaturer vers le
blanc seulement très haut.

```
Y  = luma(c) ; t = T(Y)                      // T : courbe du tonemapper
c' = mix( c·(t/Y), vec3(t), smoothstep(2, 16, Y)^1.5 )
```

| Candidat | Teintes saturées | Hautes lumières | Décision |
|---|---|---|---|
| `KhronosPbrNeutral` (en place) | fidèles | désaturation tardive | **garder par défaut** |
| `AgX` | légèrement ternies | très douces | essai comparatif en C2, capture côte à côte |
| `TonyMcMapface` | douces | douces | essai si la table de correspondance est préchargée (règle 8) |

**Aberration chromatique ciblée** (remplace le décalage plein cadre) :

`A(r, Y) = A0·smoothstep(0.35, 1, r)²·(0.3 + 0.7·smoothstep(0.5, 2, Y))·(1 + 3·env(TR; 2 ms, 90 ms))`

Nulle au centre (le sujet reste net), forte dans les coins et sur les hautes lumières. `A ≤ 0.01`.

**Halation** (frange chaude du film autour des hautes lumières) :
`C += flou(max(C − 1.5, 0), σ = 0.4 % de la largeur)·(1, 0.35, 0.15)·0.25`. Elle réutilise le deuxième niveau de
la pyramide du bloom.

**Grain** (après tonemapping, en espace affiché) :

```
n  = hash_entier(px, floor(t·24))            // lowbias32, pas de sin()
c' = c·(1 + G·(n − 0.5)·w(Y')) ,  w(Y') = 0.25 + 3·Y'·(1 − Y')     // maximal dans les demi-teintes
```

`G = 0.06` au calme, `0.03` au sommet. Multiplicatif : les noirs restent noirs.

**Persistance** (tampon d'historique en demi-résolution, alloué à l'ouverture) :

```
masque = smoothstep(θ_p, 2·θ_p, luma(C))               // seuls les émissifs laissent une trace
uv'    = pf + (uv − pf)·(1 − ζ)                        // pf = point de fuite ; ζ = 0.004·vitesse/v*
H_t    = max(C·masque, H_{t−1}(uv')·exp(−Δt/τ_p))
C_out  = C + 0.6·(H_t − C·masque)⁺
```

`τ_p = 0.06 + 0.2·env(E_f; 50 ms, 600 ms)` s. Les traces fuient vers les bords, ce qui renforce la vitesse. La
remise à zéro de `H` est obligatoire à chaque coupe de plan.

### 3.2 Atmosphère et volumétrie

**Brouillard par fragment.** Une fonction WESL partagée, importée par tous les shaders du monde, plutôt qu'une
passe plein écran (les couches n'ont pas de profondeur commune au compositeur) :

```
τ      = σ_t·ρ0·exp(−o_y/H_f)·D·(1 − exp(−u))/u ,  u = D·d_y/H_f           // phase 2 §1.3
T      = exp(−τ)
C_brume = c_fill + c_key·P_HG(cos θ_key, g) + Σ_{i<4} c_i·I_i·tube(o, d, D, P_i)   // 4 tubes proches, forme close
C'     = C·T + C_brume·(1 − T)
```

- Rayleigh ne sert qu'à teinter : `C_brume` tire vers `azure` avec la distance (`×(0.43, 1, 2.44)` normalisé).
- La brume **éclaircit** au loin (règle 2 du §1.2) : `c_fill` au loin est au-dessus de la luminance des tours de P1.
- Modulation audio : table de la phase 2 §1.3 (densité, anisotropie, coquille de kick, bouffée de drop).
- Les plans `mist` et `haze` existants restent : ils donnent le détail proche que la formule n'a pas.

**Rayons en espace écran** (source brillante dans le champ : soleil à bandes, plasma, phare) :
`L = Σ_{i<N} C(uv + (s − uv)·ρ·i/N)·δ^i·w`, `N = 24`, `δ = 0.96`, `ρ = 0.9`, sur la cible du bloom en quart de
résolution. Éteint si la source sort de l'écran (fondu sur 0,3 s).

**Poussières et particules d'ambiance** (plan P4 et P3), trois coquilles autour de la caméra :

| Coquille | Rayon | Nombre (haut / moyen / bas) | Taille écran | Mouvement |
|---|---|---|---|---|
| Proche | 0,5–3 m | 200 / 100 / 0 | 4–12 px, flou | dérive `N3(p0, 0.1·t)`, traînée par la vitesse |
| Médiane | 3–20 m | 800 / 300 / 100 | 1,5–4 px | vent de la brume `u_vent` |
| Lointaine | 20–80 m | 1500 / 500 / 0 | 1–2 px | scintillement `h(id, floor(8·t))` |

- Position sans état : `p = p0(id) + dérive(t)`, repliée dans la coquille par `fract` autour de la caméra.
- Couleur : `C_brume` local `×(1 + 2·env(TR))`. Une poussière n'est visible que là où il y a de la lumière.
- Un seul maillage instancié, tout au vertex shader, densité liée à `QualityLevers::particle_density`.

---

## 4. Dynamisme et rythme visuel

### 4.1 Animation et mouvement

**Caméra** (`cf-cinema`). Toute pose passe par un lissage indépendant de la cadence d'image.

| Grandeur | Borne | Lissage |
|---|---|---|
| Vitesse angulaire | ≤ 60°/s (panoramique éclair : 400°/s pendant ≤ 0,25 s) | ressort critique, `f_n = 1.5 Hz` |
| Accélération angulaire | ≤ 180°/s² | idem |
| Position | à-coup borné : pas de saut de vitesse hors coupe | ressort critique, `f_n = 1 Hz` |
| Focale | ≤ 20°/s ; coup de focale `≤ 2°` sur kick | `τ = 0.2 s` |
| Roulis | ≤ 8° hors plan « dutch » | `τ = 0.5 s` |
| Tremblement | phase 2 §2.3 (`trauma²`, 9 Hz, ≤ 0,6°) | — |

Règles :

1. **Un seul mouvement à la fois** : travelling, panoramique ou zoom. Deux ensemble seulement sur un drop.
2. **Coupe sur le temps, mouvement sur la phrase** : un mouvement dure 4, 8 ou 16 temps et finit sur un temps.
3. **Entrée et sortie amorties**, jamais linéaires.
4. **Le sujet ne quitte pas les 80 % centraux**, sauf plan d'ensemble.

| Usage | Courbe |
|---|---|
| Mouvement de caméra complet | `easeInOutCubic` : `u < .5 ? 4u³ : 1 − (−2u + 2)³/2` |
| Arrivée d'un élément (enseigne, hologramme) | `easeOutExpo` : `1 − 2^(−10u)` |
| Sortie | `easeInCubic` : `u³` |
| Impact (échelle, pistons) | ressort sous-amorti `ζ = 0.4` (phase 2 §2.2) |
| Suivi continu d'un signal | attaque/relâchement ou One-Euro (phase 2 §2.2) |
| Bascule d'état (LOD, quartier) | fondu tramé 0,3 s, jamais sur un kick |

**Déformations procédurales** : celles de la phase 1 (« Maths de structure »). Deux règles en plus : amplitude
proportionnelle à la masse inverse (une tour bouge de 2 % au plus, un câble de 25 %), et vitesse apparente du
mouvement décroissante avec la distance (hiérarchie du §1.2).

### 4.2 Respiration et contraste temporel

**Densité visuelle `Δ ∈ [0, 1]`** : une seule grandeur que lisent tous les systèmes, dérivée de ce que `cf-show`
publie déjà (`intensity`, `Section`, état du drop). Elle ne remplace pas les portes du look : elle les module.

```
cible   = table[section]·(0.7 + 0.3·intensity)
Δ      += (cible − Δ)·(1 − exp(−Δt/τ)) ,  τ = 2·T_b à la montée, 8·T_b à la descente
fatigue : si Δ > 0.8 depuis plus de 32 temps, plafond = 1 − 0.3·sat((t_haut − 32·T_b)/(32·T_b))
pré-drop: Δ ← 0.15 pendant le dernier temps avant la frappe (rampe ≥ 80 ms) [L]
```

| État | `Δ` | Fenêtres allumées | Brume `ρ0` | Queue du bloom `q` | Accent (part d'image) | Particules | Aberration, persistance | Grille de coupe |
|---|---|---|---|---|---|---|---|---|
| Apnée | 0–0.15 | 5 % | ×0.3 | 0.45 | 0 % | lointaines seules | éteintes | aucune coupe |
| Calme | 0.15–0.35 | 20 % | ×0.7 | 0.5 | 3 % | ×0.3 | minimales | 16 temps |
| Groove | 0.35–0.6 | 35 % | ×1 | 0.6 | 8 % | ×0.6 | basses | 8–16 temps |
| Montée | 0.6–0.8 | 45 %, balayage vers le haut | ×1.2 | 0.7 | 10 % | ×0.8 | croissantes | 4–8 temps |
| Climax | 0.8–1 | 60 % | ×1.4 | 0.8 | 15 % | ×1 | pleines | 2–4 temps |

Règles de contraste temporel :

1. **Un climax vaut par ce qui le précède** : `Δ_sommet/moyenne(Δ sur les 8 mesures d'avant) ≥ 2`. Sinon le
   pré-drop creuse plus longtemps (jusqu'à 1 mesure).
2. **Budget de simultanéité** : trois familles d'effets au plus en même temps, parmi géométrie, émissif, caméra,
   post-process, particules, brume. Priorité à la famille la moins utilisée depuis 16 mesures (la fraîcheur de
   `cf-show` s'applique).
3. **Respiration imposée** : après 64 temps sans passage sous `Δ = 0.4`, une mesure d'épure est insérée au
   prochain début de phrase.
4. **Une seule idée par phrase** : un événement de quartier ou une idée de rupture à la fois.

---

## 5. Cinq idées de rupture

| # | Idée | Principe | Formule clé | Déclencheur | Coût et risque |
|---|---|---|---|---|---|
| 1 | **Fenêtres habitées** (fausse profondeur) | chaque fenêtre ouvre sur une pièce en volume, calculée dans le shader de façade, sans géométrie | rayon en espace tangent, boîte unité : `t = min_axe((step(0, d) − p)/d)` ; `p_pièce = p + t·d` ; identité `h(immeuble, étage, colonne)` → teinte, profondeur, silhouette de mobilier (plan à `z = 0.6`) | permanent ; la lumière de pièce suit le vu-mètre | faible : quelques instructions par pixel de fenêtre. LOD : aplat au-delà de 150 m |
| 2 | **Ville-spectrogramme** | l'horizon P1 n'est plus le spectre instantané mais son histoire : la profondeur de la ville est l'axe du temps, la ville est la forme des 64 derniers temps | texture 64 × 32 (temps × bandes log), une ligne poussée par temps ; hauteur de la tour `(i, j)` : `H = H0·(0.3 + S[j][bande(i)])`, relâchement `τ = 0.25 s` | permanent sur P1 ; au drop, l'histoire se vide puis se réécrit | très faible : une texture, un uniforme. Aucune variation de luminance |
| 3 | **Pli du monde** | la route et la ville se courbent vers le ciel pendant la montée et retombent à plat sur la frappe | au vertex, en repère caméra : `y += κ·z²`, puis enroulement `(x, y) ← R·(sin(x/R), 1 − cos(x/R)) + …` ; `κ = κ_max·easeInOutCubic(progression de la montée)` ; retour par ressort `ζ = 0.4` | montée détectée (EV2 armé), relâche sur `φ_bar → 0` | moyen : tous les shaders du monde importent la fonction ; le culling teste un frustum élargi de la flèche max (`κ_max·z_max²`), jamais de `NoFrustumCulling` |
| 4 | **Balayage temporel** (slit-scan) | chaque colonne de l'image montre un instant différent : un néon qui passe s'étire comme une matière liquide | anneau de 8 images en demi-résolution ; `C(uv) = Hist[t − τ_max·g(uv)]`, `g = abs(uv.x − 0.5)·2` ou `g = fbm(uv·3)`, interpolation entre deux images voisines | transitoires soutenus (`z(F_TR) > 2` sur 2 temps), 1 à 2 mesures | mémoire ≈ 35 Mo à 1080p, allouée à l'ouverture ; retard maximal `τ_max ≤ 130 ms` ; coupé à chaque coupe de plan |
| 5 | **Colosse holographique** | une silhouette filaire de 400 m marche derrière l'horizon, masquée par les tours, et éclaire la brume : elle donne l'échelle de toute la ville | squelette de 15 capsules animé par cycle de marche procédural (`θ_hanche = A·sin(π·φ_demi-mesure)`), un pas par blanche ; shader hologramme de la phase 1 §3.1 ; lumière dans la brume par la forme close des tubes | sommets longs (`Δ > 0.8`), 8 à 16 mesures, puis dissolution par tranches | faible : 15 instances. Risque de lisibilité : il reste dans P1, à `Y < 0.4`, jamais devant le sujet |

Ordre conseillé : 1 et 2 d'abord (permanentes, peu risquées, elles changent l'image de base), puis 4, 3 et 5.
