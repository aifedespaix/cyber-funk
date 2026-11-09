//! Le pilotage à la main de l'onglet Debug : voie visée, ordres de vol, état de la voiture en retour.
//!
//! L'interface écrit [`ManualDrive`] ; le pilote (`cf-world-neon::pilot`) le lit, joue les ordres et publie en retour
//! l'état du vol ([`FlightStatus`]) pour que les boutons se grisent d'eux-mêmes (pas de figure au sol). Tant que le
//! pilotage manuel n'est pas engagé, le pilote automatique conduit comme avant : rien n'est sauvegardé, chaque session
//! repart en automatique.

use bevy::prelude::*;

/// Largeur d'un pas latéral : un bouton « Gauche » ou « Droite » déplace la voie visée de ce tiers de la route.
pub const LANE_STEP: f32 = 1.0 / 3.0;
/// Nombre d'ordres gardés en file : un opérateur qui martèle un bouton ne remplit pas la mémoire.
pub const MAX_ORDERS: usize = 8;

/// Un ordre de vol donné à la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualOrder {
    /// Décoller (déploie les roues, monte au premier étage).
    TakeOff,
    /// Se poser (descend, replie les roues).
    Land,
    /// Monter d'un étage.
    Up,
    /// Descendre d'un étage.
    Down,
    /// Tonneau à gauche.
    RollLeft,
    /// Tonneau à droite.
    RollRight,
    /// Vrille à gauche (rotation à plat, autour de la verticale).
    SpinLeft,
    /// Vrille à droite.
    SpinRight,
    /// Salto arrière (monte d'un étage).
    Backflip,
    /// Salto avant (descend d'un étage).
    Frontflip,
}

/// Ce que le pilote dit de la voiture, pour griser les boutons qui n'ont pas de sens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlightStatus {
    /// Les roues sont repliées et la voiture tient son étage : les étages et les figures sont permis.
    pub flying: bool,
    /// Elle n'est plus (tout à fait) au sol : déploiement, vol ou repli.
    pub airborne: bool,
    /// Une figure est en cours.
    pub tricking: bool,
    /// Étage visé, 0 = sol.
    pub floor: u8,
}

/// Le pilotage manuel.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct ManualDrive {
    /// Le pilotage manuel est engagé : le pilote automatique ne décide plus ni la voie ni le vol.
    engaged: bool,
    /// Voie visée, −1 (bord gauche) à 1 (bord droit).
    lane: f32,
    /// Ordres en attente, du plus ancien au plus récent.
    orders: Vec<ManualOrder>,
    /// État du vol, écrit par le pilote.
    status: FlightStatus,
    /// Compteur des demandes d'Ascension ; les cascades réagissent quand il monte.
    ascension_seq: u32,
}

impl ManualDrive {
    /// Le pilotage manuel est engagé.
    #[must_use]
    pub const fn engaged(&self) -> bool {
        self.engaged
    }

    /// Engage ou rend la main au pilote automatique. Rendre la main recentre la voie visée.
    pub fn set_engaged(&mut self, engaged: bool) {
        if !engaged {
            self.lane = 0.0;
            self.orders.clear();
        }
        self.engaged = engaged;
    }

    /// Voie visée, −1..=1.
    #[must_use]
    pub const fn lane(&self) -> f32 {
        self.lane
    }

    /// Décale la voie visée d'un pas vers la gauche (`−1`) ou la droite (`+1`), bornée à la route.
    pub fn nudge(&mut self, direction: i8) {
        let step = LANE_STEP * f32::from(direction.signum());
        self.lane = (self.lane + step).clamp(-1.0, 1.0);
    }

    /// Recentre la voiture.
    pub const fn centre(&mut self) {
        self.lane = 0.0;
    }

    /// Ajoute un ordre de vol. Ignoré hors pilotage manuel et quand la file est pleine.
    pub fn push(&mut self, order: ManualOrder) {
        if self.engaged && self.orders.len() < MAX_ORDERS {
            self.orders.push(order);
        }
    }

    /// Demande l'Ascension tout de suite, sans attendre un spectacle ni un temps fort (elle sert aussi en automatique).
    pub const fn request_ascension(&mut self) {
        self.ascension_seq = self.ascension_seq.wrapping_add(1);
    }

    /// Nombre de demandes d'Ascension depuis l'ouverture.
    #[must_use]
    pub const fn ascension_seq(&self) -> u32 {
        self.ascension_seq
    }

    /// Les ordres en attente, sans les consommer.
    #[must_use]
    pub fn pending(&self) -> &[ManualOrder] {
        &self.orders
    }

    /// Prend le premier ordre en attente. Le pilote le joue ; la file n'est écrite que s'il y en a un.
    pub fn pop(&mut self) -> Option<ManualOrder> {
        (!self.orders.is_empty()).then(|| self.orders.remove(0))
    }

    /// L'état du vol publié par le pilote.
    #[must_use]
    pub const fn status(&self) -> FlightStatus {
        self.status
    }

    /// Publie l'état du vol. Rend `true` s'il a changé (l'appelant ne réveille la ressource que dans ce cas).
    pub fn would_change(&self, status: FlightStatus) -> bool {
        self.status != status
    }

    /// Écrit l'état du vol.
    pub const fn set_status(&mut self, status: FlightStatus) {
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_voie_visee_reste_sur_la_route() {
        let mut drive = ManualDrive::default();
        for _ in 0..10 {
            drive.nudge(1);
        }
        assert!((drive.lane() - 1.0).abs() < f32::EPSILON);
        for _ in 0..10 {
            drive.nudge(-1);
        }
        assert!((drive.lane() + 1.0).abs() < f32::EPSILON);
        drive.centre();
        assert!(drive.lane().abs() < f32::EPSILON);
    }

    #[test]
    fn hors_pilotage_manuel_aucun_ordre_de_vol_ne_passe_mais_l_ascension_se_demande() {
        let mut drive = ManualDrive::default();
        drive.push(ManualOrder::Up);
        assert!(drive.pending().is_empty());
        drive.request_ascension();
        assert_eq!(drive.ascension_seq(), 1);
    }

    #[test]
    fn la_file_est_plafonnee_et_ordonnee() {
        let mut drive = ManualDrive::default();
        drive.set_engaged(true);
        for _ in 0..(MAX_ORDERS + 5) {
            drive.push(ManualOrder::Up);
        }
        assert_eq!(drive.pending().len(), MAX_ORDERS);
        drive.push(ManualOrder::Land);
        assert_eq!(drive.pop(), Some(ManualOrder::Up));
    }

    #[test]
    fn rendre_la_main_recentre_et_vide() {
        let mut drive = ManualDrive::default();
        drive.set_engaged(true);
        drive.nudge(1);
        drive.push(ManualOrder::TakeOff);
        drive.set_engaged(false);
        assert!(drive.lane().abs() < f32::EPSILON);
        assert!(drive.pending().is_empty());
        assert!(!drive.engaged());
    }
}
