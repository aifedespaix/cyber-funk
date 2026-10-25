//! Chef de l'enseigne géante du DJ (`dj-sign-director.ts`), indépendant du chef d'orchestre du trafic. Il pose l'immeuble
//! support loin devant la voiture (du côté qu'il tire au sort), dégage la ville autour, laisse l'enseigne passer puis la
//! recycle. Elle vient : pendant la pré-intro, après chaque changement de DJ, sur l'ordre « afficher le DJ », et toutes
//! les 10 à 15 minutes. Elle reste hors de la chaussée : aucune obstruction pour le vol.
//!
//! Pur : le hasard est injecté (graine), la palette et l'état du spectacle sont passés à chaque image.

use bevy::math::{DVec3, Vec3};
use cf_render::{SceneColor, ScenePalette};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use super::frame::LEG_HEIGHT;
use super::sign::{DjSign, Measure, SignInput};
use crate::scenery::{BASE_DEPTH_M, Clearing};

/// Distance d'apparition devant la voiture : au-delà de la brume des façades (380 m), seul le néon se devine.
const AHEAD_M: f64 = 520.0;
/// En pré-intro la voiture roule lentement : l'enseigne apparaît plus près pour qu'on la voie arriver.
const AHEAD_INTRO_M: f64 = 360.0;
/// L'enseigne est rangée quand la voiture l'a dépassée de cette distance.
const BEHIND_M: f64 = 240.0;
/// Une enseigne qu'on a presque dépassée n'est plus « devant » : un changement de DJ en fait venir une neuve.
const STILL_AHEAD_M: f64 = -40.0;
/// Retour périodique : entre 10 et 15 minutes de spectacle.
const RETURN_MIN_S: f64 = 600.0;
const RETURN_SPAN_S: f64 = 300.0;
/// Teintes spéciales, différentes du fond de la ville ; on prend la plus éloignée de l'ambiance courante.
const SPECIAL: [SceneColor; 4] = [SceneColor::Sun, SceneColor::Amber, SceneColor::Rose, SceneColor::Lime];

/// Distance de la façade la plus proche à l'axe de la route : la chaussée et ses marges restent libres.
pub const SITE_NEAR_M: f32 = 18.0;
/// Profondeur (le long de la route) de l'immeuble support.
pub const SITE_DEPTH_M: f32 = 34.0;
/// Hauteur de l'immeuble support.
pub const SITE_HEIGHT_M: f32 = 58.0;
const PARAPET_M: f32 = 3.0;

/// Un pavé de l'immeuble support : `(x, y, z)` est le centre de sa base, `y` le bas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SiteBlock {
    /// Centre de la base, m.
    pub base: DVec3,
    /// Largeur (x), hauteur (y), profondeur (z), m.
    pub size: Vec3,
    /// Décalage de la graine de façade (fenêtres).
    pub seed_offset: u32,
}

/// L'immeuble qui porte l'enseigne : une dalle très large posée de travers à côté de la route (sa longue façade regarde la
/// voiture qui arrive), avec un parapet au sommet. Dessiné avec le matériau des gratte-ciel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SitePlan {
    /// La dalle, qui plonge sous la route.
    pub body: SiteBlock,
    /// Le parapet du toit.
    pub parapet: SiteBlock,
    /// Graine de façade tirée à la pose.
    pub seed: u32,
    /// Zone où la ville s'écarte.
    pub clearing: Clearing,
    /// Centre de l'enseigne sur x (monde).
    pub x: f32,
    /// Altitude du toit, au pied de l'enseigne.
    pub roof_y: f32,
    /// z de l'enseigne.
    pub sign_z: f64,
}

/// Pose l'immeuble pour une enseigne de `sign_width` mètres, centré en z sur `z`, du côté `side` (`SignSite.place`).
#[must_use]
pub fn place_site(z: f64, side: i8, sign_width: f32, seed: u32) -> SitePlan {
    let width = sign_width + 16.0;
    let cx = f32::from(side) * (SITE_NEAR_M + width / 2.0);
    let body = SiteBlock {
        base: DVec3::new(f64::from(cx), -f64::from(BASE_DEPTH_M), z),
        size: Vec3::new(width, BASE_DEPTH_M + SITE_HEIGHT_M, SITE_DEPTH_M),
        seed_offset: 0,
    };
    let parapet = SiteBlock {
        base: DVec3::new(
            f64::from(cx),
            f64::from(SITE_HEIGHT_M),
            z + f64::from(SITE_DEPTH_M * 0.18),
        ),
        size: Vec3::new(width - 5.0, PARAPET_M, SITE_DEPTH_M * 0.64),
        seed_offset: 5,
    };
    let margin = f64::from(SITE_DEPTH_M / 2.0 + 16.0);
    SitePlan {
        body,
        parapet,
        seed,
        clearing: Clearing {
            z0: z - margin,
            z1: z + margin,
            side,
        },
        x: cx,
        roof_y: SITE_HEIGHT_M + PARAPET_M,
        sign_z: z + f64::from(SITE_DEPTH_M * 0.18 + SITE_DEPTH_M * 0.32 - 3.0),
    }
}

