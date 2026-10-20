//! Chef d'orchestre du drop (portage de `show/drop.ts`, `show/build.ts` et `show/drop-titles.ts` de Sillage).
//!
//! Le drop est *préparé* pendant la montée (plan caméra, cascade, paquet d'effets), puis tout frappe sur la même
//! image : celle du drop réel de la musique, ou d'un drop forcé par l'opérateur. C'est ce qui donne l'effet « l'image
//! sait ce qui arrive ». Les systèmes lisent [`DropConductor::plan`] pendant la montée pour se préparer, et frappent
//! tous sur l'image où [`DropConductor::drop_now`] est vrai ; le plan reste lisible cette image-là, puis disparaît.
//!
//! Sans Bevy, sans horloge : le temps (`now`, `dt`) est un paramètre, tout aléatoire passe par le RNG injecté (même
//! graine, même suite de drops). La partie qui *lit* le plan (choix du plan caméra au drop, plans de préparation de
//! `cinema/choreo.ts`) appartient au réalisateur (J6) ; les identifiants de plans et de cascades restent des chaînes
//! typées ([`ShotId`], [`StuntKind`]), validées plus tard contre les catalogues réels (J6, J7).
//!
//! Sûreté : le paquet d'effets ne porte aucun flash quand les flashs sont coupés, y compris s'ils sont coupés entre la
//! préparation et la frappe. Les flashs demandés restent ensuite plafonnés par le limiteur photosensible
//! ([`crate::limiter`]), qui passe toujours en dernier.

mod build;
mod choose;
mod titles;

use std::sync::Arc;

use rand_core::Rng;

pub use build::BuildDetector;
pub use titles::{DropTitle, DropTitles, MAX_TITLE_LEN, TitleLanguage};

use crate::freshness::Freshness;
use crate::model::{ShotId, ShowGates, StuntKind};

/// Niveau d'intensité du look, de 0 (pré-intro) à [`MAX_LEVEL`] (cosmos) : `ShowLevel` de Sillage. Il règle le nombre
/// d'ondes de choc, la fréquence et le choix des titres. Le chef du spectacle (J2-05) le déduit du look courant.
pub type ShowLevel = u8;
/// Dernier niveau (`ShowLevel` 5 de Sillage).
pub const MAX_LEVEL: ShowLevel = 5;
/// Un drop préparé qui n'arrive pas dans ce délai (s) est oublié.
pub const PLAN_TTL: f32 = 30.0;

/// Ce qu'un drop va déclencher, décidé pendant la montée (`DropFxPackage` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropFxPackage {
    /// Flash blanc de l'écran, 0 → 1 (demande, plafonnée ensuite par le limiteur). **Toujours 0 quand les flashs sont
    /// coupés.**
    pub flash: f32,
    /// Décalage RVB, 0 → 1.
    pub rgb_split: f32,
    /// Pulsation de la ville, 0 → 1.
    pub city_pulse: f32,
    /// Ondes de choc à lancer, 1 à 3.
    pub shockwaves: u8,
    /// Gerbes d'étincelles à lancer, 20 à 60.
    pub burst: u16,
}

/// Le drop préparé (`DropPlan` de Sillage). Un seul est préparé à la fois.
#[derive(Debug, Clone, PartialEq)]
pub struct DropPlan {
    /// Cascade à jouer au drop (liste close vérifiée en J7).
    pub stunt: Option<StuntKind>,
    /// Plan caméra à couper sur le drop (catalogue vérifié en J6).
    pub shot: Option<ShotId>,
    /// Les effets.
    pub fx: DropFxPackage,
    /// Carton-titre à afficher au drop (rare), déjà en capitales et dans la langue demandée. `Arc` : le copier ne
    /// coûte aucune allocation.
    pub title: Option<Arc<str>>,
    /// Drop déclenché à la main par l'opérateur, sans montée.
    pub forced: bool,
}

