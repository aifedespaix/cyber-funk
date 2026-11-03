//! Où vont les graffitis : le motif mural de chaque immeuble et les projecteurs holographiques d'une rangée.
//!
//! Tout est une fonction pure de l'index de rangée et de la configuration, comme [`Row::generate`] : même rangée, même
//! bibliothèque, mêmes graffitis. Les tirages utilisent la graine d'emplacement des immeubles avec des sels propres
//! (61 à 64) : ils ne décalent aucun tirage historique, et sans bibliothèque rien n'est tiré (la ville et le golden
//! `tests/golden/scenery.json` sont inchangés).
//!
//! Dans un dégradé de quartier, le quartier d'un emplacement suit déjà une probabilité croissante (voir
//! [`super::DistrictPlan::blend_at`]) ; la densité de graffitis suit le quartier de l'emplacement, donc rien n'apparaît
//! ni ne disparaît d'un coup. Aucune allocation : [`ProjectorList`] est un tableau de taille fixe.

use super::config::SceneryConfig;
use super::districts::DistrictLook;
use super::layout::{MAX_AUTO_PROJECTORS, Row, SPACING_M};
use crate::graffiti::{GraffitiWord, Placement, ProjectorSize};
use crate::rng::hash;
use crate::route::ROAD_HALF_WIDTH_M;

/// Motifs muraux au plus : le rang + 1 tient sur les 5 bits 27..31 du tag de façade.
pub const MAX_WALL_MOTIFS: usize = 31;
/// Projecteurs au plus par rangée, manuels compris : au-delà, les derniers placements de la rangée sont ignorés.
pub const MAX_ROW_PROJECTORS: usize = 16;
/// Toit à partir duquel un projecteur automatique est grand (les tours), en m (spec, décision 5).
pub const LARGE_MIN_HEIGHT_M: f32 = 45.0;
/// Toit à partir duquel un projecteur automatique est moyen, en m ; en dessous, il est petit.
pub const MEDIUM_MIN_HEIGHT_M: f32 = 18.0;
/// Distance à l'axe d'un projecteur posé à la main : juste au bord de la route.
pub const MANUAL_X_M: f32 = ROAD_HALF_WIDTH_M + 2.0;
/// Les lettres flottent au-dessus de la barre, séparées d'elle par 30 % de leur hauteur (le faisceau).
pub const LETTER_LIFT: f32 = 0.3;

/// Demi-épaisseur du volume d'un projecteur (barre et panneau des lettres), en m.
const PANEL_HALF_M: f32 = 0.25;
/// Retrait de la barre depuis le bord du toit côté route, en m.
const ROOF_INSET_M: f64 = 0.5;
/// Garde autour d'un projecteur automatique contre les immeubles voisins, en m (en x et z).
const CLEARANCE_M: f64 = 0.5;
/// Écart minimal, le long de la route, entre un projecteur manuel et un automatique du même côté, en m.
const OVERLAP_MARGIN_M: f64 = 2.0;
/// Portée des voisins à examiner autour d'une rangée : deux demi-barres de grand projecteur, le décalage des immeubles
/// (3 m) et la marge.
const NEIGHBOR_REACH_M: f64 = 20.0 + 3.0 + OVERLAP_MARGIN_M;

/// Sels des tirages de graffitis, distincts de ceux de [`Row::generate`] (1 à 19) et du dégradé (97).
const SALT_WALL: f64 = 61.0;
const SALT_MOTIF: f64 = 62.0;
const SALT_HOLO: f64 = 63.0;
const SALT_WORD: f64 = 64.0;

/// Un projecteur holographique : une barre posée à `(x, y, z)`, qui projette le mot `word` au-dessus d'elle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projector {
    /// Centre de la barre en x (axe de la route = 0), en m.
    pub x: f32,
    /// Altitude de la barre, en m (le toit pour un automatique).
    pub y: f32,
    /// Centre de la barre le long de la route, en m (absolu).
    pub z: f64,
    /// −1 : côté gauche, 1 : côté droit.
    pub side: i8,
    /// Classe de taille.
    pub size: ProjectorSize,
    /// Rang du mot dans [`GraffitiLibrary::holo_words`].
    pub word: u16,
    /// Rotation autour de la verticale, en degrés ; 0 : barre le long de la route, texte tourné vers elle.
    pub yaw_deg: f32,
    /// Posé à la main (`placements.ron`) plutôt que tiré.
    pub manual: bool,
}

impl Projector {
    /// Valeur de remplissage des tableaux de taille fixe.
    pub const EMPTY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        side: 1,
        size: ProjectorSize::Small,
        word: 0,
        yaw_deg: 0.0,
        manual: false,
    };

    /// Boîte qui contient la barre, le faisceau et les lettres : x, y (barre → haut des lettres), z.
    #[must_use]
    pub fn extent(&self) -> ([f32; 2], [f32; 2], [f64; 2]) {
        let half = self.size.bar_len_m() / 2.0;
        let (sin, cos) = self.yaw_deg.to_radians().sin_cos();
        let dx = sin.abs() * half + PANEL_HALF_M;
        let dz = f64::from(cos.abs() * half + PANEL_HALF_M);
        let top = self.y + self.size.letter_h_m() * (1.0 + LETTER_LIFT);
        ([self.x - dx, self.x + dx], [self.y, top], [self.z - dz, self.z + dz])
    }
}

