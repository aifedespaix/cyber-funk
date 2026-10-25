//! Les fiches des dix immeubles-vitrines à façade (`FacadeSpec` de `facade.ts` et `showcase/*.ts`) : dimensions, graine du
//! corps, couleurs passées au fragment, hauteurs des lumières de la ville. Le fragment de chacun est un shader
//! (`landmark_show_<nom>.wesl`).

use cf_render::SceneColor;

use super::super::landmark::LandmarkKind;
use super::super::part::Look;

/// Ce qui sort de la façade seule : une construction annexe qui s'ajoute au plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    /// Rien d'autre que le plan de façade.
    None,
    /// Le faisceau tournant du phare, et la lumière qui suit son bout.
    Beam,
    /// Les ondes concentriques qui partent de l'antenne.
    Waves,
    /// Les étages et les toits de la pagode (blocs de ville) à la place du corps plein.
    Blocks,
}

/// La fiche d'un immeuble-vitrine à façade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacadeSpec {
    /// Lequel.
    pub kind: LandmarkKind,
    /// Largeur le long de la route, m.
    pub span: f32,
    /// Hauteur, m.
    pub height: f32,
    /// Profondeur du corps, m.
    pub depth: f32,
    /// Graine des fenêtres du corps.
    pub seed: u32,
    /// Teinte du corps et des lumières.
    pub tint: SceneColor,
    /// Couleurs `uA`, `uB`, `uC`, `uD` du fragment.
    pub colors: [SceneColor; 4],
    /// Hauteurs (fractions de la hauteur) des lumières de la ville.
    pub lights: &'static [f32],
    /// Rayon des lumières de la ville, m.
    pub light_radius: f32,
    /// Durée de retombée du niveau de drop, s (`uDrop` : 0,8 s ; `uBlast` de la tour radio : 1,1 s).
    pub drop_decay_s: f32,
    /// Le rendu du plan de façade.
    pub look: Look,
    /// Y a-t-il un corps plein derrière la façade ? Faux pour la pagode, qui fournit tout son volume.
    pub body: bool,
    /// La construction annexe.
    pub extra: Extra,
}

use SceneColor::{Alarm, Amber, Azure, Ice, Lime, Mint, Rose, Sun, Violet};

const DROP_DECAY_S: f32 = 0.8;

/// Les dix fiches, dans l'ordre du registre de Sillage (`organ`, `aquarium`, `greenhouse`, `lighthouse`, `clock`, `ice`,
/// `radio`, `pagoda`, `console`, `hotel`).
pub const SPECS: [FacadeSpec; 10] = [
    FacadeSpec {
        kind: LandmarkKind::Organ,
        span: 44.0,
        height: 84.0,
        depth: 26.0,
        seed: 1212,
        tint: Violet,
        colors: [Ice, Violet, Rose, Amber],
        lights: &[0.22, 0.5, 0.78],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowOrgan,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Aquarium,
        span: 42.0,
        height: 70.0,
        depth: 24.0,
        seed: 3030,
        tint: Azure,
        colors: [Mint, Azure, Rose, Amber],
        lights: &[0.2, 0.5, 0.8],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowAquarium,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Greenhouse,
        span: 40.0,
        height: 72.0,
        depth: 26.0,
        seed: 4545,
        tint: Lime,
        colors: [Lime, Mint, Rose, Ice],
        lights: &[0.15, 0.4, 0.7],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowGreenhouse,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Lighthouse,
        span: 20.0,
        height: 104.0,
        depth: 20.0,
        seed: 5151,
        tint: Sun,
        colors: [Alarm, Ice, Sun, Azure],
        lights: &[0.25, 0.55],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowLighthouse,
        body: true,
        extra: Extra::Beam,
    },
    FacadeSpec {
        kind: LandmarkKind::Clock,
        span: 34.0,
        height: 96.0,
        depth: 24.0,
        seed: 6262,
        tint: Amber,
        colors: [Amber, Sun, Ice, Rose],
        lights: &[0.22, 0.68],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowClock,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Ice,
        span: 38.0,
        height: 80.0,
        depth: 26.0,
        seed: 7373,
        tint: Ice,
        colors: [Ice, Azure, Violet, Mint],
        lights: &[0.2, 0.5, 0.8],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowIce,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Radio,
        span: 24.0,
        height: 118.0,
        depth: 16.0,
        seed: 8484,
        tint: Rose,
        colors: [Ice, Rose, Alarm, Mint],
        lights: &[0.3, 0.62, 0.9],
        light_radius: 100.0,
        drop_decay_s: 1.1,
        look: Look::ShowRadio,
        body: true,
        extra: Extra::Waves,
    },
    FacadeSpec {
        kind: LandmarkKind::Pagoda,
        span: 32.0,
        height: 92.0,
        depth: 26.0,
        seed: 9595,
        tint: Rose,
        colors: [Rose, Amber, Sun, Mint],
        lights: &[0.2, 0.45, 0.7],
        light_radius: 95.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowPagoda,
        body: false,
        extra: Extra::Blocks,
    },
    FacadeSpec {
        kind: LandmarkKind::Console,
        span: 40.0,
        height: 66.0,
        depth: 22.0,
        seed: 1313,
        tint: Rose,
        colors: [Rose, Amber, Lime, Azure],
        lights: &[0.25, 0.55, 0.8],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowConsole,
        body: true,
        extra: Extra::None,
    },
    FacadeSpec {
        kind: LandmarkKind::Hotel,
        span: 36.0,
        height: 88.0,
        depth: 24.0,
        seed: 2424,
        tint: Azure,
        colors: [Azure, Amber, Rose, Ice],
        lights: &[0.2, 0.5, 0.8],
        light_radius: 100.0,
        drop_decay_s: DROP_DECAY_S,
        look: Look::ShowHotel,
        body: true,
        extra: Extra::None,
    },
];

/// La fiche de cet immeuble, s'il est de ceux-là.
#[must_use]
pub fn spec_of(kind: LandmarkKind) -> Option<&'static FacadeSpec> {
    SPECS.iter().find(|spec| spec.kind == kind)
}
