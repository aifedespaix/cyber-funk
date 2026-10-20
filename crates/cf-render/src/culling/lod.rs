//! Niveau 3 du culling : la simulation. Ce qui est hors champ ou loin s'anime moins souvent.
//!
//! Le frustum et l'occlusion épargnent le GPU ; ils ne dispensent pas le processeur de faire tourner l'animation d'un
//! objet qu'on ne voit pas (enseignes qui clignotent, voitures qui roulent, fenêtres qui pulsent). [`SimulationLod`]
//! donne à chaque objet une **cadence** : tous les systèmes d'animation coûteux la consultent au lieu de tourner à
//! chaque image.
//!
//! # Contrat
//!
//! - Seule l'**animation** est ralentie ou gelée. Les décisions de mise en scène (quel événement se déclenche, où
//!   tombe la cascade, quel plan de caméra) restent calculées à chaque image par la logique, qui n'a pas à regarder
//!   ce composant : un objet hors champ doit être au bon état quand il rentre dans le champ.
//! - Un système d'animation lit [`SimulationLod::due`] (« mon tour ? ») et intègre [`SimulationLod::step_s`] (le temps
//!   écoulé depuis son dernier tour, pas le temps d'une image) : une enseigne à un tour sur quatre avance de quatre
//!   images d'un coup, elle ne tourne pas quatre fois moins vite.
//! - Hors champ, la cadence tombe à un tour sur huit, puis au gel au-delà de [`LodBands::freeze_m`]. Gelé, le temps
//!   n'est pas rattrapé (`step_s` repart de zéro à la reprise) : réservé au décor dont l'état ne se raccorde à rien.
//! - Les tours sont **décalés par entité** (une phase tirée de l'identifiant) : les objets d'une même cadence ne
//!   tombent pas tous sur la même image, la charge est lissée.
//! - Le composant se pose sur une entité **dessinable** (un `Mesh3d`). `ViewVisibility` n'est calculée que pour elles ;
//!   sur une entité sans mesh, elle resterait fausse et l'objet serait pris pour hors champ.
//!
//! La visibilité lue est celle de l'image précédente (Bevy la calcule en `PostUpdate`) : un objet qui entre dans le
//! champ repart donc à pleine cadence une image après. Invisible, puisque l'accumulation de temps rattrape.

use bevy::camera::visibility::ViewVisibility;
use bevy::prelude::*;

use super::{CullingSettings, CullingView};
use crate::streaming::splitmix64;

/// Distances (m, de la caméra de culling) qui découpent la cadence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodBands {
    /// En dessous, à l'écran : une image sur une.
    pub near_m: f32,
    /// En dessous, à l'écran : une image sur deux ; au-delà, une sur quatre.
    pub far_m: f32,
    /// Hors champ en dessous : une image sur huit ; au-delà, gelé.
    pub freeze_m: f32,
}

impl Default for LodBands {
    /// Bornes de départ pour la ville de Sillage : la rangée de façades proche (13 à 60 m) à pleine cadence, la
    /// ville jusqu'à la brume demi-cadence, gel hors champ au-delà de 150 m. À régler sur la scène réelle (J5).
    fn default() -> Self {
        Self {
            near_m: 60.0,
            far_m: 200.0,
            freeze_m: 150.0,
        }
    }
}

/// À quelle fréquence un objet est animé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cadence {
    /// Chaque image.
    #[default]
    Full,
    /// Une image sur deux.
    Half,
    /// Une image sur quatre.
    Quarter,
    /// Une image sur huit.
    Eighth,
    /// Jamais : l'animation est suspendue.
    Frozen,
}

impl Cadence {
    /// Nombre d'images entre deux tours ; `None` si gelé.
    #[must_use]
    pub const fn period(self) -> Option<u32> {
        match self {
            Self::Full => Some(1),
            Self::Half => Some(2),
            Self::Quarter => Some(4),
            Self::Eighth => Some(8),
            Self::Frozen => None,
        }
    }

    /// La cadence d'un objet à `distance_m` de la caméra, à l'écran (`visible`) ou non.
    ///
    /// Une distance qui n'est pas un nombre est traitée comme infiniment loin (plutôt que de donner `Full` à tout
    /// ce qu'on ne sait pas situer).
    #[must_use]
    pub fn select(bands: &LodBands, visible: bool, distance_m: f32) -> Self {
        let distance_m = if distance_m.is_nan() { f32::INFINITY } else { distance_m };
        if visible {
            if distance_m < bands.near_m {
                Self::Full
            } else if distance_m < bands.far_m {
                Self::Half
            } else {
                Self::Quarter
            }
        } else if distance_m < bands.freeze_m {
            Self::Eighth
        } else {
            Self::Frozen
        }
    }
}