/// La classe de taille d'un projecteur automatique posé sur un toit à `top` m : les tours portent les grands.
#[must_use]
pub fn size_for_roof(top: f32) -> ProjectorSize {
    if top >= LARGE_MIN_HEIGHT_M {
        ProjectorSize::Large
    } else if top >= MEDIUM_MIN_HEIGHT_M {
        ProjectorSize::Medium
    } else {
        ProjectorSize::Small
    }
}

/// Les projecteurs d'une rangée. Tableau de taille fixe : le remplir n'alloue pas.
#[derive(Debug, Clone)]
pub struct ProjectorList {
    items: [Projector; MAX_ROW_PROJECTORS],
    len: usize,
}

impl Default for ProjectorList {
    fn default() -> Self {
        Self {
            items: [Projector::EMPTY; MAX_ROW_PROJECTORS],
            len: 0,
        }
    }
}

impl ProjectorList {
    /// Les projecteurs.
    #[must_use]
    pub fn as_slice(&self) -> &[Projector] {
        &self.items[..self.len]
    }

    /// Nombre de projecteurs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Aucun projecteur ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Vide la liste.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Ajoute un projecteur ; faux (et rien d'ajouté) si la liste est pleine.
    pub fn push(&mut self, p: Projector) -> bool {
        let Some(slot) = self.items.get_mut(self.len) else {
            return false;
        };
        *slot = p;
        self.len += 1;
        true
    }
}

/// Mots muraux, mots holographiques et projecteurs posés à la main. Vide par défaut : la ville historique.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraffitiLibrary {
    wall: Vec<String>,
    holo: Vec<String>,
    /// Placements valides, en z décroissant (distance le long de la route croissante).
    manual: Vec<Projector>,
}

impl GraffitiLibrary {
    /// Construit la bibliothèque : les mots dans l'ordre du fichier (le rang mural + 1 est le motif du tag, le rang
    /// holographique le mot du projecteur), les placements résolus en projecteurs. Un placement hors route (distance
    /// négative ou non finie), à hauteur négative, d'un côté invalide, ou dont le mot n'est pas holographique est
    /// ignoré avec un avertissement ; les mots muraux au-delà du 31e aussi.
    #[must_use]
    pub fn new(words: &[GraffitiWord], placements: &[Placement]) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut wall: Vec<String> = words.iter().filter(|w| w.wall).map(|w| w.text.clone()).collect();
        if wall.len() > MAX_WALL_MOTIFS {
            warnings.push(format!(
                "graffitis : {} mots muraux, seuls les {MAX_WALL_MOTIFS} premiers sont peints",
                wall.len()
            ));
            wall.truncate(MAX_WALL_MOTIFS);
        }
        let mut holo: Vec<String> = words.iter().filter(|w| w.holo).map(|w| w.text.clone()).collect();
        holo.truncate(usize::from(u16::MAX));
        let mut manual = Vec::with_capacity(placements.len());
        for p in placements {
            let off_road = (!p.z_m.is_finite() || p.z_m < 0.0).then_some("hors route");
            let word = holo
                .iter()
                .position(|t| *t == p.word)
                .and_then(|i| u16::try_from(i).ok());
            match (off_road.or_else(|| p.problem()), word) {
                (Some(why), _) => warnings.push(format!("placements : « {} » ignoré ({why})", p.word)),
                (None, None) => warnings.push(format!(
                    "placements : « {} » ignoré (mot inconnu ou non holographique)",
                    p.word
                )),
                (None, Some(word)) => manual.push(Projector {
                    x: f32::from(p.side) * MANUAL_X_M,
                    y: p.height_m,
                    z: -p.z_m,
                    side: p.side,
                    size: p.size,
                    word,
                    yaw_deg: p.yaw_deg,
                    manual: true,
                }),
            }
        }
        manual.sort_by(|a, b| b.z.total_cmp(&a.z));
        (Self { wall, holo, manual }, warnings)
    }

    /// Rien à peindre ni à projeter ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.wall.is_empty() && self.holo.is_empty() && self.manual.is_empty()
    }

    /// Les mots muraux : le motif `k` du tag est `wall_words()[k − 1]`.
    #[must_use]
    pub fn wall_words(&self) -> &[String] {
        &self.wall
    }

    /// Les mots holographiques, indexés par [`Projector::word`].
    #[must_use]
    pub fn holo_words(&self) -> &[String] {
        &self.holo
    }

    /// Les projecteurs posés à la main, valides, en z décroissant.
    #[must_use]
    pub fn manual(&self) -> &[Projector] {
        &self.manual
    }

    /// Les projecteurs manuels dont z est dans `[lo, hi]`.
    fn manual_between(&self, lo: f64, hi: f64) -> &[Projector] {
        let first = self.manual.partition_point(|p| p.z > hi);
        let last = self.manual.partition_point(|p| p.z >= lo).max(first);
        &self.manual[first..last]
    }
}

