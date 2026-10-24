//! Le chef d'orchestre des enseignes de la route (`signs/sign-manager.ts`), sans Bevy ni rendu.
//!
//! Les emplacements sont numérotés le long de la route ; chacun reçoit un format (hologramme, portique, potence, panneau
//! de toit, écran géant, projection, néon, caméra) tiré dans une donne battue de façon déterministe
//! ([`format_at`]), donc stable d'une image à l'autre et variée de proche en proche. Un petit nombre d'unités ([`POOL`])
//! est recyclé : elles se reconfigurent pour le format qu'on leur assigne, sans rien créer ni détruire.
//!
//! Ce module décide (qui est où, quand un panneau apparaît, s'efface, bugue) ; `systems` pose les entités et le matériau
//! d'après ses unités. La peinture n'est plus à refaire : les textures sont cuites (`atlas`).

use bevy::math::{DVec3, Vec3};

use super::ads::{AdId, AdKind, CAR_FRONT_AD, CAR_SIDE_AD, NEON_COUNT, NEON_FIRST, PRODUCT_ADS, WATCH_AD};
use super::formats::{FormatId, Layout, Look, MAX_BEHIND, MAX_RANGE, Mount, format_at};
use super::show_signs::{LiveAd, LiveRole, SignShow, live_allowed, live_ad, slogan_index};
use super::structure::ROPE;
use super::ticker::Ticker;
use crate::pilot::flight::Obstruction;
use crate::rng::hash;

/// Écart entre deux emplacements le long de la route, et décalage du premier.
pub const SPACING: f64 = 105.0;
/// Décalage du premier emplacement, en m.
pub const FIRST: f64 = 90.0;
/// Neuf emplacements pour la route, plus deux prêtés au vaisseau publicitaire.
pub const POOL: usize = 11;
/// Le flux vidéo n'existe que si la caméra la plus proche est à cette portée, en m.
pub const FEED_RANGE: f64 = 230.0;
/// Part des portiques et potences que la voiture traverse en les faisant buguer au lieu de les éviter.
pub const PHANTOM_CHANCE: f64 = 0.45;
/// Distance (m) à laquelle le radar prend sa photo.
pub const SHOT_RANGE: f64 = 16.0;
/// Emplacements demandés au plus par image (étendue de la route ÷ écart, avec de la marge).
const WANTED_MAX: usize = 16;

/// Le plan que filme un écran-caméra : large depuis le portique, face à la voiture façon pub auto, ou de profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CamShot {
    /// Large, depuis le portique.
    Wide,
    /// Face à la voiture, façon pub auto.
    Front,
    /// De profil.
    Side,
}

/// La publicité d'un panneau : une du catalogue, ou une annonce du DJ figée à la pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignAd {
    /// Une publicité du catalogue.
    Catalogue(AdId),
    /// Une annonce du DJ.
    Live(LiveAd),
}

impl SignAd {
    /// Le visuel.
    #[must_use]
    pub fn kind(&self) -> AdKind {
        match self {
            Self::Catalogue(id) => id.ad().kind,
            Self::Live(live) => live.kind,
        }
    }

    /// La couleur dominante.
    #[must_use]
    pub fn accent(&self) -> cf_render::SceneColor {
        match self {
            Self::Catalogue(id) => id.ad().accent,
            Self::Live(live) => live.accent,
        }
    }

    /// Nombre de slogans.
    #[must_use]
    pub fn slogan_count(&self) -> usize {
        match self {
            Self::Catalogue(id) => id.ad().slogans.len(),
            Self::Live(live) => live.slogan_count(),
        }
    }
}

/// État d'un panneau tant qu'il est affiché (`SignState`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignState {
    /// La publicité.
    pub ad: SignAd,
    /// Graine d'ambiance propre au panneau, 0 à 2π.
    pub seed: f32,
    /// Rang du slogan affiché.
    pub slogan: u8,
    /// Instant d'apparition (s), pour que chaque panneau démarre son animation de zéro.
    pub born: f64,
    /// Distance qui sépare la voiture du panneau (positive tant qu'il est devant), en m.
    pub range: f64,
    /// Vrai pour la caméra la plus proche : seule elle a un flux à montrer.
    pub live: bool,
    /// Le plan de la caméra, fixé à la pose.
    pub shot: CamShot,
    /// Panneau qui barre la route mais ne s'évite pas : il bugue à l'approche, puis disparaît le temps du passage.
    pub phantom: bool,
    /// Le flash du radar est déjà parti pour ce passage.
    pub flashed: bool,
    /// Le radar vient de partir (une image) : le système le soumet au limiteur de flashs.
    pub radar: bool,
}

