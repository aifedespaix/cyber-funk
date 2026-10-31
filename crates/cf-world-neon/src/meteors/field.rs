//! La logique des astres qui tombent (`Meteors` de `meteors.ts`) : pool de [`POOL`] astres, lancers (cadence du
//! spectacle, pluies, ordre du réalisateur), vol par petits pas contre le décor, fin dans une explosion ou dans la brume.
//!
//! Pure et déterministe : aucune entité, aucune dépendance au rendu. Le système Bevy ([`super::systems`]) fournit le
//! décor (`collide`) et lit le résultat ; le rendu ([`super::rig`]) n'en fait que des transformations.
//!
//! Écarts de portage assumés :
//!
//! - `Math.random()` (déclenchement et taille des pluies) devient un `Pcg32` à graine injectée ;
//! - l'anneau de brume (`splash`) a **son propre pool** au lieu d'appartenir à l'astre : relancer un astre sur le même
//!   emplacement ne coupe plus net un anneau qui s'étale encore (une coupure sèche serait un saut de luminance) ;
//! - l'explosion n'est plus lancée ici mais publiée en [`FieldEvent::Blast`] : elle est rendue par [`super::blast`].

use bevy::math::DVec3;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use super::types::{BlastRecord, FieldEvent, FieldInput, HEAT_COUNT, MIST_Y, Meteor, POOL, ShowGate, Splash};
use crate::rng::hash;

/// Pluie d'astres (spectacle) : nombre minimal d'astres.
pub const SHOWER_MIN: u32 = 6;
/// Pluie d'astres : nombre maximal d'astres.
pub const SHOWER_MAX: u32 = 12;
/// Durée minimale d'une pluie (s, soit 2 à 3 mesures).
const SHOWER_SPAN_MIN: f32 = 4.0;
/// Durée maximale d'une pluie (s).
const SHOWER_SPAN_MAX: f32 = 6.5;
/// Pause minimale entre deux pluies (s).
const SHOWER_COOLDOWN: f32 = 100.0;
/// Part des drops qui déclenchent une pluie quand elle est permise.
const SHOWER_CHANCE: f64 = 0.4;
/// Un astre ne parcourt jamais plus que ça entre deux tests de collision (m) : il ne traverse pas un mur mince.
const MAX_STEP: f64 = 3.0;
/// Un astre plus loin que ça derrière la voiture est recyclé (m).
const RECYCLE_BEHIND_M: f64 = 420.0;

/// Le pool d'astres, leurs anneaux de brume et le minuteur des lancers.
#[derive(Debug, Clone)]
pub struct MeteorField {
    meteors: [Meteor; POOL],
    splashes: [Splash; POOL],
    countdown_s: f32,
    spawned: u64,
    /// Pluie en cours : astres restant à lancer, durée entre deux lancers, temps avant le prochain, pause avant la suivante.
    shower_left: u32,
    shower_gap_s: f32,
    shower_next_s: f32,
    shower_rest_s: f32,
    rng: Pcg32,
    last_blast: Option<BlastRecord>,
    blasts: u32,
    /// Ce qui s'est passé pendant la dernière image (vidé au début de [`MeteorField::update`]). Capacité réservée :
    /// au plus un événement par astre et par image.
    events: Vec<FieldEvent>,
}

impl Default for MeteorField {
    fn default() -> Self {
        Self::new(0x4D45_5445_4F52)
    }
}

