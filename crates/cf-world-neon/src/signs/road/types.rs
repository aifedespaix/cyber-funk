//! Les types de la route d'enseignes : plan de caméra, publicité d'un panneau, état, affichage, unité du pool.

use bevy::math::DVec3;

use super::super::ads::{AdId, AdKind};
use super::super::show_signs::LiveAd;
use super::{FormatId, Layout, POOL, SignShow, Ticker};

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
#[allow(
    clippy::struct_excessive_bools,
    reason = "des drapeaux de vie d'un panneau : flux, fantôme, radar"
)]
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "des drapeaux de vie d'une unité : prêtée, visible, bug, neuve"
)]
pub struct Unit {
    /// Rang de l'emplacement le long de la route, `None` s'il est libre ; -1 pour un panneau prêté.
    pub slot: Option<i64>,
    /// Prêté à un événement : ni recyclé ni placé par la route.
    pub leased: bool,
    /// Posé à la main (onglet Debug) : la route ne le recycle pas ; il part une fois dépassé et éteint.
    pub pinned: bool,
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
    pub(super) fn free() -> Self {
        Self {
            slot: None,
            leased: false,
            pinned: false,
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

    pub(super) fn release(&mut self) {
        self.slot = None;
        self.pinned = false;
        self.state = None;
        self.format = None;
        self.visible = false;
        self.display = Display::NORMAL;
        self.glitching = false;
        self.fresh = true;
    }

    pub(super) fn is_free(&self) -> bool {
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

/// Écart maximal retenu entre la caméra et la voiture (m) : au-delà (caméra libre, vue d'ensemble) on ne garde pas des
/// panneaux que personne ne regarde.
const EYE_SPAN_M: f64 = 150.0;

impl RoadFrame<'_> {
    /// Les z extrêmes à couvrir : (le plus en arrière, le plus en avant) entre la voiture et la caméra.
    #[must_use]
    pub fn view_span(&self) -> (f64, f64) {
        let eye = self.eye_z.clamp(self.car_z - EYE_SPAN_M, self.car_z + EYE_SPAN_M);
        (self.car_z.max(eye), self.car_z.min(eye))
    }

    /// Distances d'un panneau en `z` (positives : devant) : `(depuis le point le plus en arrière, depuis le point le plus en
    /// avant)`. Le panneau disparaît quand il est loin derrière **les deux** ; il apparaît quand il est à portée du premier.
    #[must_use]
    pub fn distances(&self, z: f64) -> (f64, f64) {
        let (rear, front) = self.view_span();
        (rear - z, front - z)
    }
}

/// Ce que la route sait de l'image.
#[derive(Debug, Clone, Copy)]
pub struct RoadFrame<'a> {
    /// Position z de la voiture (m).
    pub car_z: f64,
    /// Position z de la caméra (m). Un panneau ne disparaît que lorsqu'il est loin derrière la voiture **et** derrière
    /// l'objectif : la poursuite, le pare-chocs ou un travelling le voient encore longtemps après le passage de la voiture.
    pub eye_z: f64,
    /// Altitude de la voiture (m).
    pub altitude: f64,
    /// Horloge de la scène (s).
    pub time: f64,
    /// Le spectacle.
    pub show: &'a SignShow,
}