/// L'affichage d'un panneau, éventuellement abîmé par un bug (`setDisplay`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    /// Faux : l'écran est éteint, la structure reste.
    pub on: bool,
    /// Décalage de l'image, en m.
    pub dx: f32,
    /// Étirement horizontal.
    pub sx: f32,
    /// Étirement vertical.
    pub sy: f32,
    /// L'extinction est celle du passage sous le panneau (une seule, longue), pas un clignotement de bug : elle seule
    /// reste quand les flashs sont coupés.
    pub passing: bool,
}

impl Display {
    /// L'affichage normal.
    pub const NORMAL: Self = Self {
        on: true,
        dx: 0.0,
        sx: 1.0,
        sy: 1.0,
        passing: false,
    };
}

/// Un emplacement recyclable.
#[derive(Debug, Clone)]
pub struct Unit {
    /// Rang de l'emplacement le long de la route, `None` s'il est libre ; -1 pour un panneau prêté.
    pub slot: Option<i64>,
    /// Prêté à un événement : ni recyclé ni placé par la route.
    pub leased: bool,
    /// Position z (m, repère de la route).
    pub z: f64,
    /// Côté de la route, -1 ou 1.
    pub side: f32,
    /// Le format, `None` si libre.
    pub format: Option<FormatId>,
    /// Place du panneau sur la route.
    pub layout: Layout,
    /// Largeur dans le monde, en m.
    pub width: f32,
    /// Hauteur dans le monde (la moitié de la largeur), en m.
    pub height: f32,
    /// Pieds ronds plutôt que carrés.
    pub round: bool,
    /// L'état de l'affichage, `None` si libre.
    pub state: Option<SignState>,
    /// Le panneau est assez proche pour être dessiné.
    pub visible: bool,
    /// Opacité (fondu d'apparition et de disparition), 0 à 1.
    pub opacity: f32,
    /// L'affichage, abîmé ou non.
    pub display: Display,
    /// Un bug est en cours.
    pub glitching: bool,
    /// Le bandeau défilant.
    pub ticker: Ticker,
    /// Vrai pendant la première image après la pose : `systems` reconfigure alors les entités.
    pub fresh: bool,
    /// Position posée par l'événement qui a prêté le panneau (absolue, m).
    pub hang: DVec3,
}

impl Unit {
    fn free() -> Self {
        Self {
            slot: None,
            leased: false,
            z: 0.0,
            side: 1.0,
            format: None,
            layout: Layout::default(),
            width: 0.0,
            height: 0.0,
            round: false,
            state: None,
            visible: false,
            opacity: 0.0,
            display: Display::NORMAL,
            glitching: false,
            ticker: Ticker::default(),
            fresh: false,
            hang: DVec3::ZERO,
        }
    }

    fn release(&mut self) {
        self.slot = None;
        self.state = None;
        self.format = None;
        self.visible = false;
        self.display = Display::NORMAL;
        self.glitching = false;
        self.fresh = true;
    }

    fn is_free(&self) -> bool {
        self.slot.is_none() && !self.leased
    }
}

/// Les panneaux de la route.
#[derive(Debug, Clone)]
pub struct SignRoad {
    /// Les unités recyclées.
    pub units: [Unit; POOL],
}

impl Default for SignRoad {
    fn default() -> Self {
        Self {
            units: std::array::from_fn(|_| Unit::free()),
        }
    }
}

/// Ce que la route sait de l'image.
#[derive(Debug, Clone, Copy)]
pub struct RoadFrame<'a> {
    /// Position z de la voiture (m).
    pub car_z: f64,
    /// Altitude de la voiture (m).
    pub altitude: f64,
    /// Horloge de la scène (s).
    pub time: f64,
    /// Le spectacle.
    pub show: &'a SignShow,
}

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Position z de l'emplacement `k`.
#[must_use]
pub fn slot_z(k: i64) -> f64 {
    -(FIRST + k as f64 * SPACING + (hash(k as f64, 6.0) - 0.5) * 30.0)
}

