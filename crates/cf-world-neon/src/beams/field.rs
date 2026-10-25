//! La logique des faisceaux (`Beams.update` de `beams.ts`) : où part chaque projecteur, où il pointe, quelle tache il
//! pose sur la route. Pur : rien de Bevy ici, tout est testable seul.
//!
//! Deux sortes de projecteurs, dans cet ordre (les premiers l'emportent quand le pool est plein) :
//!
//! - les **épinglés** ([`PinBeam`]), braqués sur un point fixe de la route par un événement du trafic (sirènes, ravitailleur,
//!   cargo publicitaire) ;
//! - les **balayeurs**, accrochés aux façades des tours ([`SpotAnchor`]), dont le point visé se promène sur la chaussée.
//!
//! # Règle 7 : balayages lisses
//!
//! Le balayage de Sillage avance « d'un cran à chaque temps » (`sin(beat × 0,9)`). Ici le compteur de temps est d'abord
//! lissé ([`BEAT_TAU_S`]) : le point visé ne saute jamais, et sa fréquence reste très sous les 3 Hz (voir
//! [`max_sweep_hz`]). Les médiums, qui règlent l'intensité, sont lissés aussi ([`MID_TAU_S`]). Le pouls des kicks que reçoit
//! [`BeamInput::kick`] est celui que le limiteur laisse passer (voir [`super::safety`]), jamais le kick brut.

use bevy::math::DVec3;

use crate::scenery::SpotAnchor;
use crate::traffic::PinBeam;

/// Projecteurs simultanés au plus (`MAX_BEAMS`).
pub const MAX_BEAMS: usize = 56;
/// Rayon de la base évasée du faisceau (m, `BEAM_RADIUS`) ; la pointe, à l'origine, a 0,1 m.
pub const BEAM_RADIUS: f32 = 1.1;
/// Hauteur de la tache au sol (m) : juste au-dessus de la route.
pub const TARGET_Y_M: f64 = 0.06;
/// Constante de temps du lissage des médiums (s) : l'intensité ne varie jamais plus vite qu'une respiration.
pub const MID_TAU_S: f32 = 0.25;
/// Constante de temps du lissage du compteur de temps (s) : le « cran » du balayage devient une glissade.
pub const BEAT_TAU_S: f32 = 0.12;

/// Vitesse du balayage lent en x (rad/s, `time × 0,55`).
const SWEEP_X_RATE: f64 = 0.55;
/// Vitesse du balayage lent en z (rad/s, `time × 0,37`).
const SWEEP_Z_RATE: f64 = 0.37;
/// Phase ajoutée par temps au balayage en x (rad par temps, `beat × 0,9`).
const SWEEP_BEAT_RATE: f64 = 0.9;

/// Un projecteur de l'image : d'où il part, où il pointe (absolus, m) et sa teinte (linéaire).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BeamSlot {
    /// Origine : la pointe du faisceau.
    pub origin: DVec3,
    /// Point visé, posé sur la route.
    pub target: DVec3,
    /// Teinte (RVB linéaire).
    pub tint: [f32; 3],
}

/// La tache lumineuse d'un faisceau sur la route (ellipse), pour savoir si la voiture roule dessus.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Spot {
    x: f64,
    z: f64,
    rx: f32,
    rz: f32,
    tint: [f32; 3],
}

/// Ce que lit [`BeamField::update`] à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct BeamInput<'a> {
    /// Durée de l'image (s).
    pub dt_s: f32,
    /// Horloge de la scène (s).
    pub time_s: f64,
    /// Compteur de temps continu de l'analyse (`beat`).
    pub beat: f64,
    /// Énergie des médiums, 0..1.
    pub mid: f32,
    /// Pouls du kick **tel que le limiteur le laisse passer**, 0..1 (nul sans spectacle, en respiration, flashs coupés).
    pub kick: f32,
    /// Points d'attache des balayeurs (façades des tours).
    pub anchors: &'a [SpotAnchor],
    /// Projecteurs épinglés par les événements.
    pub pins: &'a [PinBeam],
    /// Le cycle prismatique (RVB linéaire), pour la teinte des balayeurs.
    pub cycle: [[f32; 3]; 6],
}

/// Les projecteurs de l'image.
#[derive(Debug, Clone)]
pub struct BeamField {
    slots: [BeamSlot; MAX_BEAMS],
    spots: [Spot; MAX_BEAMS],
    count: usize,
    pinned: usize,
    mid: f32,
    beat: f64,
    primed: bool,
    cone_level: f32,
    pool_level: f32,
    radius: f32,
}

impl Default for BeamField {
    fn default() -> Self {
        Self {
            slots: [BeamSlot::default(); MAX_BEAMS],
            spots: [Spot::default(); MAX_BEAMS],
            count: 0,
            pinned: 0,
            mid: 0.0,
            beat: 0.0,
            primed: false,
            cone_level: 0.0,
            pool_level: 0.0,
            radius: BEAM_RADIUS,
        }
    }
}

