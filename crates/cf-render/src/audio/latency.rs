//! Ligne à retard du décalage visuel : tient les états et les événements d'analyse pendant `délai` secondes avant de
//! les montrer à l'image.
//!
//! Tout est daté sur l'horloge murale de l'image (celle de `Time<Real>`), pas sur le temps audio : le temps audio
//! s'arrête quand le signal s'arrête (pause, débranchement) alors que le décalage, lui, doit continuer de courir, sans
//! quoi l'image resterait figée sur le dernier état « vivant ». L'âge d'un événement au moment où l'image le découvre
//! se déduit de l'écart entre son temps audio et celui de l'état lu juste avant, deux dates de la même horloge audio.
//!
//! Sans allocation en régime établi : les deux files sont dimensionnées une fois.

use std::collections::VecDeque;

use cf_analysis::{AudioEvent, AudioFeatures};

/// Décalage maximal réglable (ms). Au-delà, l'historique des états deviendrait un tampon vidéo ; 500 ms couvre les
/// projecteurs les plus lents et les retards de sonorisation de grandes salles.
pub const MAX_LATENCY_MS: f32 = 500.0;

/// Capacité de l'historique des états : une entrée par image, 500 ms à 240 i/s en tiennent 120 ; on garde de la marge.
const HISTORY_CAPACITY: usize = 512;

/// Capacité des événements en attente : le canal d'analyse en tient 256, la file ne peut pas en recevoir plus que lui
/// entre deux images raisonnables.
const PENDING_CAPACITY: usize = 512;

/// Les états et événements d'analyse en attente d'être montrés.
#[derive(Debug)]
pub struct LatencyLine {
    /// États vus à chaque image, du plus ancien au plus récent : (date murale, état).
    history: VecDeque<(f64, AudioFeatures)>,
    /// Événements pas encore échus : (date murale estimée de l'événement, événement).
    pending: VecDeque<(f64, AudioEvent)>,
    /// Événements jetés faute de place.
    dropped: u64,
}

impl Default for LatencyLine {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyLine {
    /// Ligne vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            pending: VecDeque::with_capacity(PENDING_CAPACITY),
            dropped: 0,
        }
    }

    /// Note l'état lu à l'image de date murale `wall_s`.
    pub fn push_features(&mut self, wall_s: f64, features: AudioFeatures) {
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back((wall_s, features));
    }

    /// Met un événement en attente ; `wall_s` est la date murale estimée à laquelle il s'est produit.
    pub fn push_event(&mut self, wall_s: f64, event: AudioEvent) {
        if self.pending.len() == PENDING_CAPACITY {
            self.pending.pop_front();
            self.dropped += 1;
        }
        self.pending.push_back((wall_s, event));
    }

    /// Prochain événement dont la date plus `delay_s` est passée à `now_s`, dans l'ordre d'arrivée.
    pub fn pop_due_event(&mut self, now_s: f64, delay_s: f64) -> Option<AudioEvent> {
        let &(at, _) = self.pending.front()?;
        if at + delay_s > now_s {
            return None;
        }
        self.pending.pop_front().map(|(_, event)| event)
    }

    /// L'état tel qu'il était `delay_s` secondes avant `now_s` : le plus récent de ceux datés d'au plus
    /// `now_s - delay_s`. `None` tant que l'historique ne remonte pas assez loin (démarrage). Les états plus anciens
    /// que celui rendu sont oubliés.
    pub fn features_at(&mut self, now_s: f64, delay_s: f64) -> Option<AudioFeatures> {
        let target = now_s - delay_s;
        let due = self.history.partition_point(|&(at, _)| at <= target);
        let keep_from = due.checked_sub(1)?;
        self.history.drain(..keep_from);
        self.history.front().map(|&(_, features)| features)
    }

    /// Événements jetés faute de place depuis le début.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Événements en attente (tests, diagnostic).
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use cf_analysis::AudioEventKind;

    use super::*;

    fn at(step: u64) -> AudioFeatures {
        AudioFeatures {
            step,
            ..AudioFeatures::NEUTRAL
        }
    }

    fn kick(time_s: f64) -> AudioEvent {
        AudioEvent {
            time_s,
            step: 1,
            kind: AudioEventKind::Kick,
        }
    }

    #[test]
    fn sans_delai_l_etat_le_plus_recent_est_rendu() {
        let mut line = LatencyLine::new();
        line.push_features(1.0, at(1));
        line.push_features(1.016, at(2));
        assert_eq!(line.features_at(1.016, 0.0).map(|f| f.step), Some(2));
    }

    #[test]
    fn avec_delai_on_rend_l_etat_d_il_y_a_delai_secondes() {
        let mut line = LatencyLine::new();
        for i in 0..20_u32 {
            line.push_features(f64::from(i) * 0.01, at(u64::from(i)));
        }
        // À t = 0,19 s avec 100 ms de retard : l'état daté de 0,09 s (image 9).
        assert_eq!(line.features_at(0.19, 0.1).map(|f| f.step), Some(9));
    }

    #[test]
    fn tant_que_l_historique_est_trop_court_rien_n_est_rendu() {
        let mut line = LatencyLine::new();
        line.push_features(5.0, at(1));
        assert!(line.features_at(5.05, 0.2).is_none());
        assert!(line.features_at(5.25, 0.2).is_some());
    }

    #[test]
    fn un_signal_perdu_finit_par_se_voir_meme_quand_le_temps_audio_est_fige() {
        let mut line = LatencyLine::new();
        let live = AudioFeatures {
            live: true,
            ..at(1)
        };
        line.push_features(0.0, live);
        // Le temps audio ne bouge plus (`step` constant) mais les images continuent de noter l'état.
        for i in 1..=30_u32 {
            line.push_features(f64::from(i) * 0.01, AudioFeatures { live: false, ..at(1) });
        }
        assert!(line.features_at(0.05, 0.1).is_none());
        assert!(line.features_at(0.10, 0.1).is_some_and(|f| f.live));
        assert!(line.features_at(0.30, 0.1).is_some_and(|f| !f.live));
    }

    #[test]
    fn un_evenement_n_echoit_qu_apres_le_delai() {
        let mut line = LatencyLine::new();
        line.push_event(10.0, kick(1.0));
        assert!(line.pop_due_event(10.05, 0.1).is_none());
        assert!(line.pop_due_event(10.1, 0.1).is_some());
        assert!(line.pop_due_event(10.2, 0.1).is_none());
    }

    #[test]
    fn les_evenements_sortent_dans_l_ordre() {
        let mut line = LatencyLine::new();
        line.push_event(1.0, kick(1.0));
        line.push_event(1.2, kick(2.0));
        assert_eq!(line.pop_due_event(2.0, 0.0).map(|e| e.time_s), Some(1.0));
        assert_eq!(line.pop_due_event(2.0, 0.0).map(|e| e.time_s), Some(2.0));
    }

    #[test]
    fn la_file_pleine_jette_le_plus_ancien_et_le_compte() {
        let mut line = LatencyLine::new();
        for i in 0..(PENDING_CAPACITY + 3) {
            line.push_event(i as f64, kick(i as f64));
        }
        assert_eq!(line.pending_len(), PENDING_CAPACITY);
        assert_eq!(line.dropped(), 3);
    }

    #[test]
    fn l_historique_reste_borne() {
        let mut line = LatencyLine::new();
        for i in 0..(HISTORY_CAPACITY * 2) {
            line.push_features(i as f64 * 0.001, at(i as u64));
        }
        assert!(line.history.len() <= HISTORY_CAPACITY);
    }
}
