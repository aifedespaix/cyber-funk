# Monde « Hyper-Transit », phase 2 : lumière, réactivité audio, génération, personnages

> Suite de `hyper-transit.md` (phase 1), dont les notations du §0 sont reprises telles quelles
> (`B, M, H`, `env`, `k(t)`, `φ`, `φ_bar`, `F_x`, `z`, `E_f`, `E_s`, `h(id)`, `W(d,t)`).
> Document de conception : maths, tables, types de configuration. Aucun code n'en découle encore.
> Les constantes chiffrées sont des valeurs de départ à régler à l'image, pas des mesures.
>
> Trois invariants valent pour tout le document :
>
> 1. **Limiteur** : toute variation de luminance (flash, bloom, brouillard, glitch, aberration) passe
>    par le limiteur de `cf-show` (`MAX_FLASHES_PER_SECOND = 3`, `MAX_FLASH_PEAK = 0.4`, `MIN_GAP = 0.25 s`).
>    Noté **[L]** ci-dessous.
> 2. **Sans état** : un effet est une fonction pure `f(seed, t, signaux)`. Une instance qui sort du champ
>    puis revient reprend au bon endroit, sans historique CPU ni saut visible.
> 3. **Sans allocation par image** : tout passe par des uniformes et des données d'instance de taille fixe.

---

## 1. Éclairage global, reflets et moteur visuel

### 1.1 Sol mouillé

**Masque d'eau.** `w ∈ [0,1]` est l'humidité globale (météo), `p = (x,z)` la position monde.

- Flaques : `m_p = smoothstep(θ − s, θ + s, fbm(p·f_p))`, avec `θ = 1 − 0.7·w`, `s = 0.05`, `f_p = 1/6 m⁻¹`.
- Film d'eau hors flaques : `m_f = w·(0.3 + 0.2·fbm(p·2))`.
- Humidité locale : `m = max(m_p, m_f)`.

**Matériau.**

| Grandeur | Formule |
|---|---|
| Albédo | `a' = a·mix(1, 0.35, m)` (l'asphalte mouillé s'assombrit) |
| Rugosité de base | `α0 = mix(α_sec, α_eau, m)`, `α_sec = 0.85`, `α_eau = 0.04` |
| Rugosité réactive | `α = clamp(α0 + m_p·(0.10·env(SB; 5 ms, 180 ms) + 0.05·pluie) − 0.03·m·calme, 0.02, 1)`, avec `calme = 1 − sat(E_s/0.25)` |
| Normale | `n = normalize(mix(n_grain, n_onde, m_p))` |
| Fresnel | `F = F0 + (1 − F0)·(1 − n·v)^5`, `F0 = 0.02` (eau), `0.04` (asphalte sec) |
| Métal | `0` partout ; les marquages peints gardent `α ≥ 0.3` |

Lecture : le kick fait frémir les flaques (reflet plus large pendant 180 ms), et un break (`calme → 1`)
les transforme en miroir net.

**Ondes de surface** (hauteur, dont on tire `n_onde = normalize(−∂h/∂x, 1, −∂h/∂z)`) :

- Pluie : `h_p = Σ_{i<4} A_i·sin(2π·(r_i·f − τ_i·v))·exp(−κ·r_i)·(1 − τ_i)`, avec `r_i` la distance au
  centre de goutte de la cellule `i`, `τ_i = fract(t·r_g + h(cell, i))`.
- Kick : `h_k = A_k·W(d_src, t)·sin(2π·d_src/λ_k)`, `c = 6 m/s`, `λ_k = 0.4 m`, `A_k ≤ 4 mm`. La source
  est la moto, ou l'enceinte la plus proche.

**SSR, marche de rayon en espace vue.** Entrées : profondeur, normale, rugosité (prépasse) et couleur HDR
de l'image précédente, reprojetée.

```
r   = reflect(−v, n)
P0  = pos_vue ; P1 = P0 + r·d_max                    // d_max = 80 m
pas : N linéaires en espace écran (DDA), départ décalé de j·pas
      j = fract(52.9829189·fract(0.06711056·px.x + 0.00583715·px.y) + 0.618·n_image)
touche si 0 < z_rayon − z_scène < e,  e = e0·(1 + z/20)        // épaisseur
puis 4 pas de dichotomie
```

- Atténuation : `f = f_bord·f_dir·f_dist·F`.
  - `f_bord = Π_{u∈{x,y}} smoothstep(0, .08, u)·smoothstep(1, .92, u)`
  - `f_dir = smoothstep(−0.2, 0.1, r·(−v_caméra))` (un rayon qui revient vers l'œil n'a rien à lire)
  - `f_dist = 1 − sat(d_touche/d_max)²`
- Flou par rugosité (cône) : `lod = log2(max(1, 2·d_px·tan θ_c))`, avec `tan θ_c ≈ 0.9·α` (à calibrer) et
  `d_px` la longueur écran du rayon.
- Étirement vertical des reflets (signature du sol mouillé en vue rasante) : 5 échantillons le long de la
  projection écran de `r`, espacés de `lod_px·(1/max(n·v, 0.1) − 1)`.
- Rayon sans touche : `mix` vers une carte d'environnement préfiltrée, plus un terme analytique par
  enseigne proche (§1.2, point représentatif réfléchi).
- Couleur finale : `C = C_diffus + f·C_ssr + (1 − f)·F·C_env`.

**Aberration du reflet.** Un seul rayon, trois lectures décalées le long de `t⊥`, la perpendiculaire
écran à `r` :

`C_c = tex(uv_touche + δ_c·t⊥·A_ca/résolution)`, `δ = (−1, 0, +1)` pour (R, V, B)

`A_ca = clamp((a0 + a1·env(TR; 2 ms, 90 ms) + a2·z(F_TR)⁺)·(1 − n·v)²·m, 0, 6 px)`, `a0 = 0.5`, `a1 = 3`, `a2 = 1`

**Paliers de qualité.**