impl DropPlan {
    fn blank() -> Self {
        Self {
            stunt: None,
            shot: None,
            fx: DropFxPackage {
                flash: 0.0,
                rgb_split: 0.0,
                city_pulse: 0.0,
                shockwaves: 1,
                burst: 20,
            },
            title: None,
            forced: false,
        }
    }
}

/// Ce que le chef d'orchestre doit savoir de l'image (`DropInput` de Sillage).
#[derive(Debug, Clone, Copy)]
pub struct DropInput<'a> {
    /// Durée de l'image, en secondes.
    pub dt: f32,
    /// Horloge du show (s) : pour la fraîcheur.
    pub now: f64,
    /// Intensité de la musique, 0 → 1 (`AudioFeatures::intensity`).
    pub intensity: f32,
    /// Un vrai drop vient de la musique (une seule image, voir `pace` de Sillage).
    pub real_drop: bool,
    /// L'opérateur force un drop.
    pub forced: bool,
    /// Portes du look courant.
    pub gates: &'a ShowGates,
    /// Chaleur du set, 0 → 1.
    pub heat: f32,
    /// Niveau d'intensité du look.
    pub level: ShowLevel,
    /// Flashs autorisés (réglage de l'opérateur, et jamais en mode respiration).
    pub flashes: bool,
    /// Langue des cartons-titres.
    pub language: TitleLanguage,
}