/// Le plan d'une caméra, tiré une fois par panneau : large le plus souvent, sinon face à la voiture ou de profil.
fn shot_for(k: i64) -> CamShot {
    let r = hash(k as f64, 21.0);
    if r < 0.42 {
        CamShot::Wide
    } else if r < 0.78 {
        CamShot::Front
    } else {
        CamShot::Side
    }
}

fn ad_for(k: i64, look: Look, shot: CamShot) -> AdId {
    let kf = k as f64;
    match look {
        Look::Camera => match shot {
            CamShot::Front => CAR_FRONT_AD,
            CamShot::Side => CAR_SIDE_AD,
            CamShot::Wide => WATCH_AD,
        },
        Look::Neon => AdId((NEON_FIRST + (hash(kf, 8.0) * NEON_COUNT as f64).floor() as usize) as u8),
        _ => AdId(((k * 3 + (hash(kf, 1.0) * 2.0).floor() as i64).rem_euclid(PRODUCT_ADS as i64)) as u8),
    }
}

impl SignRoad {
    /// L'annonce du DJ pour cet emplacement, si le spectacle la demande et que la route n'en porte pas déjà assez.
    fn live_for(&self, k: i64, id: FormatId, except: usize, show: &SignShow) -> Option<LiveAd> {
        let (mut placed, mut live) = (0, 0);
        for (i, other) in self.units.iter().enumerate() {
            let Some(state) = &other.state else { continue };
            if i == except {
                continue;
            }
            placed += 1;
            if matches!(state.ad, SignAd::Live(_)) {
                live += 1;
            }
        }
        if live_allowed(k, id, show, placed, live) {
            live_ad(k, id, show)
        } else {
            None
        }
    }

    /// Recompose les panneaux pour cette image (`update`). `near_bridge(z, marge)` : le point est-il près d'une arche ?
    pub fn update(&mut self, frame: &RoadFrame<'_>, near_bridge: impl Fn(f64, f64) -> bool) {
        let first = (((-frame.car_z - MAX_BEHIND - FIRST) / SPACING).floor() as i64 - 1).max(0);
        let last = ((-frame.car_z + MAX_RANGE - FIRST) / SPACING).floor() as i64 + 1;
        let mut wanted = [(0_i64, FormatId::Holo); WANTED_MAX];
        let mut count = 0;
        for k in first..=last {
            let id = format_at(k);
            let format = id.format();
            let ahead = frame.car_z - slot_z(k);
            if ahead > format.range || ahead < -format.behind {
                continue;
            }
            if !matches!(format.mount, Mount::None | Mount::Tower) && near_bridge(slot_z(k), 30.0) {
                continue;
            }
            if count < WANTED_MAX {
                wanted[count] = (k, id);
                count += 1;
            }
        }
        let wanted = &wanted[..count];
        for unit in &mut self.units {
            if !unit.leased && unit.slot.is_some_and(|slot| !wanted.iter().any(|(k, _)| *k == slot)) {
                unit.release();
            }
        }
        for (k, id) in wanted {
            if self.units.iter().any(|u| u.slot == Some(*k)) {
                continue;
            }
            if let Some(free) = self.units.iter().position(Unit::is_free) {
                self.place(free, *k, *id, frame);
            }
        }
        for unit in &mut self.units {
            if unit.state.is_some() {
                step(unit, frame);
            }
        }
        self.select_feed();
    }

    fn place(&mut self, index: usize, k: i64, id: FormatId, frame: &RoadFrame<'_>) {
        let format = id.format();
        let kf = k as f64;
        let side = if hash(kf, 2.0) < 0.5 { -1.0_f32 } else { 1.0 };
        let shot = shot_for(k);
        let ad = match self.live_for(k, id, index, frame.show) {
            Some(live) => SignAd::Live(live),
            None => SignAd::Catalogue(ad_for(k, format.look, shot)),
        };
        let slogan = match ad {
            SignAd::Live(live) => match live.role {
                LiveRole::Poster => 1,
                LiveRole::Text => 0,
                LiveRole::Screen => (hash(kf, 4.0) * 2.0).floor() as u8,
            },
            SignAd::Catalogue(ad) => slogan_index(ad.ad(), k, id, frame.show.active) as u8,
        };
        let layout = id.layout(side, |salt| hash(kf, salt));
        let z = slot_z(k);
        let state = SignState {
            ad,
            seed: (hash(kf, 3.0) * 6.28) as f32,
            slogan,
            born: frame.time,
            range: 0.0,
            live: false,
            shot,
            phantom: matches!(format.mount, Mount::Gantry | Mount::Arm) && hash(kf, 31.0) < PHANTOM_CHANCE,
            flashed: false,
            radar: false,
        };
        let unit = &mut self.units[index];
        unit.slot = Some(k);
        unit.z = z;
        unit.state = Some(state);
        unit.apply(id, side, layout, format.width);
    }