/// Fréquence la plus haute (Hz) du point visé en x, à `bpm` battements par minute : le balayage lent plus celui qui suit
/// les temps. Sert la garantie « balayage lisse, sous 3 Hz » (règle 7).
#[must_use]
pub fn max_sweep_hz(bpm: f64) -> f64 {
    let slow = SWEEP_X_RATE.max(SWEEP_Z_RATE) / std::f64::consts::TAU;
    let beats = SWEEP_BEAT_RATE * (bpm / 60.0) / std::f64::consts::TAU;
    slow + beats
}

impl BeamField {
    /// Un champ vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Les projecteurs vivants de l'image.
    #[must_use]
    pub fn slots(&self) -> &[BeamSlot] {
        &self.slots[..self.count]
    }

    /// Combien de projecteurs sont épinglés (les premiers de [`Self::slots`]).
    #[must_use]
    pub fn pinned(&self) -> usize {
        self.pinned
    }

    /// Niveau du cône (`uLevel` du cône) : `0,12 + pulse × 0,3 + mid × 0,3`.
    #[must_use]
    pub fn cone_level(&self) -> f32 {
        self.cone_level
    }

    /// Niveau de la tache au sol : `0,35 + pulse × 0,8 + mid × 0,5`.
    #[must_use]
    pub fn pool_level(&self) -> f32 {
        self.pool_level
    }

    /// Rayon de la tache au sol (m, demi-axe en x ; en z il vaut `× 1,4`).
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Avance d'une image.
    pub fn update(&mut self, input: &BeamInput<'_>) {
        let dt = input.dt_s.clamp(0.0, 0.25);
        if self.primed {
            self.mid += (input.mid - self.mid) * (1.0 - (-dt / MID_TAU_S).exp());
            self.beat += (input.beat - self.beat) * f64::from(1.0 - (-dt / BEAT_TAU_S).exp());
        } else {
            // Premier appel : pas de glissade depuis zéro.
            self.mid = input.mid;
            self.beat = input.beat;
            self.primed = true;
        }
        let kick = input.kick.clamp(0.0, 1.0);
        self.cone_level = 0.12 + kick * 0.3 + self.mid * 0.3;
        self.pool_level = 0.35 + kick * 0.8 + self.mid * 0.5;
        self.radius = BEAM_RADIUS * (1.6 + kick * 0.4);

        let pinned = input.pins.len().min(MAX_BEAMS);
        self.pinned = pinned;
        self.count = pinned + input.anchors.len().min(MAX_BEAMS - pinned);
        for i in 0..self.count {
            let (origin, target, tint) = if i < pinned {
                let pin = &input.pins[i];
                let tint = pin.tint;
                (
                    pin.origin,
                    DVec3::new(f64::from(pin.target_x), TARGET_Y_M, pin.target_z),
                    [tint.red, tint.green, tint.blue],
                )
            } else {
                let a = &input.anchors[i - pinned];
                let phase = f64::from(a.phase);
                // Balayage lent + un cran par temps (adouci) ; le faisceau traverse toute la chaussée.
                let sweep_x = (input.time_s * SWEEP_X_RATE + phase).sin() * 6.0
                    + (self.beat * SWEEP_BEAT_RATE + phase * 3.0).sin() * 2.8;
                let sweep_z = (input.time_s * SWEEP_Z_RATE + phase * 2.0).sin() * 11.0 - 6.0;
                (
                    DVec3::new(f64::from(a.x), f64::from(a.y), a.z),
                    DVec3::new(sweep_x, TARGET_Y_M, a.z + sweep_z),
                    input.cycle[usize::from(a.tint) % input.cycle.len()],
                )
            };
            self.slots[i] = BeamSlot { origin, target, tint };
            self.spots[i] = Spot {
                x: target.x,
                z: target.z,
                rx: self.radius * 0.8,
                rz: self.radius * 1.4 * 0.8,
                tint,
            };
        }
    }

    /// Teinte du faisceau sur lequel roule la voiture en `(x, z)` (absolus), ou `None` si elle est dans l'ombre.
    #[must_use]
    pub fn sample(&self, x: f64, z: f64) -> Option<[f32; 3]> {
        self.spots[..self.count].iter().find_map(|spot| {
            let dx = ((x - spot.x) / f64::from(spot.rx)) as f32;
            let dz = ((z - spot.z) / f64::from(spot.rz)) as f32;
            (dx * dx + dz * dz <= 1.0).then_some(spot.tint)
        })
    }
}

#[cfg(test)]
mod tests {
    use bevy::color::LinearRgba;

    use super::*;