| Palier | N pas | Résolution SSR | Étirement | Aberration |
|---|---|---|---|---|
| Haut | 32 | 1/1 | 5 éch. | 3 lectures |
| Moyen | 16 | 1/2, remontée bilatérale | 3 éch. | 3 lectures |
| Bas | 0 (carte d'environnement + terme analytique) | — | — | décalage fixe |

### 1.2 Néons : éclairage direct, rebond, bloom

**Tube = segment lumineux** `[A, B]`, de flux `Φ` (lm) et de couleur `c`.

- Point représentatif (spéculaire) : `P_r` = point du segment le plus proche du rayon réfléchi ;
  normalisation d'énergie `norm = α/α'`, avec `α' = sat(α + L_tube/(2·d))`.
- Diffus : `E(x) = (Φ/(4π·L_tube))·∫_A^B (n·ω̂)⁺/d² dl`. On l'approche par 3 points de Gauss sur le
  segment, de poids `(5, 8, 5)/18` aux abscisses `0.5 ± 0.387`, `0.5`.
- Fenêtre de portée (éclairage groupé) : `att = sat(1 − (d/r_max)⁴)² / (d² + ε²)`, `r_max = √(Φ/Φ_seuil)`.

**Rebond unique, sans illumination globale.** Chaque enseigne porte une lumière virtuelle (VPL) qui
représente la façade qu'elle éclaire :

- Position : `P_v = P_enseigne − n_façade·0.5 m`.
- Flux : `Φ_v = ρ_mur·Φ·F_g`, avec `F_g ≈ 0.5` (la moitié du flux part vers le mur) et `ρ_mur` l'albédo
  de la façade : béton brut `0.25`, béton sombre `0.12`, tôle peinte `0.08`.
- Émission lambertienne : `I_v(ω) = Φ_v·(n_façade·ω)⁺/π`.
- La VPL éclaire le sol, la brume et les façades d'en face avec la même `att`. Elle ne projette pas d'ombre.

**Halo sur la façade porteuse** (remplace l'éclairage réel au-delà du budget de lumières) : dans le
shader de façade, pour les 4 sockets les plus proches (données d'instance) :

`L_halo = Σ_{i<4} c_i·I_i·ρ_mur·exp(−d_i/ℓ)/(1 + (d_i/d0)²)·(0.7 + 0.3·fbm(uv·40))`, `ℓ = 6 m`, `d0 = 1.2 m`

Le facteur `fbm` casse le halo sur le grain du béton. `I_i` suit l'enveloppe audio de l'enseigne : la
façade respire donc avec elle, sans lumière dynamique.

**Budget.** 64 lumières groupées au plus dans le champ (tubes et VPL confondus), triées par
`Φ·att(d_caméra)`. Le reste passe en halo.

**Diffusion dans l'air.** Le halo d'un néon dans la brume est la diffusion simple de Mie : profil
angulaire `P_HG(θ, g_halo)` (§1.3) pondéré par `(1 − T(d))`. Le bloom en donne l'approximation écran,
d'où un noyau à queue longue.

**Bloom exponentiel.** Chaîne de `N = 7` niveaux : descente 13 lectures, montée en tente 3×3.

- Seuil à genou doux, avec `l = luma(c)` :
  `s = clamp(l − θ_b + κ, 0, 2κ)²/(4κ)`, puis `c' = c·max(s, l − θ_b)/max(l, ε)`.
- Anti-luciole au premier niveau : moyenne pondérée par `1/(1 + l)`.
- Recomposition : `Bloom = Σ_{i<N} w_i·G_i`, `w_i = q^i/Σ_j q^j`, où `G_i` a un écart-type `σ_i = σ0·2^i`.
- Une somme de gaussiennes d'échelles `2^i` et de poids `q^i` suit une loi de puissance
  `K(r) ∝ r^(−p)`, `p = 2 − log2(q)` : `q = 0.5` donne `r⁻³` (air sec), `q = 0.8` donne `r^−2.3` (brume).
- Pilotage : `q = clamp(0.5 + 0.3·ρ_brume + 0.1·env(SB; 20 ms, 400 ms), 0.4, 0.9)`.
- Intensité **[L]** : `I_b = I0·(1 + 0.25·k(t))`.

À confronter au bloom déjà en place (`cf-render/src/postfx/bloom.rs`) avant d'ajouter une passe.

### 1.3 Brouillard volumétrique réactif

**Milieu.** Densité `ρ(x) = ρ0·exp(−y/H_f)·(1 + a_n·N3(x·f_n − u_vent·t))·D(x,t)`, `H_f = 25 m`.

| Composante | Coefficient de diffusion | Fonction de phase |
|---|---|---|
| Rayleigh | `β_R(λ) = β_R0·(550/λ)⁴`, soit en RVB (680, 550, 440 nm) `β_R0·(0.43, 1, 2.44)` | `P_R(θ) = 3/(16π)·(1 + cos²θ)` |
| Mie | `β_M` gris ; extinction `β_M,ext = 1.1·β_M` | `P_HG(θ, g) = (1 − g²)/(4π·(1 + g² − 2g·cos θ)^1.5)` |

`σ_s = β_R + β_M`, `σ_t = β_R + β_M,ext`. En ville de nuit, Mie domine : `β_M/β_R0 ≈ 20`. Rayleigh
sert seulement à bleuir les lointains.

**Transmittance analytique** (brume exponentielle en hauteur), pour un rayon `o + s·d` de longueur `D` :

`τ = σ_t·ρ0·exp(−o_y/H_f)·D·(1 − exp(−u))/u`, `u = D·d_y/H_f` (limite `→ 1` si `|u| < 1e−4`), puis `T = exp(−τ)`.

**Lumière diffusée.** `L = Σ_k T(s_k)·σ_s·ρ(s_k)·Δs_k·Σ_l P(θ_l)·att_l·c_l·Φ_l`

- Pas exponentiels : `s_k = D·(exp(c·(k + j)/K) − 1)/(exp(c) − 1)`, `c = 3`, `K = 24`, décalage `j` du §1.1.
- Variante en grille alignée sur le frustum (palier haut) : 160×90×64, tranche
  `z_k = n·(f/n)^(k/64)`, reprojection temporelle `mix(histo, courant, 0.1)`.
- Tube ou VPL en forme close (milieu homogène, sans phase), avec `q = o − P_l`, `b = d·q`,
  `h = √(q·q − b²)` :
  `L_l = σ_s·ρ·I_l·(atan((D + b)/h) − atan(b/h))/h`.
  Un tube compte pour 3 points de Gauss. C'est le chemin par défaut des paliers moyen et bas.
- Composition : `C = C_scène·T + L`.

**Modulation audio.**

| Paramètre | Formule | Rôle |
|---|---|---|
| Densité de fond | `ρ0 = ρ_look·(0.6 + 0.8·E_s)` | la salle s'épaissit avec l'énergie lente |
| Anisotropie | `g = mix(0.55, 0.85, env(E_f − E_s; 50 ms, 1 s)⁺)` | sur un drop, les faisceaux tranchent |
| Dispersion au kick | `D(x,t) = 1 − 0.5·Σ_i g_i·exp(−((r − c·(t − t_i))/σ)²)·exp(−(t − t_i)/τ)` | coquille de vide qui s'éloigne de la source : `c = 40 m/s`, `σ = 5 m`, `τ = 0.6 s` |
| Vent du bruit | `u_vent = u0·(1 + 2·env(LM))` | la brume file sur les nappes |
| Bouffée de drop **[L]** | enveloppe ADSR sur `ρ0` : `A = T_b/2`, `D = 2·T_b`, `S = 1.3`, `R = 8·T_b`, pic `×1.8` | montée bornée en `ΔL/s` |
| Apnée (EV4) | `ρ0 → 0.3·ρ_look` en 1 mesure | — |

Distance utile, pour le culling (§3.3) : `d_brume = ln(50)/(σ_t·ρ0)`, c'est-à-dire `T < 2 %`.

---

## 2. Logique systémique de réactivité audio

### 2.1 Décomposition de la source

| Signal | Bande | Extraction | Détecteur | Champ `AudioFeatures` actuel |
|---|---|---|---|---|
| `SB` sub-basse | 20–60 Hz | passe-bande biquad, RMS sur 1024 éch. | enveloppe | `bass` (fusionné) |
| `PB` punch | 60–150 Hz | idem | flux `F_PB`, crête | `bass`, `kick`, `pulse` |
| `LM` bas-médium | 150–500 Hz | FFT 2048, recouvrement 75 % | enveloppe | `mid` (fusionné) |
| `HM` haut-médium | 500–4000 Hz | FFT | enveloppe, spectre `S(f)` en 32 bandes log | `mid` / `high` |
| `TR` transitoires | 6–16 kHz | FFT | flux `F_TR`, HFC | `high`, `hat`, `onset`, `flux` |
| Horloge | — | `cf-analysis/tempo` (inchangé) | `φ`, `φ_bar`, `T_b` | `phase`, `bar`, `phrase`, `bpm`, `confidence` |

- Rappel de la phase 1 : `B ≈ max(SB, PB)`, `M` couvre 150 Hz–2 kHz, `H` 2–16 kHz. Les formules de la
  phase 1 restent valides telles quelles.
- La FFT 2048 à 48 kHz a une résolution de 23,4 Hz : elle ne sépare pas `SB` de `PB`. D'où le banc de
  biquads en domaine temporel pour ces deux bandes, qui a aussi moins de latence.
- Biquad passe-bande (gain crête 0 dB) : `ω0 = 2π·f0/fs`, `a = sin ω0/(2Q)` ;
  `b = (a, 0, −a)/(1 + a)`, `a1 = −2·cos ω0/(1 + a)`, `a2 = (1 − a)/(1 + a)`.
  `SB` : `f0 = 35 Hz`, `Q = 0.9`. `PB` : `f0 = 95 Hz`, `Q = 1.1`. Deux étages en cascade.
- Contenu haute fréquence : `HFC = Σ_k k·|X_k|²`. Transitoire si `z(HFC) > 2`, période réfractaire 60 ms.
- Kick : `z(F_PB) > 2 ∧ SB > 0.4`, période réfractaire `0.4·T_b`.
- Ces sous-bandes seraient une extension d'`AudioFeatures` dans `cf-analysis` (thread d'analyse, sans
  Bevy). Rien ne change dans le callback `cpal`.

### 2.2 Chaîne de filtrage

Chaîne par signal, dans cet ordre :

| Étape | Formule |
|---|---|
| 1. Niveau | `L = 10·log10(e + 1e−10)` |
| 2. AGC | `L_pic = max(L, L_pic − Δt·36/τ_agc)`, `τ_agc = 10 s` ; `x = sat((L − L_pic + R)/R)`, plage `R = 36 dB` |
| 3. Courbe | `x ← x^γ` (`γ > 1` creuse le bas, `γ < 1` gonfle) |
| 4. Seuil avec hystérésis | bascule de Schmitt `θ_on = 0.12`, `θ_off = 0.08` ; sinon zone morte `x ← sat((x − θ)/(1 − θ))` |
| 5. Attaque/relâchement | `y += (x − y)·(1 − exp(−Δt/τ))`, `τ = τ_a` si `x > y`, sinon `τ_r` |
| 6. Crête | `p = max(x, p·exp(−Δt/τ_d))`, avec maintien : pas de décroissance pendant `t_h` |
| 7. Borne de pente | `y ← y_préc + clamp(y − y_préc, −r↓·Δt, r↑·Δt)` |
| 8. Ressort (cibles géométriques) | voir ci-dessous |

Tous les coefficients sont en `1 − exp(−Δt/τ)` : le résultat ne dépend pas de la cadence d'image.

**Filtre « One-Euro »** (variante de l'étape 5 pour les signaux continus, comme le centroïde ou
`S(f)`) : `f_c = f_min + β·|ẋ_lissé|`, `a = 1/(1 + 1/(2π·f_c·Δt))`, `y += a·(x − y)`. Lent au repos
(pas de tremblement), rapide sur les fronts (pas de retard). Départ : `f_min = 1 Hz`, `β = 0.5`.

**Ressort à amortissement critique**, intégration exacte, stable quel que soit `Δt`. Avec `j = y − x` :

```
e  = exp(−ω·Δt) ; u = (v + ω·j)·Δt
y' = x + (j + u)·e
v' = (v − ω·u)·e
```

`ω = 2π·f_n`. Forme sous-amortie (`ζ < 1`), notée `ressort(x; f_n, ζ)` dans la suite, avec `ω_d = ω·√(1 − ζ²)` :

```
E  = exp(−ζ·ω·Δt) ; C = cos(ω_d·Δt) ; S = sin(ω_d·Δt) ; b = (v + ζ·ω·j)/ω_d
y' = x + E·(j·C + b·S)
v' = E·((b·ω_d − ζ·ω·j)·C − (j·ω_d + ζ·ω·b)·S)
```

Pour un rebond musical : `ζ = 0.4`, `ω = 2π/T_b`, le dépassement tombe sur le contretemps.

**Constantes de temps par signal et par famille de cible** (`τ_a / τ_r`, en ms) :

| Signal | Géométrie | Émissif | Caméra | Post-process | `γ` |
|---|---|---|---|---|---|
| `SB` | 30 / 350, ressort 3 Hz | 10 / 250 | crête `τ_d = 120` | 20 / 400 | 1.6 |
| `PB` | 8 / 180, ressort 5 Hz | 5 / 150 | crête `τ_d = 90` | 10 / 200 | 1.4 |
| `LM` | 60 / 400 | 30 / 300 | — | 80 / 500 | 1.0 |
| `HM` | 40 / 250 | 15 / 200, One-Euro sur `S(f)` | — | 40 / 300 | 0.9 |
| `TR` | — (trop rapide pour de la masse) | 2 / 90 | — | 2 / 90 | 0.8 |
| `E_s` | 2000 / 4000 | 1000 / 3000 | 1500 / 3000 | 1000 / 3000 | 1.0 |

Règle : plus la cible est massive, plus `τ` est long et plus la borne de pente est serrée. Un
transitoire ne déplace jamais de géométrie.

**Compensation de latence.** Les cibles calées sur le temps lisent `φ_vis = fract(φ + (L_audio + L_rendu)/T_b)`.
Les événements quantifiés partent sur `φ_vis → 0`.

**Type de configuration** (une ligne de la matrice, format RON) :

```ron
Route(
    source: SubBass,                      // SubBass | Punch | LowMid | HighMid | Transient | Slow | Fast | Phase | Bar
    detector: Peak(decay_s: 0.12, hold_s: 0.03),   // Envelope | Peak | Flux(z: 2.0) | Spectrum(bins: 32)
    curve: Pow(1.6),
    smooth: AttackRelease(attack_s: 0.008, release_s: 0.18),   // | OneEuro(f_min, beta) | Spring(hz, zeta)
    target: CameraShake,
    gain: 1.0, clamp: (0.0, 1.0), slew: (up: 40.0, down: 6.0),
    quantize: None,                       // None | Beat | HalfBeat | Bar
    limiter: None,                        // None | Flash | Sweep  (canal du limiteur de cf-show)
)
```

### 2.3 Cartographie d'impact

**Sub-basse et kicks (20–150 Hz) : la masse.**

| Cible | Formule | Borne |
|---|---|---|
| Macro-géométrie | phase 1, « Maths de structure » (1) et (2), avec `B_env = env(SB)` et `W` déclenché par le kick | `A ≤ 0.02·H` |
| Pulsation d'échelle globale | `S = 1 + 0.012·ressort(k(t); 4 Hz, ζ = 0.5)`, appliquée autour du point de fuite | `≤ 1,5 %` |
| Tremblement caméra | `trauma = min(1, trauma + 0.35·kick)·exp(−Δt/0.25)` ; `s = trauma²` ; `δ_i = A_i·s·N1(i, t·f)` | rotation `≤ 0,6°`, translation `≤ 4 cm`, `f = 9 Hz` |
| Coup de focale | `Δfov = 1.5°·k(t)`, relâchement 200 ms | `≤ 2°` |
| Dispersion de brouillard | `D(x,t)` du §1.3 | — |
| Flaques | `α` et `h_k` du §1.1 | — |
| Bloom | `q` et `I_b` du §1.2 **[L]** | — |

Le tremblement est coupé (`s ← 0`) quand `BPM > 170` en doubles croches de kick, et dans tout plan long
du réalisateur : il doit rester un accent.

**Médiums, synthés, voix (500–4000 Hz) : la surface.**

| Cible | Formule |
|---|---|
| Fenêtres vu-mètre | phase 1 (4) : `lit(i,j) = step(j/N_ét, Ŝ(f_i))`, `Ŝ` filtré One-Euro puis crête `τ_d = 200 ms` ; colonne décalée de `h(id)` pour que deux tours ne soient pas synchrones |
| Enseignes néon | `I = motif·(0.55 + 0.45·env(HM))·scintillement` ; saturation `sat = mix(0.7, 1, env(HM))` ; voix détectée (`HM > LM ∧ noisiness < 0.3`) : les enseignes de type texte prennent le pas sur les logos |
| Câbles | oscillateur forcé `θ̈ + 2ζ·ω0·θ̇ + ω0²·θ = F0·env(LM)·sin(2π·t/T_b + 2π·h)`, `ω0 = √(g/(0.8·f))` avec `f` la flèche (un câble pendant oscille comme un pendule de longueur `0,8·f`), `ζ = 0.08` ; forme close utilisée par le shader : `θ = Θ·sin(ω0·t + 2π·h)`, `Θ = 0.05 + 0.25·env(LM; 300 ms, 1.5 s)` rad |
| Pylônes, antennes | flexion en porte-à-faux : `Δx(y) = A·(y/H)²·(3 − y/H)/2·env(HM)·sin(ω1·t + 2π·h)`, `ω1 ∝ 1/H²` |
| Hologrammes | phase 1 §3.1, la profondeur de parallaxe et la vitesse suivent `env(HM)` |

**Transitoires et aigus (6 kHz et plus) : le grain.**

| Cible | Formule | Borne |
|---|---|---|
| Micro-particules | taux `λ = λ0·(0.2 + 3·env(TR))` ; rafale de `n = 40·z(F_TR)⁺` sur transitoire ; vie `0.4 s` ; vitesse `v0·(1 + 2·h)` ; réserve fixe de 4096 | pas d'émission hors réserve |
| Flash laser **[L]** | sur transitoire quantifié à la double croche : faisceau `I = exp(−τ_l/0.08)`, direction `h(N_beat)`, largeur `0,3°` | canal `Flash` du limiteur |
| Aberration chromatique | `uv_c = 0.5 + (uv − 0.5)·(1 + δ_c·A)`, `δ = (−1, 0, 1)`, `A = 0.002 + 0.006·env(TR; 2 ms, 90 ms)` | `A ≤ 0.01` |
| Défilement UV glitché **[L]** | `uv.x += G·(h(floor(uv.y·N), floor(t·r)) − 0.5)·A_g` ; `uv.y += G·v_s·τ_g` ; porte `G = 1` pendant `min(T_b/4, 120 ms)` après `z(F_TR) > 2.5` ; `r = 4·BPM/60` | zone `≤ 25 %` de l'image, cooldown 1 temps |
| Scintillement de tube | phase 1 §3.2, le terme de bruit passe à `0.08 + 0.12·env(TR)` | — |
| Arcs (pylônes) | phase 1 §1.2, inchangé | — |

---

## 3. Génération procédurale

### 3.1 Graines et hachage

Hachage entier (`lowbias32`), puis `h = x/2³²` :

```
x ^= x >> 16 ; x *= 0x7feb352d ; x ^= x >> 15 ; x *= 0x846ca68b ; x ^= x >> 16
```

Hiérarchie, chaque niveau dérivé du précédent par `H(parent, étiquette)` :

```
graine_monde → graine_quartier(q) → graine_cellule(i, j) → graine_socket(type, k) → graine_détail(n)
```

Une cellule se régénère donc à l'identique, seule, dans n'importe quel ordre (streaming). À brancher sur
`cf-world-neon/src/rng.rs`.

### 3.2 Distribution des 30 bâtiments

**Repère.** `s` est l'abscisse le long de la route, `x` l'écart latéral signé. Une cellule est `(i, j)`
avec `i = floor(s/c_q)`, `j = floor(x/c_q)`.

| Quartier | Maille `c_q` | Hauteur `[H_min, H_max]` | Densité `ρ_q` | Largeur de rue | Jitter |
|---|---|---|---|---|---|
| A Monolithes | 240 m | 200–600 m | 0.55 | 60 m | 0.35 |
| B Ruche | 28 m | 10–60 m | 0.92 | 4 m | 0.80 |
| C Fonderie | 90 m | 30–180 m | 0.70 | 18 m | 0.50 |

**Sites et parcelles** (Voronoï sur grille perturbée) :

- Site : `p_ij = c_q·((i, j) + 0.5 + jitter·(h2(i, j) − 0.5))`.
- Parcelle : cellule de Voronoï du site, calculée sur le voisinage 3×3.
- Rue : tout point où `F2 − F1 < w_rue`. L'emprise est la parcelle rentrée de `w_rue/2`.
- L'archétype est mis à l'échelle dans le cercle inscrit de l'emprise : `R_in = (F2 − F1)_min/2`.

**Canaux de circulation** (zones interdites, testées avant tout le reste) :

| Canal | Condition d'exclusion |
|---|---|
| Couloir de la route | `abs(x) < w_route/2 + 12 m` |
| Avenues transversales | `fract(s/L_av + h(q)) < w_av/L_av`, `L_av = 8·c_q` |
| Couloirs de drones | plafond `H ≤ y_couloir − 10 m` si `abs(x − x_d(s)) < 15 m`, `x_d(s) = 80·sin(2π·s/900)` |
| Portails du tunnel | cône de dégagement d'angle 25° sur 300 m à chaque bouche |
| Cônes de vue | couloir libre entre la route et chaque repère (`landmarks`) |

**Présence.** Le site est construit si `h(graine_cellule, "présence") < ρ_q·(0.6 + 0.4·fbm(p/(6·c_q)))`.
Sinon c'est un vide : place, parking ou cuve basse.

**Hauteur.**

`H = H_min + (H_max − H_min)·sat(fbm(p/(4·c_q)))^1.8·g(x)·(0.75 + 0.5·h)`

- `g(x) = smoothstep(x0, x1, abs(x))`, `x0 = 20 m`, `x1 = 12·c_q` : bas près de la route, haut au loin.
  La ville forme un amphithéâtre, chaque rangée se lit au-dessus de la précédente.
- Règle de contraste : si `abs(H − H_voisin) < 0.15·H` pour le voisin `(i−1, j)`, alors `H ← H·(1 ± 0.22)`,
  le signe venant de `h`. Le voisin se recalcule depuis sa graine, pas depuis l'état.

**Choix de l'archétype sans répétition.**

`arch = π_q[(i + 3·j + o_q) mod 10]`, avec `π_q` une permutation des 10 archétypes du quartier, tirée de
`graine_quartier`.

Pour tout décalage `(Δi, Δj)` avec `max(|Δi|, |Δj|) ≤ 2`, `Δi + 3·Δj` n'est jamais multiple de 10 : aucun
archétype ne se répète dans le voisinage 5×5. Le calcul est en O(1) et ne dépend d'aucun voisin. La
périodicité (10 cellules) est masquée par les vides, les hauteurs et les variations du §3.3.

**Orientation.** Lacet `ψ = ψ_route(s) + (π/2)·floor(4·h) + 0.14·(h' − 0.5)` : quatre orientations
franches, plus 4° d'écart. En quartier B, l'écart monte à 15°.

### 3.3 Sockets et variations

**Types de socket** (exposés par les archétypes de la phase 1) :

| Socket | Repère local | Accueille | Taille |
|---|---|---|---|
| `face` | centre, normale, `(l, h)` | holo, enseigne, graffiti, conduite | S / M / L / XL |
| `edge` | segment `[A, B]` | liseré néon, câble (ancre) | longueur |
| `roof` | disque `(c, R)` | antenne, parabole, cuve, évent | rayon |
| `corner` | point, bissectrice | enseigne drapeau, ancre de câble | S / M |
| `under` | rectangle, normale vers le bas | néon linéaire, évent de vapeur | `(l, p)` |

**Occupation.** Pour chaque socket `k` :

1. Score de visibilité : `v_k = (n_k·d̂_route)⁺·sat(1 − d_route/600 m)·(1 − occlusion_k)`.
2. Occupé si `h(graine_socket, "occ") < p_type(q)·(0.3 + 0.7·v_k)`. Les faces vues de la route sont
   donc servies en premier.
3. Les `K_max(palier)` sockets de plus haut `v_k` sont gardés (4 / 8 / 16 par bâtiment).

| `p_type(q)` | Holo | Enseigne | Graffiti | Antenne | Conduite | Câble |
|---|---|---|---|---|---|---|
| A | 0.5 | 0.2 | 0.1 | 0.6 | 0.1 | 0.1 |
| B | 0.2 | 0.9 | 0.8 | 0.7 | 0.5 | 0.9 |
| C | 0.1 | 0.3 | 0.5 | 0.4 | 0.9 | 0.5 |

**Tirage des 30 pubs sans doublon.** Catalogue indexé `0..29` (H01–H10, N01–N10, G01–G10), filtré par
type de socket.

`item_n = (a0 + n·pas) mod 30`, avec `a0 = floor(30·h(graine_cellule))` et
`pas ∈ {7, 11, 13, 17, 19, 23, 29}` choisi par `h`.

Le pas est premier avec 30 : un bâtiment ne répète un visuel qu'après avoir épuisé les 30. Contraintes
ajoutées (rejet, puis `n ← n + 1`) :

- taille de l'item ≤ taille du socket ;
- deux hologrammes à moins de 40 m : le second est rejeté ;
- un graffiti exige `y_socket < 12 m` ou une face `under` ;
- le visuel d'un bâtiment voisin `(i−1, j)` ou `(i, j−1)` de même index est rejeté.

**Variation par instance** (aucun bâtiment identique) :

| Paramètre | Tirage |
|---|---|
| Proportions | `(sx, sy, sz) = (1, 1, 1) + (0.15, 0.25, 0.15)·(h3 − 0.5)`, puis mise à l'échelle par `H` et `R_in` |
| Paramètres de forme | nombre de terrasses, de lames, d'empilements : `N = N0 + floor((ΔN + 1)·h)` |
| Palette d'émissif | 2 couleurs sur 5 de la palette du look, sans remise |
| Teinte et usure du béton | `ρ_mur = ρ0·(0.8 + 0.4·h)` ; `usure = h²` |
| Phase audio | `φ_id = h` : déphase les ondes, les colonnes de vu-mètre, les motifs d'enseigne |
| Bande dominante | celle de l'archétype, avec 15 % de chances de bascule vers la bande voisine |

La signature `(arch, N, palette, item_0..3)` porte plus de `10·4·10·30·29·28·27 ≈ 2,6·10⁸` combinaisons
par quartier, avant même les proportions continues.

**Antennes.** Nombre `n = floor(λ_q + h)` par socket `roof`, `λ = (1.2, 2.5, 0.8)` pour A, B, C.
Hauteur `h_a = H·(0.04 + 0.08·h²)`, inclinaison `≤ 3°`, position par disque de Poisson dans `R`
(rejet, 8 essais). Balise : `I = step(0.9, fract(t/1.6 + h))`, rouge désamorcé par le limiteur **[L]**.

**Conduites de vapeur.** Chemin de Manhattan sur la face, du socket bas vers le toit :

- Montée par tronçons `Δy_k = 3 + 9·h(k)` m ; à chaque coude, déport latéral `Δx_k = ±(1 + 3·h)` m, le
  signe ramenant vers l'axe de la face.
- Rayon `r = 0.12 + 0.2·h` m ; brides tous les 2,4 m.
- Évent au bout : taux `λ_v = λ0·(0.3 + env(PB))`, bouffée sur kick de vitesse `v0·(1 + 2·k(t))`.
- Particules en billboards tirés d'une réserve, opacité `exp(−âge/τ)·(1 − exp(−âge/0.1))`,
  dérive `u_vent` du §1.3.

**Câbles : caténaire exacte.** Ancres `P0`, `P1`. Portée horizontale `d`, dénivelé `v = y1 − y0`,
longueur `L = (1 + mou)·|P1 − P0|`, `mou = 0.03 + 0.12·h`.

1. Résoudre `sinh(u)/u = r`, avec `r = √(L² − v²)/d`. Départ `u0 = √(6·(r − 1))`, puis 4 pas de Newton :
   `u ← u − (sinh u − r·u)/(cosh u − r)`.
2. Paramètre `a = d/(2u)`. Sommet à `x_v = d/2 − a·atanh(v/L)`.
3. Courbe : `y(x) = y0 + a·(cosh((x − x_v)/a) − cosh(x_v/a))`, `x ∈ [0, d]`.
4. Flèche `f = y_corde(x_v) − y(x_v)`. Elle sert au balancement du §2.3.

Règles d'appariement :

- candidats : ancres `edge` et `corner` des cellules voisines 3×3, distance dans `[8, 60]` m ;
- accepté si `h(min(id0, id1), max(id0, id1)) < p_câble(q)`, degré `≤ 3` par ancre ;
- dégagement : au-dessus de la route, `min y(x) ≥ y_route + 7 m` ; sinon rejet ;
- quartier B : faisceaux de 2 à 5 câbles parallèles, `mou` décalé de `0.02·n`.

Rendu : un seul maillage de ruban unité (`N` segments), instancié avec `(P0, P1, a, x_v, φ_id)`. La
courbe et le balancement sont évalués dans le vertex shader. `Aabb` : la boîte des ancres, élargie de
`f + f·sin(Θ_max)`.

### 3.4 Instancing, LOD et culling

**Streaming.** Tronçons de 200 m le long de `s`. Fenêtre active `[s − 200, s + d_vue]`, avec
`d_vue = min(d_palier, d_brume)`. Les entités viennent de réserves par archétype
(`cf-render/streaming`). Rien n'est créé pendant un show.

**Données d'instance** (taille fixe, écrites une fois à l'activation du tronçon) :

```
transform 3×4 · graine u32 · arch u8 · bande u8 · φ_id f32 · palette 2×rgb8 · H f32 · sockets 4×(pos vec3, couleur rgb8)
```

Les signaux audio sont des uniformes globaux. Aucune donnée d'instance n'est réécrite par image.

**LOD par taille projetée.** `px = R_sphère·h_écran/(d·tan(fov_y/2))`.

| LOD | `px` | Contenu |
|---|---|---|
| 0 | `> 250` | maillage complet, tous les sockets, déformation au vertex |
| 1 | `60–250` | maillage simplifié, `K_max/2` sockets, câbles à 8 segments, pas de conduites |
| 2 | `8–60` | boîte avec texture d'émissif (façade vu-mètre conservée, elle se lit de loin) |
| 3 | `< 8` | supprimé, sauf la ligne d'horizon : silhouettes fusionnées en un seul maillage par tronçon |

Hystérésis de 15 % sur chaque seuil. Bascule en fondu tramé sur 0,3 s, jamais sur un kick. Câbles,
antennes et particules sortent à `px < 2`.

**Culling.**

| Test | Formule |
|---|---|
| Frustum | `Aabb` élargi des amplitudes max de déformation (phase 1), jamais de `NoFrustumCulling` |
| Brume | `d > d_brume` : supprimé, hors émissifs dont `Φ·T(d) > seuil` |
| Tunnel, courbure | portée de vue dans un tube de rayon de courbure `R_c` et de largeur `w` : `d_max = √(R_c·w − w²/4) + √(2·R_c·w)` ; les anneaux au-delà sont coupés |
| Tunnel, extérieur | dans le tunnel, toute la ville est coupée, sauf si une bouche est à moins de `d_max` : seuls les tronçons qui coupent le cône (œil, contour de la bouche) restent actifs |
| Extérieur, intérieur | hors du tunnel, l'intérieur se réduit à une carte émissive à la bouche |
| Simulation | hors champ : particules et ressorts CPU gelés. Les effets sans état (tout le reste) n'ont rien à geler |

**Budgets par palier.**

| Palier | Bâtiments visibles | Sockets | Câbles | Lumières groupées | Triangles |
|---|---|---|---|---|---|
| Haut | 600 | 4000 | 1500 | 64 | 2,5 M |
| Moyen | 350 | 1800 | 600 | 32 | 1,2 M |
| Bas | 200 | 700 | 200 | 16 | 0,5 M |

---

## 4. Vêtements et personnages : 10 styles

`u_s` est l'abscisse curviligne le long d'une couture (m), `d_s` la distance à la couture (texture de
champ de distance), `m_att ∈ [0,1]` le poids d'attache (couleur de vertex : 0 = cousu au corps, 1 = libre).

| # | Style | Matériaux et shaders | Animation du vêtement | Déclencheur audio (bande et algorithme) |
|---|---|---|---|---|
| 1 | **Ronin à visière** | casque intégral, visière OLED (émissif à balayage de lignes), cuir mat `α = 0.6` | col rigide ; pans de veste `A0 = 2 cm`, `q = 1` | visière = oscilloscope (G10) sur `HM` ; inversion de couleur sur kick, crête `τ_d = 90 ms` **[L]** |
| 2 | **Coursière fibre optique** | combinaison noire, coutures en fibre (émissif le long de `u_s`), brillance d'extrémité | franges de fibres libres, pendule `ℓ = 12 cm` | spectre `Ŝ` étalé le long des fibres, One-Euro ; bouts qui scintillent sur `z(F_TR) > 2` |
| 3 | **Cuir synthwave** | cuir verni `α = 0.15`, vernis transparent, liserés magenta et cyan, dégradé de soleil dans le dos | veste lourde : `A0 = 1 cm`, inertie seule | liserés : onde `W(u_s, t)` au kick ; le dégradé monte avec `env(E_s; 1 s, 3 s)` |
| 4 | **Masque à gaz néon** | masque à 2 cartouches lumineuses, tubes translucides (diffusion sous la surface simulée), poncho ciré | poncho : cape Verlet 8×8 | cartouches : respiration `sin²(π·φ_bar)` ; souffle de vapeur à l'expiration, taux `∝ env(PB)` |
| 5 | **Techwear cargo** | nylon ripstop (normale en micro-grille, `α = 0.5`), sangles réfléchissantes | sangles : chaînes de 6 points ; `A0 = 3 cm` | bandes réfléchissantes en chenillard, 16 pas par mesure, pas `floor(16·φ_bar)` |
| 6 | **Imper holographique** | PVC iridescent (film mince : teinte `∝ cos(2π·e·n·v/λ)`), transparence 40 % | pans longs : onde progressive, `A0 = 6 cm`, `q = ½` | épaisseur du film `e = e0·(1 + 0.3·env(HM))` : la teinte glisse sur les nappes |
| 7 | **Exo-squelette léger** | carbone tissé (anisotrope), vérins émissifs, joints en caoutchouc | rigide ; pistons `Δl = 1 cm·ressort(k; 5 Hz, ζ = 0.4)` | pistons sur `PB` ; voyants d'articulation sur transitoires, réfractaire 60 ms |
| 8 | **Kimono circuit** | soie satinée (lobe de Charlie), motif G08 (circuit hexagonal) en émissif | manches : cape Verlet 6×10 ; `q = ¼` | impulsions qui parcourent le circuit à `c ∝ BPM` ; densité `∝ env(LM)` |
| 9 | **Punk à LED** | veste cloutée (clous = LED instanciées), crête en fibres, jean délavé | crête : 5 brins, pendule amorti | LED en matrice vu-mètre (phase 1 (4)) sur `Ŝ` ; clou tiré par `h(id, N_beat)` sur transitoire |
| 10 | **Spectre drapé** | voile semi-transparent tramé, bord émissif (Fresnel), capuche profonde, visage = vide étoilé | drapé complet : onde et bruit, `A0 = 10 cm`, `q = ½`, `a2 = 0.5` | opacité `0.3 + 0.5·E_s` ; bord `∝ env(SB; 30 ms, 350 ms)` ; dissolution en apnée (EV4) |

Budget : coutures et tissus en vertex shader pour toute la foule (instanciée). Les capes Verlet sont
réservées aux 4 personnages de premier plan (64 points au plus, tampons fixes).

### 4.1 Couture lumineuse

Masque : `m_c = smoothstep(w, 0, d_s)`, `w = 1.5 mm` (élargi par `fwidth(d_s)` pour ne pas créneler au loin).

`I_c = m_c·c_id·[ I0 + I_k·W(u_s, t) + I_s·Ŝ(f(u_s)) + I_φ·P(u_s, φ_vis) ]`

| Terme | Définition | Usage |
|---|---|---|
| Repos | `I0 = 0.15` | couture toujours lisible |
| Onde de kick | `W(u_s, t)` avec `c = 3 m/s`, `τ = 0.25 s`, source `u_s = 0` au sternum | l'impulsion part du cœur vers les extrémités |
| Spectre | `f(u_s) = f_min·(f_max/f_min)^(u_s/L_c)`, `Ŝ` filtré One-Euro | fibre optique (style 2), crête (style 9) |
| Chenillard | `P = exp(−(wrap(u_s/L_c − n·φ_vis)·L_c/σ_p)²)`, `wrap` ramène dans `[−0.5, 0.5]`, `n` = tours par temps | point lumineux qui boucle, calé sur le tempo |
| Fibre : pertes | `I ← I·exp(−κ·u_s) + I_bout·exp(−(L_c − u_s)/2 mm)` | la lumière s'épuise le long de la fibre et ressort au bout |

Luminance bornée avant le bloom : `I_c ≤ I_max(palier)`. Les coutures couvrent bien moins de 25 % de
l'image, mais le terme de kick passe quand même par **[L]**.

### 4.2 Déformation du tissu au rythme

**Onde au vertex** (toute la foule), avec `(u, v)` les UV du vêtement, `v` croissant vers le bas :

`Δp = n·A·m_att·[ sin(2π·(κ_u·u + κ_v·v) − ω·t + 2π·h) + a2·N2((u, v)·f_n, t·0.7) ] − m_att·τ_i²·a_perso`

- Pulsation calée sur le tempo : `ω = 2π·q/T_b`, `q ∈ {¼, ½, 1}` selon le style. Une ondulation complète
  dure 4, 2 ou 1 temps.
- Amplitude : `A = A0·(0.3 + 0.7·env(LM; 60 ms, 400 ms)) + A_k·ressort(k(t); 4 Hz, ζ = 0.4)`, `A_k = 0.5·A0`.
- Le poids `m_att` croît vers le bas : un pan part de l'épaule, immobile, et finit libre.
- Inertie : le dernier terme fait traîner le tissu derrière les accélérations du personnage, `τ_i = 0.12 s`.
- Anti-pénétration : la composante vers le corps est écrêtée, `Δp·n ≥ −0.2·A`.
- Normale, par dérivées analytiques : `n' = normalize(n − (∂Δ/∂u)·t_u − (∂Δ/∂v)·t_v)`.
- `Aabb` du personnage élargi de `A0 + A_k`.

**Cape Verlet** (premier plan), sur une grille `N×M` de points `x`, pas de simulation fixe `h = 1/120 s` :

```
x' = x + (x − x_préc)·(1 − amort) + (g + F_vent + F_audio)·h²
puis 2 itérations : pour chaque arête (i, j) de longueur au repos l0
    δ = x_j − x_i ; corr = δ·(1 − l0/|δ|)/2 ; x_i += corr·w_i ; x_j −= corr·w_j
puis collision : capsules du torse et des bras, projection vers l'extérieur
```

- `F_audio = n·(F_B·env(SB) + F_k·k(t))·N2(point, t)` : la basse gonfle la cape, le kick la claque.
- `F_vent = c_d·(u_vent − v_perso)` : la cape suit le vent de la brume (§1.3) et la vitesse du personnage.
- Points attachés (`w = 0`) : la rangée d'épaules. `amort = 0.02`.
- Contraintes de flexion : arêtes à 2 pas, raideur 0,3.
- Nombre de pas de simulation borné à 4 par image : après une image longue, la cape ralentit, elle
  n'explose pas.

---

## 5. Découpage proposé pour les sous-agents

| Lot | Contenu | Crate | Dépend de |
|---|---|---|---|
| P2-A | Sous-bandes `SB/PB/LM/HM/TR`, HFC, extension d'`AudioFeatures` (§2.1) | `cf-analysis` | — |
| P2-B | Chaîne de filtrage et matrice `Route` en RON (§2.2) | `cf-show` (types), `cf-render/audio` (application) | P2-A |
| P2-C | Sol mouillé : matériau, ondes, SSR, aberration (§1.1) | `cf-render`, `cf-world-neon/road` | — |
| P2-D | Néons : halo de façade, VPL, bloom à loi de puissance (§1.2) | `cf-render/postfx`, `cf-world-neon/facade`, `signs` | — |
| P2-E | Brouillard volumétrique et modulation (§1.3) | `cf-render`, `cf-world-neon/weather` | P2-B |
| P2-F | Distribution, archétypes, sockets, tirage des pubs (§3.1–3.3) | `cf-world-neon/districts` | — |
| P2-G | Câbles caténaires, antennes, conduites (§3.3) | `cf-world-neon/scenery` | P2-F |
| P2-H | LOD, culling de tunnel, budgets (§3.4) | `cf-render/culling`, `streaming`, `quality` | P2-F |
| P2-I | Coutures, tissus, 10 styles (§4) | `cf-world-neon/rider` (ou nouveau module foule) | P2-B |

Tests sans matériel attendus par lot : propriétés (`proptest`) sur la caténaire (les deux ancres sont
atteintes, la longueur vaut `L`), sur la non-répétition 5×5 des archétypes et sur le tirage sans
doublon ; instantanés (`insta`) sur la génération d'un tronçon à graine fixe ; réponse de la chaîne de
filtrage à un échelon, à 30, 60 et 144 images par seconde (indépendance à la cadence).