    /// Prête une unité à un événement (le vaisseau publicitaire) : il la place lui-même à chaque image
    /// ([`Unit::hang`]) ; la route se charge de la faire apparaître et de la rendre. Rend son indice.
    pub fn lease(&mut self, id: FormatId, ad: SignAd, width: f32, phantom: bool, time: f64) -> Option<usize> {
        let index = self.units.iter().position(Unit::is_free)?;
        let format = id.format();
        let unit = &mut self.units[index];
        unit.leased = true;
        unit.slot = Some(-1);
        unit.z = 0.0;
        unit.state = Some(SignState {
            ad,
            seed: 3.0,
            slogan: 0,
            born: time,
            range: 0.0,
            live: false,
            shot: CamShot::Wide,
            phantom,
            flashed: false,
            radar: false,
        });
        unit.apply(id, 1.0, id.layout(1.0, |_| 0.5), if width > 0.0 { width } else { format.width });
        unit.visible = false;
        Some(index)
    }

    /// Rend une unité prêtée.
    pub fn unlease(&mut self, index: usize) {
        if let Some(unit) = self.units.get_mut(index) {
            unit.leased = false;
            unit.release();
        }
    }

    /// La caméra la plus proche, à portée, a un flux ; les autres non (`renderFeed`, sans le rendu).
    fn select_feed(&mut self) {
        let mut best: Option<(usize, f64)> = None;
        for (i, unit) in self.units.iter_mut().enumerate() {
            let Some(state) = &mut unit.state else { continue };
            if unit.format.map(|f| f.format().look) != Some(Look::Camera) {
                continue;
            }
            state.live = false;
            if !unit.visible {
                continue;
            }
            // Un panneau transporté peut être derrière la voiture et la filmer quand même.
            let out = if unit.leased {
                state.range.abs() > FEED_RANGE
            } else {
                state.range > FEED_RANGE || state.range < -4.0
            };
            if out {
                continue;
            }
            if best.is_none_or(|(_, nearest)| state.range.abs() < nearest) {
                best = Some((i, state.range.abs()));
            }
        }
        if let Some(state) = best.and_then(|(b, _)| self.units[b].state.as_mut()) {
            state.live = true;
        }
    }

    /// Les panneaux visibles et assez opaques pour être lus : (identifiant, centre de la face, direction de la face
    /// lisible, largeur). La face lisible est le +Z local du panneau, tourné par son lacet (`collectSigns`).
    pub fn collect_signs(&self, mut sink: impl FnMut(u64, DVec3, Vec3, f32)) {
        for (i, unit) in self.units.iter().enumerate() {
            if unit.state.is_none() || !unit.visible || unit.opacity < 0.35 {
                continue;
            }
            let yaw = unit.layout.yaw;
            let id = match unit.slot {
                Some(k) if k >= 0 => k as u64,
                _ => 100_000 + i as u64,
            };
            sink(id, unit.position(), Vec3::new(yaw.sin(), 0.0, yaw.cos()), unit.height * 2.0);
        }
    }

    /// Les panneaux qui barrent la route (portiques, potences, panneaux sous le vaisseau) : la tranche d'altitude qu'ils
    /// occupent, pour que la voiture volante passe dessus ou dessous. Les hologrammes et projections, posés hors des
    /// voies, ne comptent pas (`obstructions`).
    pub fn obstructions(&self, car_z: f64, out: &mut Vec<Obstruction>) {
        for unit in &self.units {
            let Some(format) = unit.format else { continue };
            let mount = format.format().mount;
            if !unit.visible || unit.slot.is_none() || unit.leased {
                continue;
            }
            if !matches!(mount, Mount::Gantry | Mount::Arm | Mount::Hang) {
                continue;
            }
            let z = unit.z;
            if z > car_z + 60.0 || z < car_z - 700.0 {
                continue;
            }
            let y = f64::from(unit.layout.y);
            let half = f64::from(unit.height) / 2.0;
            // Un panneau fantôme se traverse (son affichage s'éteint), mais sa poutre reste : seule elle est à esquiver.
            let phantom = unit.state.is_some_and(|s| s.phantom);
            let lo = if phantom { y + half + 0.3 } else { y - half - 0.5 };
            // Au-dessus du caisson : poutre ou palonnier (câbles et palonnier pour un panneau suspendu).
            let hi = if mount == Mount::Hang {
                y + half + f64::from(ROPE) + 1.2
            } else {
                y + half + 1.6
            };
            out.push(Obstruction {
                z_near: z + 1.6,
                z_far: z - 1.6,
                lo,
                hi,
            });
        }
    }
}