impl DropInput<'_> {
    /// Chaleur ramenée dans 0..=1 (une valeur non finie compte pour 0).
    fn heat(&self) -> f32 {
        if self.heat.is_finite() {
            self.heat.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Repère les montées, prépare le drop (cascade, plan caméra, effets) selon les portes du look et la fraîcheur, et
/// publie `drop_now` une seule image : au drop réel de la musique ou à un drop forcé par l'opérateur. Le plan reste
/// lisible cette image-là, puis disparaît ; un plan préparé que le drop ne rejoint pas s'efface après [`PLAN_TTL`] s.
///
/// Aucune allocation par image : le plan est un objet unique mis à jour sur place. Seule la préparation d'un drop
/// (une fois par montée) copie des identifiants.
#[derive(Debug, Clone)]
pub struct DropConductor<R> {
    rng: R,
    titles: DropTitles,
    detector: BuildDetector,
    plan: DropPlan,
    published: bool,
    drop_now: bool,
    /// Âge (s) du plan préparé.
    age_s: f32,
    /// Drops restants avant le prochain carton-titre.
    until_title: i32,
    /// Tampon réutilisé pour écrire les clés de fraîcheur (`shot:<id>`…) sans allouer.
    key: String,
}

impl<R: Rng> DropConductor<R> {
    /// Crée le chef d'orchestre. `rng` est le générateur du show (avec sa graine) ; `titles` les cartons-titres
    /// (vide : jamais de titre).
    pub fn new(rng: R, titles: DropTitles) -> Self {
        let mut conductor = Self {
            rng,
            titles,
            detector: BuildDetector::default(),
            plan: DropPlan::blank(),
            published: false,
            drop_now: false,
            age_s: 0.0,
            until_title: 0,
            // Un identifiant fait 64 caractères au plus, plus le préfixe : le tampon ne grossit jamais ensuite.
            key: String::with_capacity(80),
        };
        conductor.until_title = conductor.draw_gap(2);
        conductor
    }

    /// Déclare les plans et les titres du conductor à la mémoire de fraîcheur (zéro usage), pour que la moyenne se
    /// calcule sur tout le catalogue. À appeler une fois, à la création du chef du spectacle.
    pub fn register_known(&self, freshness: &mut Freshness) {
        choose::register_known(&self.titles, freshness);
    }

    /// Nouveau set : oublie la montée, le plan et le compte des titres. La fraîcheur est celle du chef du spectacle.
    pub fn reset(&mut self) {
        self.detector.reset();
        self.published = false;
        self.drop_now = false;
        self.age_s = 0.0;
        self.until_title = self.draw_gap(2);
    }

    /// Le plan à lire pendant la montée et l'image du drop, ou `None`.
    #[must_use]
    pub fn plan(&self) -> Option<&DropPlan> {
        self.published.then_some(&self.plan)
    }

    /// Vrai pendant une seule image : le drop frappe maintenant.
    #[must_use]
    pub fn drop_now(&self) -> bool {
        self.drop_now
    }

    /// Avance d'une image : repère la montée, prépare le drop, le fait frapper, oublie un plan périmé.
    pub fn update(&mut self, freshness: &mut Freshness, input: &DropInput<'_>) {
        if self.drop_now {
            self.drop_now = false;
            self.published = false;
        }
        let dt = if input.dt.is_finite() { input.dt.max(0.0) } else { 0.0 };
        let intensity = if input.intensity.is_finite() {
            input.intensity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let build = self.detector.update(intensity, dt);
        if self.published {
            self.age_s += dt;
        }
        if build && !self.published {
            self.prepare(freshness, input, false);
        }
        if self.published && self.age_s > PLAN_TTL {
            self.published = false;
        }
        if input.real_drop || input.forced {
            if !self.published {
                self.prepare(freshness, input, input.forced);
            } else if input.forced {
                self.plan.forced = true;
            }
            self.drop_now = true;
            self.plan.title = self.choose_title(freshness, input);
            self.commit(freshness, input.now);
        }
        // Les flashs ont pu être coupés depuis la préparation (réglage de l'opérateur, mode respiration) : le paquet
        // lu pendant la montée comme celui qui frappe n'en porte alors jamais.
        if !input.flashes {
            self.plan.fx.flash = 0.0;
        }
    }

    /// Le niveau vient de changer au moment du drop : le plan est refait avec les nouvelles portes (sans titre : le
    /// changement de niveau a déjà le sien). Sans effet s'il n'y a pas de plan.
    pub fn replan(&mut self, freshness: &Freshness, input: &DropInput<'_>) {
        if self.published {
            let forced = self.plan.forced;
            self.prepare(freshness, input, forced);
        }
    }

    fn prepare(&mut self, freshness: &Freshness, input: &DropInput<'_>, forced: bool) {
        let gates = input.gates;
        let heat = input.heat();
        self.plan.forced = forced;
        self.plan.stunt = self.choose_stunt(freshness, input);
        self.plan.shot = self.choose_shot(freshness, input, self.plan.stunt.is_some());
        let fx = &mut self.plan.fx;
        fx.flash = if input.flashes {
            sane_unit(gates.fx.flash * (0.6 + 0.4 * heat))
        } else {
            0.0
        };
        fx.rgb_split = sane_unit(gates.fx.rgb_split);
        fx.city_pulse = sane_unit(gates.fx.city_pulse);
        fx.shockwaves = 1 + u8::from(input.level >= 3) + u8::from(input.level >= MAX_LEVEL);
        fx.burst = (20.0 + 40.0 * heat).round() as u16;
        self.plan.title = None;
        self.age_s = 0.0;
        self.published = true;
    }

    /// Le drop frappe : ce qui a été choisi compte comme joué.
    fn commit(&mut self, freshness: &mut Freshness, now: f64) {
        if let Some(stunt) = &self.plan.stunt {
            freshness.mark(choose::key(&mut self.key, "stunt:", stunt.as_str()), now);
        }
        if let Some(shot) = &self.plan.shot {
            freshness.mark(choose::key(&mut self.key, "shot:", shot.as_str()), now);
        }
    }

    /// Réel uniforme dans `[0, 1)`, 53 bits.
    fn unit(&mut self) -> f64 {
        choose::unit(&mut self.rng)
    }
}

/// Ramène une valeur dans 0..=1 (une valeur non finie compte pour 0).
fn sane_unit(value: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 }
}