impl MeteorField {
    /// Un pool vide ; `seed` est celle des tirages de pluie.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            meteors: [Meteor::FREE; POOL],
            splashes: [Splash::FREE; POOL],
            countdown_s: 2.0,
            spawned: 0,
            shower_left: 0,
            shower_gap_s: 0.0,
            shower_next_s: 0.0,
            shower_rest_s: 30.0,
            rng: Pcg32::seed_from_u64(seed),
            last_blast: None,
            blasts: 0,
            events: Vec::with_capacity(POOL * 2),
        }
    }

    /// Les emplacements du pool, en vol ou libres.
    #[must_use]
    pub fn meteors(&self) -> &[Meteor; POOL] {
        &self.meteors
    }

    /// Les anneaux de brume, libres ou en vie.
    #[must_use]
    pub fn splashes(&self) -> &[Splash; POOL] {
        &self.splashes
    }

    /// Les anneaux de brume, en écriture (le limiteur y marque l'éclat accordé).
    pub(crate) fn splashes_mut(&mut self) -> &mut [Splash; POOL] {
        &mut self.splashes
    }

    /// Ce qui s'est passé pendant la dernière image.
    #[must_use]
    pub fn events(&self) -> &[FieldEvent] {
        &self.events
    }

    /// Le dernier astre écrasé contre un immeuble (`lastBlast`).
    #[must_use]
    pub fn last_blast(&self) -> Option<BlastRecord> {
        self.last_blast
    }

    /// Les astres en vol.
    pub fn active(&self) -> impl Iterator<Item = &Meteor> + '_ {
        self.meteors.iter().filter(|m| m.active)
    }

    /// Vrai pendant qu'une pluie d'astres tombe.
    #[must_use]
    pub fn showering(&self) -> bool {
        self.shower_left > 0
    }

    /// Nombre d'astres lancés depuis le début.
    #[must_use]
    pub fn spawned(&self) -> u64 {
        self.spawned
    }

    fn unit(&mut self) -> f64 {
        f64::from(self.rng.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }

    /// Fait tomber un astre maintenant, autour de la voiture située en `anchor`. Renvoie son identifiant, ou `None`
    /// s'il n'y a pas de place (ou pas de texture).
    pub fn launch(&mut self, anchor: DVec3, calm: bool, textures: usize) -> Option<u64> {
        if textures == 0 {
            return None;
        }
        let slot = self.meteors.iter().position(|m| !m.active)?;
        let k = self.spawned;
        self.spawned += 1;
        let h = |salt: f64| hash(k as f64, salt);

        // Départ : haut dans le ciel et en arrière. Arrivée : loin devant, dans la brume, plutôt de l'autre côté.
        let side = if h(1.0) < 0.5 { -1.0 } else { 1.0 };
        let position = DVec3::new(
            anchor.x + side * (10.0 + h(2.0) * 90.0),
            105.0 + h(3.0) * 80.0,
            anchor.z + 25.0 + h(4.0) * 80.0,
        );
        let end = DVec3::new(
            anchor.x - side * (20.0 + h(5.0) * 130.0),
            MIST_Y,
            anchor.z - (240.0 + h(6.0) * 240.0),
        );
        let speed = (60.0 + h(7.0) * 70.0) * if calm { 0.45 } else { 1.0 };
        let velocity = ((end - position).normalize() * speed).as_vec3();

        self.meteors[slot] = Meteor {
            id: k,
            position,
            velocity,
            // Surtout des cailloux, parfois un gros bloc.
            radius: (0.7 + h(8.0).powf(2.4) * 6.5) as f32,
            spin_rad_s: ((h(9.0) - 0.5) * 2.4) as f32,
            angle_rad: (0.0, 0.0),
            tint: ((h(10.0) * HEAT_COUNT as f64) as usize).min(HEAT_COUNT - 1),
            seed: (h(11.0) * 40.0) as f32,
            texture: (k % textures as u64) as usize,
            active: true,
            age_s: 0.0,
        };
        Some(k)
    }

    /// Lance une pluie de `count` astres sur `seconds` (la caméra et les tests peuvent la réclamer).
    pub fn start_shower(&mut self, count: u32, seconds: f32) {
        self.shower_left = count.min(SHOWER_MAX);
        self.shower_gap_s = seconds / self.shower_left.max(1) as f32;
        self.shower_next_s = 0.0;
    }

    /// Avance d'une image : cadence des lancers, vol, explosions et anneaux. `collide` dit si une sphère (centre absolu,
    /// rayon) touche un immeuble.
    pub fn update(&mut self, input: &FieldInput, collide: &mut dyn FnMut(DVec3, f64) -> bool) {
        self.events.clear();
        let dt = input.dt_s.max(0.0);
        if let Some(show) = input.show {
            self.step_show(input, show);
        } else {
            self.countdown_s -= dt;
            if self.countdown_s <= 0.0 {
                self.launch(input.anchor, input.calm, input.textures);
                let (base, span) = if input.calm { (8.0, 8.0) } else { (2.2, 4.0) };
                self.countdown_s = base + hash(self.spawned as f64, 12.0) as f32 * span;
            }
        }

        for splash in &mut self.splashes {
            if !splash.is_free() {
                splash.age_s += dt;
                if splash.is_free() {
                    splash.flash_granted = false;
                }
            }
        }
        for slot in 0..POOL {
            if !self.meteors[slot].active {
                continue;
            }
            self.meteors[slot].age_s += dt;
            if self.advance(slot, dt, input.now_s, collide) {
                continue;
            }
            // Trop loin derrière la voiture (cas du pool saturé par un astre lent) : on le recycle.
            if self.meteors[slot].position.z > input.anchor.z + RECYCLE_BEHIND_M {
                self.meteors[slot].active = false;
                continue;
            }
            let m = &mut self.meteors[slot];
            m.angle_rad.0 += m.spin_rad_s * dt;
            m.angle_rad.1 += m.spin_rad_s * 0.4 * dt;
        }
    }

    /// Cadence du spectacle : `meteor_rate` lancers par minute (0 : aucun), moins de la moitié en respiration. Une pluie
    /// part parfois sur un drop quand `meteor_shower` le permet ; ses astres sont étalés, un lancer refusé (pool plein) est
    /// retenté.
    fn step_show(&mut self, input: &FieldInput, show: ShowGate) {
        let dt = input.dt_s.max(0.0);
        let rate = show.meteor_rate * if show.breathing { 0.4 } else { 1.0 };
        if rate > 0.0 {
            self.countdown_s -= dt;
            // Si la cadence monte, on n'attend pas l'ancien délai.
            let longest = (60.0 / rate) * 1.4;
            self.countdown_s = self.countdown_s.min(longest);
            if self.countdown_s <= 0.0 {
                self.launch(input.anchor, input.calm, input.textures);
                self.countdown_s = (60.0 / rate) * (0.6 + hash(self.spawned as f64, 12.0) as f32 * 0.8);
            }
        } else {
            self.countdown_s = 4.0;
        }

        self.shower_rest_s -= dt;
        if self.shower_left == 0
            && show.meteor_shower
            && !show.breathing
            && !input.calm
            && self.shower_rest_s <= 0.0
            && show.drop_now
            && self.unit() < SHOWER_CHANCE
        {
            let count = SHOWER_MIN + (self.unit() * f64::from(SHOWER_MAX - SHOWER_MIN + 1)) as u32;
            let span = SHOWER_SPAN_MIN + self.unit() as f32 * (SHOWER_SPAN_MAX - SHOWER_SPAN_MIN);
            self.start_shower(count.min(SHOWER_MAX), span);
            self.shower_rest_s = SHOWER_COOLDOWN;
        }
        if self.shower_left > 0 {
            self.shower_next_s -= dt;
            if self.shower_next_s <= 0.0 {
                if self.launch(input.anchor, input.calm, input.textures).is_some() {
                    self.shower_left -= 1;
                    self.shower_next_s = self.shower_gap_s * (0.6 + self.unit() as f32 * 0.8);
                } else {
                    self.shower_next_s = 0.3;
                }
            }
        }
    }

    /// Avance par petits pas, en testant le décor à chacun. Renvoie vrai si l'astre vient de finir (explosion ou brume).
    fn advance(&mut self, slot: usize, dt: f32, now_s: f64, collide: &mut dyn FnMut(DVec3, f64) -> bool) -> bool {
        let m = self.meteors[slot];
        let distance = f64::from(m.velocity.length() * dt);
        let steps = ((distance / MAX_STEP).ceil() as u32).max(1);
        let step = m.velocity.as_dvec3() * (f64::from(dt) / f64::from(steps));
        let mut position = m.position;
        for _ in 0..steps {
            position += step;
            if position.y <= MIST_Y + f64::from(m.radius) * 0.4 {
                self.meteors[slot].position = position;
                self.impact(slot);
                return true;
            }
            if collide(position, f64::from(m.radius)) {
                self.meteors[slot].position = position;
                self.explode(slot, now_s);
                return true;
            }
        }
        self.meteors[slot].position = position;
        false
    }

    fn explode(&mut self, slot: usize, now_s: f64) {
        let m = &mut self.meteors[slot];
        // Recule d'un demi-rayon le long de la trajectoire : l'explosion naît sur le mur, pas dedans.
        let at = m.position - (m.heading() * (m.radius * 0.5)).as_dvec3();
        m.position = at;
        m.active = false;
        let (radius, tint, id) = (m.radius, m.tint, m.id);
        self.blasts = self.blasts.wrapping_add(1);
        self.last_blast = Some(BlastRecord {
            at,
            radius,
            meteor_id: id,
            seq: self.blasts,
            time_s: now_s,
        });
        self.events.push(FieldEvent::Blast {
            at,
            radius,
            tint,
            meteor_id: id,
        });
    }

    fn impact(&mut self, slot: usize) {
        let m = &mut self.meteors[slot];
        m.active = false;
        let (x, z, radius, tint) = (m.position.x, m.position.z, m.radius, m.tint);
        // Un anneau libre, sinon le plus avancé dans sa vie : il est presque éteint.
        let target = self.splashes.iter().position(Splash::is_free).unwrap_or_else(|| {
            self.splashes
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.age_s.total_cmp(&b.1.age_s))
                .map_or(0, |(i, _)| i)
        });
        self.splashes[target] = Splash {
            position: DVec3::new(x, MIST_Y + 0.3, z),
            radius,
            tint,
            age_s: 0.0,
            flash_granted: false,
        };
        self.events.push(FieldEvent::Splash { slot: target });
    }
}

#[cfg(test)]
#[path = "field_tests.rs"]
mod tests;
