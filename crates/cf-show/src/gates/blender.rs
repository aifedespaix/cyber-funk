//! `GateBlender` de Sillage (`engine/show/gates.ts`), généralisé à un fondu entre deux looks quelconques.

use crate::model::look::MAX_FLOOR;
use crate::model::{CutGrid, EventName, FxGates, ShotId, ShotTier, ShowGates, StuntKind, StuntTier};

/// Constante de temps (s) du glissement des nombres : un changement de look ou de variation se fond, il ne saute pas
/// (`SMOOTH_SECONDS` de Sillage).
pub const SMOOTH_SECONDS: f32 = 3.0;

/// Portes du show inactif (`OPEN_GATES` de Sillage) : tout est permis, rien de nouveau n'est ajouté. Les effets propres
/// au show sont à zéro ; le trafic et les astéroïdes gardent leur comportement d'avant (≈ 14 lancers par minute).
///
/// Alloue les listes : à appeler à l'ouverture, pas par image.
#[must_use]
pub fn open_gates() -> ShowGates {
    let stunt_kinds = [
        "off-road",
        "slalom",
        "near-miss",
        "head-on",
        "threading",
        "wall-ride",
        "sign-hop",
        "loop",
        "inverted",
        "rooftop",
        "big-jump",
    ];
    let events = [
        "chase",
        "convoy",
        "crossroad",
        "tower",
        "airship",
        "swarm",
        "escort",
        "landmark",
        "bikes",
        "adship",
        "storm",
        "aurora",
        "fireworks",
        "race",
        "lanterns",
        "dancer",
        "blackout",
        "fish",
        "toll",
        "refuel",
        "tunnel",
        "city-pulse",
        "holo-title",
    ];
    ShowGates {
        max_floor: MAX_FLOOR,
        flight_rate: 1.0,
        flight_on_drop_only: false,
        tricks: true,
        stunt_tiers: ids(["common", "uncommon", "rare"], |s| StuntTier::new(s).ok()),
        stunt_kinds: ids(stunt_kinds, |s| StuntKind::new(s).ok()),
        stunt_gap_scale: 1.0,
        shot_tiers: ids(["common", "uncommon", "rare", "epic"], |s| ShotTier::new(s).ok()),
        shot_block: Vec::<ShotId>::new(),
        cut_grid: CutGrid {
            calm: 8,
            groove: 4,
            peak: 2,
        },
        salvo: true,
        events: ids(events, |s| EventName::new(s).ok()),
        event_rate: 1.0,
        meteor_rate: 14.0,
        meteor_shower: true,
        cruise: 1.0,
        ground_traffic: 1.0,
        fx: FxGates {
            rgb_split: 0.0,
            flash: 0.0,
            city_pulse: 0.0,
        },
        ascension: true,
    }
}

/// Construit une liste d'identifiants à partir de slugs écrits en dur (ignore ceux qui seraient invalides : le test
/// `open_gates_match_sillage` les compte).
fn ids<T, const N: usize>(slugs: [&str; N], make: impl Fn(&str) -> Option<T>) -> Vec<T> {
    slugs.into_iter().filter_map(make).collect()
}

/// D'où viennent les listes et les booléens des portes pendant un fondu (les nombres, eux, glissent toujours).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSource {
    /// Toujours ceux du look de départ (`from`) : le comportement de Sillage, où le niveau en cours impose ses listes
    /// et où seul l'avancement dans l'acte fait glisser les nombres.
    From,
    /// Ceux de `from` jusqu'à mi-fondu (`mix < 0,5`), puis ceux de `to` : entrée d'un cue en `Fade`.
    MidFade,
}

/// Comment un fondu traite les listes et les nombres qui doivent rester à zéro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blend {
    /// D'où viennent les listes et les booléens.
    pub lists: ListSource,
    /// Vrai : `flight_rate` et `meteor_rate` ne quittent pas zéro tant que `from` les met à zéro (pas de vol ni
    /// d'astéroïde en avance sur le niveau suivant, `STAYS_ZERO` de Sillage).
    pub hold_zero: bool,
}

impl Blend {
    /// Réglage de Sillage : progression dans un acte, listes de l'acte en cours, vol et astéroïdes retenus à zéro.
    pub const ACT_PROGRESS: Self = Self {
        lists: ListSource::From,
        hold_zero: true,
    };
    /// Entrée d'un cue en `Fade` : les listes basculent à mi-fondu, tous les nombres glissent.
    pub const CUE_FADE: Self = Self {
        lists: ListSource::MidFade,
        hold_zero: false,
    };
}