    const DT: f32 = 1.0 / 60.0;
    const CYCLE: [[f32; 3]; 6] = [[1.0, 0.0, 0.0]; 6];

    fn anchor(z: f64, phase: f32) -> SpotAnchor {
        SpotAnchor {
            x: 12.0,
            y: 30.0,
            z,
            side: 1,
            phase,
            tint: 2,
        }
    }

    fn input<'a>(time_s: f64, beat: f64, anchors: &'a [SpotAnchor], pins: &'a [PinBeam]) -> BeamInput<'a> {
        BeamInput {
            dt_s: DT,
            time_s,
            beat,
            mid: 0.4,
            kick: 0.0,
            anchors,
            pins,
            cycle: CYCLE,
        }
    }

    #[test]
    fn les_epingles_passent_avant_les_balayeurs_et_le_pool_est_borne() {
        let pin = PinBeam {
            origin: DVec3::new(0.0, 40.0, -100.0),
            target_x: 3.0,
            target_z: -130.0,
            tint: LinearRgba::rgb(0.0, 1.0, 0.0),
        };
        let anchors = vec![anchor(-500.0, 0.3); 80];
        let mut field = BeamField::new();
        field.update(&input(0.0, 0.0, &anchors, &[pin, pin]));
        assert_eq!(field.pinned(), 2);
        assert_eq!(field.slots().len(), MAX_BEAMS, "au plus {MAX_BEAMS} projecteurs");
        assert_eq!(field.slots()[0].target, DVec3::new(3.0, TARGET_Y_M, -130.0));
        assert_eq!(field.slots()[0].tint, [0.0, 1.0, 0.0]);
        assert_eq!(field.slots()[2].tint, CYCLE[2], "un balayeur prend la teinte du cycle");
        assert_eq!(field.slots()[2].origin, DVec3::new(12.0, 30.0, -500.0));
    }

    #[test]
    fn les_niveaux_suivent_les_formules_de_sillage() {
        let mut field = BeamField::new();
        let mut i = input(0.0, 0.0, &[], &[]);
        i.kick = 1.0;
        i.mid = 0.5;
        field.update(&i);
        assert!((field.cone_level() - (0.12 + 0.3 + 0.15)).abs() < 1e-6);
        assert!((field.pool_level() - (0.35 + 0.8 + 0.25)).abs() < 1e-6);
        assert!((field.radius() - BEAM_RADIUS * 2.0).abs() < 1e-6);
    }

    #[test]
    fn le_balayage_reste_tres_sous_trois_hertz_meme_a_200_bpm() {
        assert!(max_sweep_hz(200.0) < 1.0, "{}", max_sweep_hz(200.0));
        // Mesure : comptage des passages par la moyenne du point visé, un compteur de temps qui saute d'un cran par temps.
        let anchors = [anchor(-300.0, 1.0)];
        let mut field = BeamField::new();
        let (mut xs, mut now, mut beat) = (Vec::new(), 0.0_f64, 0.0_f64);
        for frame in 0..(60 * 60) {
            // 200 bpm : un temps tous les 18 images, le compteur monte d'un coup (le pire cas pour le « cran »).
            if frame % 18 == 0 {
                beat += 1.0;
            }
            field.update(&input(now, beat, &anchors, &[]));
            xs.push(field.slots()[0].target.x);
            now += f64::from(DT);
        }
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let crossings = xs.windows(2).filter(|w| (w[0] - mean) * (w[1] - mean) < 0.0).count();
        let hz = crossings as f64 / 2.0 / 60.0;
        assert!(hz < 3.0, "{hz} Hz");
        // Lisse : un temps qui saute d'un cran ne déplace jamais le point visé de plus de ~1 m par image.
        let max_step = xs.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        assert!(max_step < 1.0, "pas de {max_step} m");
    }

    #[test]
    fn les_mediums_sont_lisses_pas_de_saut_d_intensite() {
        let mut field = BeamField::new();
        field.update(&input(0.0, 0.0, &[], &[]));
        let before = field.cone_level();
        let mut i = input(DT.into(), 0.0, &[], &[]);
        i.mid = 1.0;
        field.update(&i);
        assert!(
            (field.cone_level() - before).abs() < 0.3 * 0.6 * (DT / MID_TAU_S) + 1e-3,
            "variation d'une image : {}",
            field.cone_level() - before
        );
    }

    #[test]
    fn la_voiture_dans_une_tache_prend_sa_teinte() {
        let mut field = BeamField::new();
        field.update(&input(0.0, 0.0, &[anchor(-300.0, 0.0)], &[]));
        let target = field.slots()[0].target;
        assert_eq!(field.sample(target.x, target.z), Some(CYCLE[2]));
        assert_eq!(field.sample(target.x + 40.0, target.z), None);
    }
}