/// Le spectacle tel que le chef de l'enseigne le lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowView<'a> {
    /// Un spectacle tourne.
    pub active: bool,
    /// Réglage `djSign` de l'événement : l'enseigne est permise.
    pub dj_sign: bool,
    /// Nom du DJ du créneau en cours.
    pub dj: &'a str,
    /// Monte à chaque changement de DJ.
    pub dj_seq: u32,
    /// Monte à chaque ordre « afficher le DJ ».
    pub dj_show_seq: u32,
    /// Niveau d'intensité du look (0 à 5).
    pub level: u8,
}

/// Ce que le chef de l'enseigne reçoit à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignContext<'a> {
    /// État du spectacle en mode cinéma, `None` sinon.
    pub show: Option<ShowView<'a>>,
    /// Durée de l'image, s.
    pub dt: f32,
    /// Horloge du spectacle, s.
    pub time: f64,
    /// z de la voiture, m.
    pub car_z: f64,
    /// Un kick vient de frapper.
    pub kick: bool,
    /// Pouls du tempo, 0 → 1.
    pub pulse: f32,
    /// Tempo connu.
    pub bpm: Option<f32>,
    /// Ambiance courante (couleur des liserés), linéaire.
    pub mood: [f32; 3],
    /// La pré-intro réclame l'enseigne : part de lettres à allumer, 0 → 1 ; `None` sinon.
    pub intro: Option<f32>,
    /// Les flashs sont permis par le spectacle (non coupés, hors respiration).
    pub flashes: bool,
}

/// Le chef de l'enseigne.
#[derive(Debug)]
pub struct DjSignDirector {
    sign: DjSign,
    rng: Pcg32,
    site: Option<SitePlan>,
    z: f64,
    hue: SceneColor,
    seen_dj: u32,
    seen_show: u32,
    last_spawn: f64,
    next_return: f64,
    allowed: f32,
    started: bool,
    /// Monte à chaque pose ou rangement de l'immeuble : la couche Bevy le reconstruit.
    site_seq: u32,
}

impl DjSignDirector {
    /// Un chef sans enseigne posée. `seed` règle le côté, la façade et le délai de retour.
    #[must_use]
    pub fn new(measure: Measure, calm: bool, seed: u64) -> Self {
        Self {
            sign: DjSign::new(measure, calm),
            rng: Pcg32::seed_from_u64(seed),
            site: None,
            z: 0.0,
            hue: SPECIAL[0],
            seen_dj: 0,
            seen_show: 0,
            last_spawn: 0.0,
            next_return: RETURN_MIN_S,
            allowed: 1.0,
            started: false,
            site_seq: 0,
        }
    }

    /// L'enseigne elle-même (poses des lettres, mise en page).
    #[must_use]
    pub fn sign(&self) -> &DjSign {
        &self.sign
    }

    /// L'immeuble support posé, s'il y en a un.
    #[must_use]
    pub fn site(&self) -> Option<&SitePlan> {
        self.site.as_ref()
    }

    /// Monte à chaque pose ou rangement de l'immeuble.
    #[must_use]
    pub fn site_seq(&self) -> u32 {
        self.site_seq
    }

    /// La teinte spéciale de l'enseigne posée.
    #[must_use]
    pub fn hue(&self) -> SceneColor {
        self.hue
    }

    /// Part des lettres qu'on a le droit d'allumer (la pré-intro).
    #[must_use]
    pub fn allowed(&self) -> f32 {
        self.allowed
    }

    /// La zone où la ville s'écarte, `None` si rien n'est posé.
    #[must_use]
    pub fn clearing(&self) -> Option<Clearing> {
        self.site.map(|s| s.clearing)
    }

    /// L'enseigne est posée et lisible (au moins une lettre existe).
    #[must_use]
    pub fn visible(&self) -> bool {
        self.site.is_some() && self.sign.present()
    }