impl Unit {
    /// Reconfigure l'unité pour un format sans rien allouer (`apply`).
    fn apply(&mut self, id: FormatId, side: f32, layout: Layout, width: f32) {
        self.format = Some(id);
        self.side = side;
        self.layout = layout;
        self.width = width;
        self.height = width / 2.0;
        self.display = Display::NORMAL;
        self.glitching = false;
        self.round = (self.z.abs() * 0.37).floor() as i64 % 2 == 1;
        self.fresh = true;
        self.visible = true;
    }

    /// Centre du panneau, en absolu (m) : posé par la route, ou par l'événement qui l'a prêté.
    #[must_use]
    pub fn position(&self) -> DVec3 {
        if self.leased {
            self.hang
        } else {
            DVec3::new(f64::from(self.layout.x), f64::from(self.layout.y), self.z)
        }
    }
}

/// Une image de vie d'un panneau posé (`step`) : distance, fondu, bug, radar.
fn step(unit: &mut Unit, frame: &RoadFrame<'_>) {
    let (Some(id), Some(mut state)) = (unit.format, unit.state) else { return };
    let format = id.format();
    let ahead = if unit.leased {
        frame.car_z - unit.hang.z
    } else {
        frame.car_z - unit.z
    };
    state.range = ahead;
    let fade = smooth(format.range, format.range - 110.0, ahead) * smooth(-format.behind, -format.behind + 30.0, ahead);
    unit.opacity = fade as f32;
    unit.visible = fade > 0.01;
    if state.phantom {
        glitch(unit, &state, ahead, frame);
    }
    // Le radar prend sa photo quand la voiture passe sous le portique-caméra.
    state.radar = false;
    if format.look == Look::Camera {
        if !state.flashed && ahead < SHOT_RANGE && ahead > 0.0 {
            state.flashed = true;
            state.radar = true;
        }
        if ahead > SHOT_RANGE * 2.0 {
            state.flashed = false;
        }
    }
    unit.state = Some(state);
}

/// Un panneau que la voiture traverse : à l'approche son affichage saccade, se déchire et clignote de plus en plus
/// fort, il s'éteint pendant le passage, puis revient en bégayant. Seule la partie lumineuse bugue : les pieds, la
/// poutre et les câbles restent, comme de vrais objets. Rien ne se passe si la voiture n'est pas à la hauteur du panneau
/// (`glitch`).
fn glitch(unit: &mut Unit, _state: &SignState, ahead: f64, frame: &RoadFrame<'_>) {
    let centre = f64::from(unit.layout.y);
    let reach = f64::from(unit.height) / 2.0 + 3.0;
    let in_band = frame.altitude + 2.2 > centre - reach && frame.altitude - 0.7 < centre + reach;
    let level = if !in_band {
        0.0
    } else if ahead >= 0.0 {
        smooth(60.0, 8.0, ahead)
    } else {
        smooth(-34.0, -14.0, ahead)
    };
    if level <= 0.0 {
        if !unit.display.on || unit.glitching {
            unit.display = Display::NORMAL;
            unit.glitching = false;
        }
        return;
    }
    unit.glitching = true;
    if ahead < 6.0 && ahead > -14.0 {
        unit.display = Display {
            on: false,
            passing: true,
            ..Display::NORMAL
        };
        return;
    }
    let slot = unit.slot.map_or(0.0, |k| k as f64);
    let tick = (frame.time * 24.0).floor() + slot * 7.0;
    let off = hash(tick, 33.0) < level * 0.7;
    unit.display = Display {
        on: !off,
        dx: ((hash(tick, 34.0) - 0.5) * 7.0 * level) as f32,
        sx: (1.0 + (hash(tick, 35.0) - 0.5) * 0.5 * level) as f32,
        sy: (1.0 + (hash(tick, 36.0) - 0.35) * 0.35 * level) as f32,
        passing: false,
    };
}

#[cfg(test)]
mod tests;