/// Le motif mural d'un immeuble proche (0 : aucun), tiré de sa graine d'emplacement selon la densité du quartier.
pub(super) fn wall_motif(lib: &GraffitiLibrary, look: &DistrictLook, seed: f64) -> u8 {
    let n = lib.wall.len();
    if n == 0 || look.wall_one_in == 0 || hash(seed, SALT_WALL) * f64::from(look.wall_one_in) >= 1.0 {
        return 0;
    }
    let k = ((hash(seed, SALT_MOTIF) * n as f64) as usize).min(n - 1);
    (k + 1) as u8
}

/// Le toit d'un immeuble proche, de quoi y poser un projecteur.
pub(super) struct Roof {
    /// Bord du toit côté route, en x.
    pub x_inner: f64,
    /// Centre de l'immeuble le long de la route (absolu).
    pub z: f64,
    /// Hauteur du toit (sommet du corps), en m.
    pub top: f64,
    /// −1 : gauche, 1 : droite.
    pub side: i8,
}

/// Le projecteur posé sur le toit d'un immeuble proche, s'il en porte un : au bord côté route, barre le long de la
/// route, taille selon la hauteur du toit. Le test de collision avec les voisins se fait dans [`row_projectors`].
pub(super) fn auto_projector(lib: &GraffitiLibrary, look: &DistrictLook, seed: f64, roof: Roof) -> Option<Projector> {
    let n = lib.holo.len();
    if n == 0 || look.holo_one_in == 0 || hash(seed, SALT_HOLO) * f64::from(look.holo_one_in) >= 1.0 {
        return None;
    }
    let top = roof.top as f32;
    Some(Projector {
        x: (roof.x_inner + f64::from(roof.side) * ROOF_INSET_M) as f32,
        y: top,
        z: roof.z,
        side: roof.side,
        size: size_for_roof(top),
        word: ((hash(seed, SALT_WORD) * n as f64) as usize).min(n - 1) as u16,
        yaw_deg: 0.0,
        manual: false,
    })
}

/// Les projecteurs de la rangée `index` : d'abord les manuels qui y tombent (la rangée la plus proche de leur
/// distance), prioritaires, puis les automatiques qui ne traversent aucun immeuble et ne chevauchent aucun manuel du
/// même côté (rangées voisines comprises). Sans allocation : quelques rangées voisines sont régénérées sur la pile.
pub fn row_projectors(index: i64, config: &SceneryConfig, out: &mut ProjectorList) {
    out.clear();
    let lib = config.graffiti();
    if lib.is_empty() {
        return;
    }
    let row_z = -(index as f64) * SPACING_M;
    let near = lib.manual_between(
        row_z - SPACING_M / 2.0 - NEIGHBOR_REACH_M,
        row_z + SPACING_M / 2.0 + NEIGHBOR_REACH_M,
    );
    for p in near {
        if (-p.z / SPACING_M).round() as i64 == index {
            out.push(*p);
        }
    }

    let row = Row::generate(index, config);
    let autos = row.auto_projectors();
    if autos.is_empty() {
        return;
    }
    let mut blocked = [false; MAX_AUTO_PROJECTORS];
    for (a, flag) in autos.iter().zip(blocked.iter_mut()) {
        *flag = near.iter().any(|m| overlaps(a, m));
    }
    for other in Row::rows_near(row_z, NEIGHBOR_REACH_M) {
        let other = Row::generate(other, config);
        for (a, flag) in autos.iter().zip(blocked.iter_mut()) {
            *flag = *flag || crosses(a, other.hitboxes());
        }
    }
    for (a, flag) in autos.iter().zip(blocked) {
        if !flag {
            out.push(*a);
        }
    }
}

/// Deux projecteurs du même côté dont les barres se recouvrent le long de la route (avec la marge) : vus de la route,
/// ils se masqueraient.
fn overlaps(a: &Projector, b: &Projector) -> bool {
    let reach = f64::from(a.size.bar_len_m() + b.size.bar_len_m()) / 2.0 + OVERLAP_MARGIN_M;
    a.side == b.side && (a.z - b.z).abs() < reach
}

/// Le volume du projecteur (élargi de la garde en x et z) entre-t-il dans une tour ? Le toit qui le porte ne compte
/// pas : la barre est posée dessus, pas dedans.
fn crosses(projector: &Projector, hitboxes: &[super::Hitbox]) -> bool {
    let (xs, ys, zs) = projector.extent();
    let guard = CLEARANCE_M as f32;
    hitboxes.iter().any(|hit| {
        xs[0] - guard < hit.x[1]
            && xs[1] + guard > hit.x[0]
            && zs[0] - CLEARANCE_M < hit.z[1]
            && zs[1] + CLEARANCE_M > hit.z[0]
            && ys[0] < hit.top
    })
}