    /// Distance (m) entre la voiture et l'enseigne, positive tant qu'elle est devant ; infinie si elle n'est pas là.
    #[must_use]
    pub fn distance_ahead(&self, car_z: f64) -> f64 {
        self.site.map_or(f64::INFINITY, |s| car_z - s.sign_z)
    }

    /// Le sujet filmable : centre des lettres, sens de lecture (+z) et largeur totale. `None` si rien à filmer.
    #[must_use]
    pub fn focus(&self) -> Option<(DVec3, Vec3, f32)> {
        let site = self.site.filter(|_| self.visible())?;
        let layout = self.sign.layout();
        let center = DVec3::new(
            f64::from(site.x),
            f64::from(site.roof_y + LEG_HEIGHT + layout.height * 0.5),
            site.sign_z,
        );
        Some((center, Vec3::Z, layout.width))
    }

    /// Range tout (cinéma coupé, réglage retiré).
    pub fn retire(&mut self) {
        if self.site.take().is_none() {
            return;
        }
        self.sign.clear();
        self.site_seq = self.site_seq.wrapping_add(1);
    }

    /// Avance d'une image. `palette` sert à choisir la teinte à la pose.
    pub fn update(&mut self, c: &SignContext<'_>, palette: &ScenePalette) {
        let name = c.show.filter(|s| s.active && s.dj_sign).map_or("", |s| s.dj.trim());
        let Some(show) = c.show.filter(|_| !name.is_empty()) else {
            self.retire();
            if let Some(show) = c.show {
                self.seen_dj = show.dj_seq;
                self.seen_show = show.dj_show_seq;
            }
            return;
        };
        self.sign.set_name(name);
        if !self.started {
            self.started = true;
            self.last_spawn = c.time;
        }

        let dj_changed = show.dj_seq != self.seen_dj;
        let show_requested = show.dj_show_seq != self.seen_show;
        self.seen_dj = show.dj_seq;
        self.seen_show = show.dj_show_seq;
        let present = self.site.is_some();
        let still_ahead = present && c.car_z - self.z > STILL_AHEAD_M;

        if present && c.car_z < self.z - BEHIND_M {
            self.retire();
        }
        if self.site.is_none() {
            let periodic = show.level > 0 && c.time - self.last_spawn > self.next_return;
            if c.intro.is_some() || dj_changed || show_requested || periodic {
                self.spawn(c, palette);
            }
        } else if dj_changed && !still_ahead {
            self.retire();
            self.spawn(c, palette);
        }
        if self.site.is_none() {
            return;
        }

        self.allowed = c.intro.unwrap_or(1.0);
        let input = SignInput {
            dt: c.dt,
            kick: c.kick,
            pulse: c.pulse,
            bpm: c.bpm,
            allowed: self.allowed,
            flashes: c.flashes,
        };
        self.sign.update(&input, c.time);
    }

    fn unit(&mut self) -> f64 {
        f64::from(self.rng.next_u32()) / 4_294_967_296.0
    }

    fn spawn(&mut self, c: &SignContext<'_>, palette: &ScenePalette) {
        let side: i8 = if self.unit() < 0.5 { -1 } else { 1 };
        self.sign.rearm();
        let width = self.sign.layout().width;
        let ahead = if c.intro.is_some() { AHEAD_INTRO_M } else { AHEAD_M };
        self.z = c.car_z - ahead - f64::from(SITE_DEPTH_M) / 2.0;
        let seed = (self.unit() * 900.0) as u32;
        self.site = Some(place_site(self.z, side, width, seed));
        self.site_seq = self.site_seq.wrapping_add(1);
        self.hue = pick_hue(palette, c.mood);
        self.last_spawn = c.time;
        self.next_return = RETURN_MIN_S + self.unit() * RETURN_SPAN_S;
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

/// La teinte spéciale la plus éloignée de l'ambiance et de la teinte des immeubles.
#[must_use]
pub fn pick_hue(palette: &ScenePalette, mood: [f32; 3]) -> SceneColor {
    let rgb = |c: SceneColor| {
        let v = palette.get(c);
        [v.r, v.g, v.b]
    };
    let violet = rgb(SceneColor::Violet);
    let mut best = SPECIAL[0];
    let mut score = -1.0_f32;
    for key in SPECIAL {
        let color = rgb(key);
        let d = distance(color, mood) + 0.6 * distance(color, violet);
        if d > score {
            score = d;
            best = key;
        }
    }
    best
}

#[cfg(test)]
mod tests;