/// Fabrique les portes de l'image : listes et booléens d'un look, nombres qui glissent vers leur cible.
///
/// L'objet est unique et mis à jour sur place ; les listes ne sont recopiées que quand elles changent (aucune
/// allocation par image en régime établi).
#[derive(Debug, Clone)]
pub struct GateBlender {
    gates: ShowGates,
}

impl Default for GateBlender {
    fn default() -> Self {
        Self::new()
    }
}

impl GateBlender {
    /// Un blender sur les portes ouvertes (show inactif).
    #[must_use]
    pub fn new() -> Self {
        Self { gates: open_gates() }
    }

    /// Les portes de l'image courante.
    #[must_use]
    pub fn gates(&self) -> &ShowGates {
        &self.gates
    }

    /// Revient aux portes « tout permis » (show inactif).
    pub fn open(&mut self) {
        self.gates = open_gates();
    }

    /// Met les portes à jour.
    ///
    /// * `from` : look dont on part ; `to` : look vers lequel on va (le même, ou le suivant, si rien ne bouge) ;
    /// * `mix` : 0 → 1, avancement de `from` vers `to` (borné) ; la cible de chaque nombre est
    ///   `from + (to - from) × mix` ;
    /// * `dt` : durée de l'image, en secondes ;
    /// * `snap` : vrai pour sauter à la cible (démarrage, saut de look), sinon les nombres glissent d'un pas
    ///   `1 - exp(-dt / SMOOTH_SECONDS)` de l'écart.
    pub fn update(&mut self, from: &ShowGates, to: &ShowGates, mix: f32, blend: Blend, dt: f32, snap: bool) {
        let mix = if mix.is_nan() { 0.0 } else { mix.clamp(0.0, 1.0) };
        let source = match blend.lists {
            ListSource::MidFade if mix >= 0.5 => to,
            ListSource::From | ListSource::MidFade => from,
        };
        self.copy_lists(source);
        let k = if snap {
            1.0
        } else {
            1.0 - (-dt.max(0.0) / SMOOTH_SECONDS).exp()
        };
        let target = |own: f32, next: f32, holds: bool| {
            if holds && blend.hold_zero && own == 0.0 {
                0.0
            } else {
                own + (next - own) * mix
            }
        };
        let g = &mut self.gates;
        let glide = |current: f32, own: f32, next: f32, holds: bool| current + (target(own, next, holds) - current) * k;
        g.flight_rate = glide(g.flight_rate, from.flight_rate, to.flight_rate, true);
        g.stunt_gap_scale = glide(g.stunt_gap_scale, from.stunt_gap_scale, to.stunt_gap_scale, false);
        g.event_rate = glide(g.event_rate, from.event_rate, to.event_rate, false);
        g.meteor_rate = glide(g.meteor_rate, from.meteor_rate, to.meteor_rate, true);
        g.cruise = glide(g.cruise, from.cruise, to.cruise, false);
        g.ground_traffic = glide(g.ground_traffic, from.ground_traffic, to.ground_traffic, false);
        g.fx.rgb_split = glide(g.fx.rgb_split, from.fx.rgb_split, to.fx.rgb_split, false);
        g.fx.flash = glide(g.fx.flash, from.fx.flash, to.fx.flash, false);
        g.fx.city_pulse = glide(g.fx.city_pulse, from.fx.city_pulse, to.fx.city_pulse, false);
    }

    /// Recopie les listes et les booléens de `source`, en ne touchant aux listes que si elles diffèrent.
    fn copy_lists(&mut self, source: &ShowGates) {
        let g = &mut self.gates;
        g.max_floor = source.max_floor;
        g.flight_on_drop_only = source.flight_on_drop_only;
        g.tricks = source.tricks;
        g.cut_grid = source.cut_grid;
        g.salvo = source.salvo;
        g.meteor_shower = source.meteor_shower;
        g.ascension = source.ascension;
        assign(&mut g.stunt_tiers, &source.stunt_tiers);
        assign(&mut g.stunt_kinds, &source.stunt_kinds);
        assign(&mut g.shot_tiers, &source.shot_tiers);
        assign(&mut g.shot_block, &source.shot_block);
        assign(&mut g.events, &source.events);
    }
}

/// Remplace `dst` par `src` seulement s'ils diffèrent (réutilise la mémoire de `dst`).
fn assign<T: Clone + PartialEq>(dst: &mut Vec<T>, src: &[T]) {
    if dst.as_slice() != src {
        dst.clear();
        dst.extend_from_slice(src);
    }
}
