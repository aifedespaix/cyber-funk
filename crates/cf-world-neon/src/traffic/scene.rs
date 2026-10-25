//! Ce que les événements écrivent dans la scène en dehors des véhicules et des points lumineux : les panneaux des
//! structures ([`Panels`]), les projecteurs braqués sur la route ([`PinBeam`]) et les effets de ville ([`SceneFx`] :
//! éclair d'orage, panne de courant, pouls des fenêtres).
//!
//! Tout est préalloué et repeuplé à chaque image par [`Scene::begin`] : un événement ne garde rien ici d'une image à
//! l'autre, il réécrit ce qu'il veut voir tant qu'il vit. C'est l'équivalent de `ctx.pins` et de `ctx.field` de Sillage.
//!
//! # Règle 7 : le sens des échanges avec le limiteur
//!
//! Un événement ne règle jamais un éclat lui-même. Il **demande** (`*_request`) et il **lit** ce que le limiteur de
//! flashs du trafic ([`TrafficFlash`](super::TrafficFlash)) a accordé à l'image précédente (`kick`, `lightning`,
//! `burst`, `flashes`, `wide_ok`) : les sorties de `SceneFx` sont écrites par lui seul, jamais par un événement. Un
//! éclat plein écran ne peut donc pas contourner les plafonds (trois départs par seconde, pente de `MIN_FADE`, pic de la
//! voie, rien en respiration ni quand les flashs sont coupés).

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use super::panels::Panels;

/// Projecteurs simultanés au plus (`MAX_BEAMS` de `beams.ts`).
pub const MAX_PINS: usize = 56;

/// Un projecteur fixe braqué sur un point de la route par un événement (`PinBeam` de `beams.ts`). Le système de faisceaux
/// du monde (J5-07) lit [`Scene::pins`] ; tant qu'il n'est pas branché, les projecteurs sont publiés et personne ne les
/// dessine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinBeam {
    /// Origine du faisceau (le projecteur), absolue, m.
    pub origin: DVec3,
    /// Abscisse du point visé, posé sur la route, m.
    pub target_x: f32,
    /// Profondeur du point visé, absolue, m.
    pub target_z: f64,
    /// Teinte du faisceau.
    pub tint: LinearRgba,
}

/// Le front d'une panne de courant (`uPower` de `CityField`) : la ville est éteinte au-delà du front, sur sa « face
/// avant » (vers les z négatifs).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PowerFront {
    /// Position du front le long de la route, relative à la voiture, m.
    pub front_rel_m: f32,
    /// Demi-largeur du fondu du front, m.
    pub edge_m: f32,
    /// Part des fenêtres éteintes au-delà du front, 0 (rien) → 1 (tout).
    pub depth: f32,
}

/// Effets de ville : demandes des événements, niveaux accordés par le limiteur.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SceneFx {
    // --- Écrit par les événements, remis à zéro à chaque image par `Scene::begin` ---
    /// Un éclair d'orage vient de partir : le limiteur dit si la ville blanchit.
    pub lightning_request: bool,
    /// Une salve de feu d'artifice éclate : le limiteur dit si son éclat part.
    pub burst_request: bool,
    /// Une variation de lumière de toute la ville démarre (début de la panne, retour de la lumière).
    pub wide_request: bool,
    /// Pouls des fenêtres demandé, 0..1 (`uWinPulse`), déjà calculé par l'événement d'après `kick`.
    pub win_pulse: f32,
    /// Le front de la panne de courant.
    pub power: PowerFront,

    // --- Écrit par le limiteur de flashs du trafic seul ---
    /// Pouls du dernier kick, tel que le limiteur le laisse passer, 0..1. Nul sans spectacle, en respiration ou quand les
    /// flashs sont coupés.
    pub kick: f32,
    /// Blanchissement de la ville par l'éclair, accordé, 0..1 (1 : le plafond de la voie, `MAX_FLASH_PEAK`).
    pub lightning: f32,
    /// Éclat d'une salve, accordé, 0..1.
    pub burst: f32,
    /// Les flashs sont permis en ce moment (spectacle actif, flashs non coupés, pas de respiration).
    pub flashes: bool,
    /// La dernière variation de lumière de toute la ville a été accordée (sinon : version adoucie).
    pub wide_ok: bool,
}

impl SceneFx {
    /// Remet à zéro ce que les événements écrivent. Les niveaux accordés sont ceux du limiteur : ils restent.
    pub fn begin(&mut self) {
        self.lightning_request = false;
        self.burst_request = false;
        self.wide_request = false;
        self.win_pulse = 0.0;
        self.power = PowerFront::default();
    }
}

/// Ce que les événements écrivent dans la scène. En possession du directeur du trafic.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Les panneaux des structures.
    pub panels: Panels,
    /// Les effets de ville.
    pub fx: SceneFx,
    pins: Vec<PinBeam>,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    /// Une scène vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            panels: Panels::new(),
            fx: SceneFx::default(),
            pins: Vec::with_capacity(MAX_PINS),
        }
    }

    /// Début d'image : vide les panneaux et les projecteurs, remet à zéro les demandes d'effets.
    pub fn begin(&mut self, origin: DVec3) {
        self.panels.begin(origin);
        self.pins.clear();
        self.fx.begin();
    }

    /// Braque un projecteur (ignoré au-delà de [`MAX_PINS`]).
    pub fn pin(&mut self, beam: PinBeam) {
        if self.pins.len() < MAX_PINS {
            self.pins.push(beam);
        }
    }

    /// Les projecteurs de l'image.
    #[must_use]
    pub fn pins(&self) -> &[PinBeam] {
        &self.pins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_efface_les_demandes_mais_garde_ce_que_le_limiteur_a_accorde() {
        let mut scene = Scene::new();
        scene.fx.lightning_request = true;
        scene.fx.win_pulse = 0.7;
        scene.fx.power.depth = 0.9;
        scene.fx.kick = 0.4;
        scene.fx.flashes = true;
        scene.begin(DVec3::ZERO);
        assert!(!scene.fx.lightning_request);
        assert_eq!(scene.fx.win_pulse, 0.0);
        assert_eq!(scene.fx.power, PowerFront::default());
        assert_eq!((scene.fx.kick, scene.fx.flashes), (0.4, true));
    }

    #[test]
    fn les_projecteurs_sont_bornes_et_repartent_a_zero() {
        let mut scene = Scene::new();
        let beam = PinBeam {
            origin: DVec3::new(0.0, 30.0, -100.0),
            target_x: 2.0,
            target_z: -120.0,
            tint: LinearRgba::WHITE,
        };
        for _ in 0..(MAX_PINS + 10) {
            scene.pin(beam);
        }
        assert_eq!(scene.pins().len(), MAX_PINS);
        scene.begin(DVec3::ZERO);
        assert!(scene.pins().is_empty());
    }
}
