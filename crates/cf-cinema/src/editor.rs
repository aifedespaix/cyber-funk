//! La mémoire éditoriale du réalisateur (`cinema/editor.ts`) : ce qui a été montré, quand, et ce qui peut venir.
//!
//! **Provisoire (J6-01).** Ce module ne porte que le minimum dont le cœur a besoin pour tourner : un tirage pondéré
//! parmi les plans disponibles, dans leur plage d'énergie et hors cooldown, et la meilleure occasion. Le vrai éditeur
//! de Sillage (rangs, garde-fous de rareté, bonus build/break/drop, plafond cockpit, respiration, fraîcheur commune)
//! arrive avec J6-02, derrière la même interface.

use crate::rng::CinemaRng;
use crate::screen_time::ScreenTime;
use crate::shot::ShotSpec;
use crate::world::CinemaFrame;

/// Au-delà de cette occasion (0 → 1), le réalisateur bascule dès le prochain temps.
pub const OPPORTUNITY_THRESHOLD: f32 = 0.7;

/// Pourquoi on tire un plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickMode {
    /// Premier plan au démarrage.
    Start,
    /// Coupe de routine (grille, phrase, durée max).
    Normal,
    /// Le drop part.
    Drop,
    /// Le plan précédent s'est terminé de lui-même.
    Ended,
}

/// La mémoire éditoriale. Les plans sont désignés par leur rang dans le catalogue.
#[derive(Debug, Clone)]
pub struct Editor {
    /// Dernière montée (horloge du monde), pour les bonus de J6-02.
    pub build_at: f64,
    /// Dernier break (horloge du monde), pour les bonus de J6-02.
    pub break_at: f64,
    /// Le dernier plan montré n'était pas un plan voiture : le suivant y revient.
    pub after_special: bool,
    last_used: Vec<f64>,
    pool: Vec<(usize, f32)>,
    screen: ScreenTime,
    calm: bool,
    rng: CinemaRng,
}

impl Editor {
    /// Éditeur d'un catalogue de `shots` plans.
    #[must_use]
    pub fn new(shots: usize, calm: bool, rng: CinemaRng) -> Self {
        Self {
            build_at: f64::NEG_INFINITY,
            break_at: f64::NEG_INFINITY,
            after_special: false,
            last_used: vec![f64::NEG_INFINITY; shots],
            pool: Vec::with_capacity(shots),
            screen: ScreenTime::new(shots),
            calm,
            rng,
        }
    }

    /// Oublie l'historique (démarrage du réalisateur).
    pub fn reset(&mut self, _now: f64) {
        self.build_at = f64::NEG_INFINITY;
        self.break_at = f64::NEG_INFINITY;
        self.after_special = false;
        self.last_used.fill(f64::NEG_INFINITY);
        self.screen.reset();
    }

    /// Mode calme (moins de coupes) : lu par J6-02.
    #[must_use]
    pub fn calm(&self) -> bool {
        self.calm
    }

    /// Part d'écran du plan `index`, plan en cours compris.
    #[must_use]
    pub fn share(&self, index: usize, now: f64) -> f64 {
        self.screen.share(index, now)
    }

    /// Tire le plan suivant, hors le plan courant ; `None` si aucun plan n'est jouable.
    ///
    /// Trois passes, de la plus exigeante à la plus permissive : disponible + énergie + cooldown ; disponible seul ;
    /// enfin le plan courant lui-même (un catalogue d'un seul plan relance ce plan, avec une nouvelle variante).
    pub fn pick(
        &mut self,
        frame: &CinemaFrame<'_>,
        catalog: &[Box<dyn ShotSpec>],
        _mode: PickMode,
        current: Option<usize>,
    ) -> Option<usize> {
        for strict in [true, false] {
            self.pool.clear();
            let mut sum = 0.0;
            for (index, spec) in catalog.iter().enumerate() {
                if Some(index) == current || !spec.available(frame) {
                    continue;
                }
                if strict && !(self.in_energy(frame, spec.as_ref()) && self.rested(frame, index, spec.as_ref())) {
                    continue;
                }
                let weight = spec.weight().max(0.0);
                self.pool.push((index, weight));
                sum += weight;
            }
            if !self.pool.is_empty() {
                let mut roll = self.rng.next_f32() * sum;
                for &(index, weight) in &self.pool {
                    roll -= weight;
                    if roll <= 0.0 {
                        return Some(index);
                    }
                }
                return self.pool.last().map(|&(index, _)| index);
            }
        }
        current.filter(|&index| catalog.get(index).is_some_and(|spec| spec.available(frame)))
    }

    /// La meilleure occasion à saisir (au-delà de [`OPPORTUNITY_THRESHOLD`]), hors le plan courant. `reactive` : seuls
    /// les plans qui jouent une animation de la voiture.
    #[must_use]
    pub fn best_opportunity(
        &self,
        frame: &CinemaFrame<'_>,
        catalog: &[Box<dyn ShotSpec>],
        current: Option<usize>,
        reactive: bool,
    ) -> Option<usize> {
        let mut best = None;
        let mut best_score = OPPORTUNITY_THRESHOLD;
        for (index, spec) in catalog.iter().enumerate() {
            if Some(index) == current || (reactive && !spec.reactive()) || !spec.available(frame) {
                continue;
            }
            if !self.rested(frame, index, spec.as_ref()) {
                continue;
            }
            let score = spec.opportunity(frame);
            if score > best_score {
                best = Some(index);
                best_score = score;
            }
        }
        best
    }

    /// Un plan vient d'être lancé.
    pub fn register(&mut self, index: usize, spec: &dyn ShotSpec, frame: &CinemaFrame<'_>) {
        if let Some(at) = self.last_used.get_mut(index) {
            *at = frame.time;
        }
        self.after_special = !spec.follows_car();
        self.screen.switch_to(index, frame.time);
    }

    fn in_energy(&self, frame: &CinemaFrame<'_>, spec: &dyn ShotSpec) -> bool {
        let (lo, hi) = spec.energy();
        (lo..=hi).contains(&frame.intensity)
    }

    fn rested(&self, frame: &CinemaFrame<'_>, index: usize, spec: &dyn ShotSpec) -> bool {
        self.last_used
            .get(index)
            .is_none_or(|&at| frame.time - at >= f64::from(spec.cooldown()))
    }
}
