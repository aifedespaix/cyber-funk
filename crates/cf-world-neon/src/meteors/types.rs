//! Les types de la logique des astres : un astre du pool, un anneau de brume, les événements et les entrées d'une image.
//! Voir [`super::field`] pour la logique.

use bevy::math::{DVec3, Vec3};

/// Assez de places pour une pluie (12 astres au plus) sans refuser un lancer de la caméra.
pub const POOL: usize = 12;
/// Durée de vie de l'anneau de brume (s).
pub const SPLASH_LIFE: f32 = 1.8;
/// Hauteur de la nappe de brouillard (m, `MIST_Y` de `building-material.ts`) : un astre qui l'atteint s'y éteint.
pub const MIST_Y: f64 = -50.0;
/// Nombre de teintes de chaleur : ambre, corail, glace, menthe, azur.
pub const HEAT_COUNT: usize = 5;

/// Un astre du pool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meteor {
    /// Numéro de lancement, unique pour la durée de la scène : un plan retrouve son astre par là.
    pub id: u64,
    /// Position absolue (m).
    pub position: DVec3,
    /// Vitesse (m/s) : constante, un astre ne subit aucune gravité.
    pub velocity: Vec3,
    /// Indice de teinte de chaleur (0..[`HEAT_COUNT`]).
    pub tint: usize,
    /// Rayon (m).
    pub radius: f32,
    /// Vitesse de rotation sur lui-même (rad/s).
    pub spin_rad_s: f32,
    /// Angle de rotation courant, autour de y puis de x (rad).
    pub angle_rad: (f32, f32),
    /// Graine du bosselage (`uSeed`).
    pub seed: f32,
    /// Indice de la texture de planète qui l'habille.
    pub texture: usize,
    /// En vol.
    pub active: bool,
    /// Âge (s) : sert à faire monter sa lueur sans à-coup.
    pub age_s: f32,
}

impl Meteor {
    pub(super) const FREE: Self = Self {
        id: u64::MAX,
        position: DVec3::ZERO,
        velocity: Vec3::NEG_Y,
        tint: 0,
        radius: 1.0,
        spin_rad_s: 0.0,
        angle_rad: (0.0, 0.0),
        seed: 0.0,
        texture: 0,
        active: false,
        age_s: 0.0,
    };

    /// Direction du vol, unitaire.
    #[must_use]
    pub fn heading(&self) -> Vec3 {
        self.velocity.normalize_or(Vec3::NEG_Y)
    }
}

/// L'anneau que laisse un astre éteint dans la nappe de brouillard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Splash {
    /// Centre (absolu, m), à `MIST_Y + 0,3`.
    pub position: DVec3,
    /// Rayon de l'astre éteint (m) : la taille de l'anneau en découle.
    pub radius: f32,
    /// Indice de teinte de chaleur.
    pub tint: usize,
    /// Âge (s) : libre à partir de [`SPLASH_LIFE`].
    pub age_s: f32,
    /// Le limiteur de flashs a accordé à cet anneau son éclat de sol.
    pub flash_granted: bool,
}

impl Splash {
    pub(super) const FREE: Self = Self {
        position: DVec3::ZERO,
        radius: 1.0,
        tint: 0,
        age_s: SPLASH_LIFE,
        flash_granted: false,
    };

    /// L'anneau est libre (invisible, prêt à servir).
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.age_s >= SPLASH_LIFE
    }

    /// Avancement de la vie, 0 → 1.
    #[must_use]
    pub fn t(&self) -> f32 {
        (self.age_s / SPLASH_LIFE).clamp(0.0, 1.0)
    }
}

/// Ce qui vient d'arriver à un astre, pour le rendu et le limiteur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldEvent {
    /// Un astre s'est écrasé contre un immeuble.
    Blast {
        /// Point d'impact (absolu, m).
        at: DVec3,
        /// Rayon de l'astre (m).
        radius: f32,
        /// Indice de teinte de chaleur.
        tint: usize,
        /// Identifiant de l'astre.
        meteor_id: u64,
    },
    /// Un astre s'est éteint dans la brume : l'anneau `slot` du pool vient de naître.
    Splash {
        /// Emplacement de l'anneau dans [`MeteorField::splashes`].
        slot: usize,
    },
}

/// Le dernier astre écrasé contre un immeuble (`lastBlast` de Sillage). `seq` change à chaque explosion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlastRecord {
    /// Point d'impact (absolu, m).
    pub at: DVec3,
    /// Rayon de l'astre (m).
    pub radius: f32,
    /// Identifiant de l'astre.
    pub meteor_id: u64,
    /// Compteur d'explosions.
    pub seq: u32,
    /// Horloge de la scène à l'impact (s).
    pub time_s: f64,
}

/// Ce que le spectacle de l'image permet (`ShowState` de Sillage, ce que les astres en lisent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShowGate {
    /// `gates.meteorRate` : lancers par minute (0 : aucun).
    pub meteor_rate: f32,
    /// `gates.meteorShower` : une pluie peut partir sur un drop.
    pub meteor_shower: bool,
    /// Mode respiration : cadence réduite, pas de pluie.
    pub breathing: bool,
    /// Le drop frappe dans cette image.
    pub drop_now: bool,
}

/// Les entrées d'une image.
#[derive(Debug, Clone, Copy)]
pub struct FieldInput {
    /// Pas de l'image (s).
    pub dt_s: f32,
    /// Horloge de la scène (s).
    pub now_s: f64,
    /// La voiture (absolue, m) : les astres tombent autour d'elle.
    pub anchor: DVec3,
    /// Mode calme (`reducedMotion`) : astres plus lents et plus rares.
    pub calm: bool,
    /// Le spectacle de l'image, ou `None` hors spectacle (cadence libre de Sillage).
    pub show: Option<ShowGate>,
    /// Nombre de textures de planètes disponibles : sans elles, rien ne tombe.
    pub textures: usize,
}