/// Cadence d'animation d'un objet, fixée à chaque image par `update_simulation_lod` (ensemble `CullingSet::Lod`).
///
/// Voir la documentation du module pour le contrat. Les systèmes d'animation s'ordonnent `.after(CullingSet::Lod)`.
///
/// ```ignore
/// fn blink(mut signs: Query<(&SimulationLod, &mut Sign)>) {
///     for (lod, mut sign) in &mut signs {
///         if lod.due() {
///             sign.phase += lod.step_s() * sign.rate;
///         }
///     }
/// }
/// ```
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SimulationLod {
    /// Distances de découpe de la cadence pour cet objet.
    pub bands: LodBands,
    cadence: Cadence,
    /// Décalage du tour de l'objet, en images.
    phase: u32,
    /// `phase` a été tirée de l'identifiant de l'entité.
    seeded: bool,
    due: bool,
    /// Temps accumulé depuis le dernier tour, en s.
    pending_s: f32,
    /// Temps à intégrer à ce tour, en s (0 si ce n'est pas son tour).
    step_s: f32,
}

impl Default for SimulationLod {
    fn default() -> Self {
        Self::new(LodBands::default())
    }
}

impl SimulationLod {
    /// Un objet qui démarre à pleine cadence, avec les bornes `bands`.
    #[must_use]
    pub const fn new(bands: LodBands) -> Self {
        Self {
            bands,
            cadence: Cadence::Full,
            phase: 0,
            seeded: false,
            due: true,
            pending_s: 0.0,
            step_s: 0.0,
        }
    }

    /// Cadence retenue à la dernière image.
    #[must_use]
    pub const fn cadence(&self) -> Cadence {
        self.cadence
    }

    /// Vrai si l'animation de l'objet doit avancer à cette image.
    #[must_use]
    pub const fn due(&self) -> bool {
        self.due
    }

    /// Temps (s) à intégrer à ce tour : celui écoulé depuis le tour précédent. Nul quand [`SimulationLod::due`] est
    /// faux.
    #[must_use]
    pub const fn step_s(&self) -> f32 {
        self.step_s
    }

    /// Avance d'une image : fixe la cadence puis décide si c'est le tour de l'objet.
    ///
    /// `frame` est le numéro d'image, `dt_s` sa durée. Pure : c'est ce que testent les tests unitaires.
    pub fn advance(&mut self, cadence: Cadence, frame: u32, dt_s: f32) {
        self.cadence = cadence;
        let Some(period) = cadence.period() else {
            // Gelé : rien n'est rattrapé à la reprise.
            self.due = false;
            self.step_s = 0.0;
            self.pending_s = 0.0;
            return;
        };
        self.pending_s += dt_s.max(0.0);
        if frame.wrapping_add(self.phase).is_multiple_of(period) {
            self.due = true;
            self.step_s = self.pending_s;
            self.pending_s = 0.0;
        } else {
            self.due = false;
            self.step_s = 0.0;
        }
    }

    fn seed(&mut self, entity: Entity) {
        if !self.seeded {
            // 3 bits suffisent : la plus longue période est 8.
            self.phase = (splitmix64(entity.to_bits()) & 0x7) as u32;
            self.seeded = true;
        }
    }
}

/// Numéro d'image du culling de simulation (compté ici : indépendant de `FrameCount`, qui n'existe pas dans toutes
/// les `App` de test).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LodClock {
    /// Images écoulées depuis le démarrage.
    pub frame: u32,
}

/// `CullingSet::Lod` : fixe la cadence de chaque [`SimulationLod`].
///
/// Sans caméra de culling, tout reste à pleine cadence (on ne sait pas ce qui est loin) ; réglage
/// [`CullingSettings::simulation_lod`] à `false` : idem, pour comparer avec et sans.
pub(crate) fn update_simulation_lod(
    time: Res<Time>,
    settings: Res<CullingSettings>,
    mut clock: ResMut<LodClock>,
    viewers: Query<&GlobalTransform, With<CullingView>>,
    mut objects: Query<(Entity, &mut SimulationLod, &ViewVisibility, &GlobalTransform)>,
) {
    clock.frame = clock.frame.wrapping_add(1);
    let frame = clock.frame;
    let dt_s = time.delta_secs();
    let eyes_count = viewers.iter().count();
    let enabled = settings.simulation_lod && eyes_count > 0;
    for (entity, mut lod, visibility, transform) in &mut objects {
        lod.seed(entity);
        let cadence = if enabled {
            let position = transform.translation();
            let distance = viewers
                .iter()
                .map(|eye| eye.translation().distance(position))
                .fold(f32::INFINITY, f32::min);
            Cadence::select(&lod.bands, visibility.get(), distance)
        } else {
            Cadence::Full
        };
        lod.advance(cadence, frame, dt_s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands() -> LodBands {
        LodBands {
            near_m: 50.0,
            far_m: 200.0,
            freeze_m: 120.0,
        }
    }

    #[test]
    fn la_cadence_suit_la_visibilite_et_la_distance() {
        let b = bands();
        let cases = [
            (true, 0.0, Cadence::Full),
            (true, 49.9, Cadence::Full),
            (true, 50.0, Cadence::Half),
            (true, 199.9, Cadence::Half),
            (true, 200.0, Cadence::Quarter),
            (true, 5_000.0, Cadence::Quarter),
            (false, 0.0, Cadence::Eighth),
            (false, 119.9, Cadence::Eighth),
            (false, 120.0, Cadence::Frozen),
            (true, f32::NAN, Cadence::Quarter),
            (false, f32::NAN, Cadence::Frozen),
        ];
        for (visible, distance, expected) in cases {
            assert_eq!(Cadence::select(&b, visible, distance), expected, "{visible} {distance}");
        }
    }

    #[test]
    fn un_objet_au_quart_de_cadence_a_un_tour_sur_quatre_et_integre_le_temps_cumule() {
        let mut lod = SimulationLod::new(bands());
        let dt = 1.0 / 60.0;
        let mut turns = 0;
        let mut integrated = 0.0;
        for frame in 0..240 {
            lod.advance(Cadence::Quarter, frame, dt);
            if lod.due() {
                turns += 1;
                integrated += lod.step_s();
            } else {
                assert_eq!(lod.step_s(), 0.0);
            }
        }
        assert_eq!(turns, 60);
        // Le temps intégré est celui de 240 images, moins les 0 à 3 dernières en attente du prochain tour.
        assert!((integrated - 4.0).abs() <= 3.0 * dt + 1e-4, "{integrated}");
    }

    #[test]
    fn l_animation_ne_ralentit_pas_quand_la_cadence_baisse() {
        // Deux objets animés à 1 unité par seconde : l'un chaque image, l'autre une sur huit.
        let (mut full, mut eighth) = (SimulationLod::new(bands()), SimulationLod::new(bands()));
        let (mut a, mut b) = (0.0_f32, 0.0_f32);
        let dt = 1.0 / 60.0;
        for frame in 0..480 {
            full.advance(Cadence::Full, frame, dt);
            eighth.advance(Cadence::Eighth, frame, dt);
            if full.due() {
                a += full.step_s();
            }
            if eighth.due() {
                b += eighth.step_s();
            }
        }
        // 480 images = 8 s ; le retard du lent est d'au plus sept images.
        assert!((a - 8.0).abs() < 1e-3, "{a}");
        assert!((a - b).abs() <= 7.0 * dt + 1e-3, "{a} / {b}");
    }

    #[test]
    fn un_objet_gele_ne_rattrape_rien_a_la_reprise() {
        let mut lod = SimulationLod::new(bands());
        let dt = 1.0 / 60.0;
        for frame in 0..600 {
            lod.advance(Cadence::Frozen, frame, dt);
            assert!(!lod.due());
            assert_eq!(lod.step_s(), 0.0);
        }
        lod.advance(Cadence::Full, 600, dt);
        assert!(lod.due());
        assert!((lod.step_s() - dt).abs() < 1e-6, "{}", lod.step_s());
    }

    #[test]
    fn les_phases_etalent_les_tours() {
        // Mille objets à un tour sur huit : à chaque image, environ un huitième d'entre eux sont de tour.
        let mut objects: Vec<SimulationLod> = (0..1_000).map(|_| SimulationLod::new(bands())).collect();
        for (i, lod) in objects.iter_mut().enumerate() {
            lod.seed(Entity::from_raw_u32(i as u32 + 1).expect("index valide"));
        }
        for frame in 0..16 {
            let due = objects
                .iter_mut()
                .map(|lod| {
                    lod.advance(Cadence::Eighth, frame, 0.016);
                    usize::from(lod.due())
                })
                .sum::<usize>();
            assert!(
                (60..190).contains(&due),
                "image {frame} : {due} objets de tour sur 1000"
            );
        }
    }

    #[test]
    fn un_dt_negatif_ou_nul_n_enleve_pas_de_temps() {
        let mut lod = SimulationLod::new(bands());
        lod.advance(Cadence::Full, 0, -1.0);
        assert_eq!(lod.step_s(), 0.0);
        assert!(lod.due());
    }
}
