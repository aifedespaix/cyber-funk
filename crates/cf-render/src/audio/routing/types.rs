//! Les types d'une ligne de la matrice de routage (P2 §2.2), tels qu'ils s'écrivent en RON.

use serde::{Deserialize, Serialize};

/// Le signal d'entrée d'une route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    /// Sub-basse 20–60 Hz.
    SubBass,
    /// Punch 60–150 Hz.
    Punch,
    /// Bas-médium 150–500 Hz.
    LowMid,
    /// Haut-médium 500–4000 Hz.
    HighMid,
    /// Transitoires 6–16 kHz.
    Transient,
    /// Énergie lente du morceau (`E_s`, l'intensité des sections), déjà normalisée.
    Slow,
    /// Énergie rapide (`flux` des attaques), déjà normalisée.
    Fast,
    /// Phase du temps, 0..1.
    Phase,
    /// Phase de la mesure, 0..1.
    Bar,
}

impl Source {
    /// Vrai pour les cinq bandes : elles passent par le niveau en dB, l'AGC et le seuil.
    #[must_use]
    pub const fn is_band(self) -> bool {
        matches!(self, Self::SubBass | Self::Punch | Self::LowMid | Self::HighMid | Self::Transient)
    }
}

/// Ce que l'on extrait du signal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Detector {
    /// L'enveloppe telle quelle.
    Envelope,
    /// Une crête qui retombe après un maintien.
    Peak {
        /// Constante de décroissance (s).
        decay_s: f32,
        /// Maintien (s).
        hold_s: f32,
    },
    /// Le flux positif de la bande, lu par son score z : 0 sous `z`, 1 à `z + 4` écarts-types.
    Flux {
        /// Seuil du score z.
        z: f32,
    },
    /// Le spectre en bandes logarithmiques (seule valeur admise : 32) ; la route porte la moyenne du spectre.
    Spectrum {
        /// Nombre de bandes.
        bins: usize,
    },
}

/// La courbe `x ← x^γ`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Curve {
    /// Identité.
    Linear,
    /// Puissance `γ` (`γ > 1` creuse le bas, `γ < 1` gonfle).
    Pow(f32),
}

/// Le lissage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Smooth {
    /// Aucun.
    None,
    /// Attaque et relâchement.
    AttackRelease {
        /// Constante de montée (s).
        attack_s: f32,
        /// Constante de descente (s).
        release_s: f32,
    },
    /// Attaque/relâchement puis ressort sous-amorti (cibles géométriques).
    AttackReleaseSpring {
        /// Constante de montée (s).
        attack_s: f32,
        /// Constante de descente (s).
        release_s: f32,
        /// Fréquence propre (Hz).
        hz: f32,
        /// Amortissement.
        zeta: f32,
    },
    /// Filtre One-Euro (signaux continus).
    OneEuro {
        /// Coupure minimale (Hz).
        f_min: f32,
        /// Coefficient de vitesse.
        beta: f32,
    },
    /// Ressort seul.
    Spring {
        /// Fréquence propre (Hz).
        hz: f32,
        /// Amortissement.
        zeta: f32,
    },
}

/// Borne de pente, en unités par seconde.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Slew {
    /// Montée maximale.
    pub up: f32,
    /// Descente maximale.
    pub down: f32,
}

/// Quantification : le front du signal attend la prochaine frontière de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quantize {
    /// Aucune.
    None,
    /// Le temps.
    Beat,
    /// Le demi-temps.
    HalfBeat,
    /// La double croche (un quart de temps).
    Sixteenth,
    /// La mesure.
    Bar,
}

impl Quantize {
    /// Frontières par mesure, `None` sans quantification.
    #[must_use]
    pub const fn per_bar(self) -> Option<f32> {
        match self {
            Self::None => None,
            Self::Bar => Some(1.0),
            Self::Beat => Some(4.0),
            Self::HalfBeat => Some(8.0),
            Self::Sixteenth => Some(16.0),
        }
    }
}

/// Voie du limiteur photosensible (`cf-show`) par où la route doit passer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimiterLane {
    /// Aucune : ce n'est pas une variation de luminance.
    None,
    /// Flash bref.
    Flash,
    /// Vague lente.
    Sweep,
}

/// Où va la valeur : ce que les systèmes de rendu lisent dans [`FilteredSignals`](super::FilteredSignals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// Macro-géométrie des tours (amplitude plafonnée côté consommateur à `0.02·H`).
    MacroGeometry,
    /// Masse du kick : la même macro-géométrie, pilotée par le punch.
    KickMass,
    /// Pulsation d'échelle globale autour du point de fuite (≤ 1,5 %).
    ScalePulse,
    /// Tremblement de caméra (`trauma`).
    CameraShake,
    /// Coup de focale.
    CameraFov,
    /// Dispersion du brouillard.
    FogDispersion,
    /// Flaques et rugosité du sol.
    Puddles,
    /// Queue du bloom.
    Bloom,
    /// Éclat émissif des néons (sub-basse).
    EmissiveBass,
    /// Éclat des enseignes (haut-médium).
    NeonIntensity,
    /// Saturation des enseignes.
    NeonSaturation,
    /// Oscillation des câbles.
    CableSwing,
    /// Flexion des pylônes et antennes.
    MastBend,
    /// Parallaxe des hologrammes.
    HologramDepth,
    /// Spectre vu-mètre des fenêtres (moyenne ; le détail est dans `spectrum`).
    WindowMeter,
    /// Taux d'émission des micro-particules.
    ParticleRate,
    /// Rafale de micro-particules.
    ParticleBurst,
    /// Flash laser.
    LaserFlash,
    /// Aberration chromatique.
    Aberration,
    /// Défilement UV glitché.
    Glitch,
    /// Scintillement des tubes.
    TubeFlicker,
    /// Lumière ambiante lente (brume, ciel).
    Ambience,
}

impl Target {
    /// Toutes les cibles, dans l'ordre de l'énumération.
    pub const ALL: [Self; 22] = [
        Self::MacroGeometry,
        Self::KickMass,
        Self::ScalePulse,
        Self::CameraShake,
        Self::CameraFov,
        Self::FogDispersion,
        Self::Puddles,
        Self::Bloom,
        Self::EmissiveBass,
        Self::NeonIntensity,
        Self::NeonSaturation,
        Self::CableSwing,
        Self::MastBend,
        Self::HologramDepth,
        Self::WindowMeter,
        Self::ParticleRate,
        Self::ParticleBurst,
        Self::LaserFlash,
        Self::Aberration,
        Self::Glitch,
        Self::TubeFlicker,
        Self::Ambience,
    ];

    /// Nombre de cibles.
    pub const COUNT: usize = Self::ALL.len();

    /// Rang dans [`FilteredSignals`](super::FilteredSignals).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Une ligne de la matrice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// Signal d'entrée.
    pub source: Source,
    /// Détecteur.
    pub detector: Detector,
    /// Courbe.
    pub curve: Curve,
    /// Lissage.
    pub smooth: Smooth,
    /// Cible.
    pub target: Target,
    /// Gain appliqué après lissage.
    pub gain: f32,
    /// Bornes de sortie.
    pub clamp: (f32, f32),
    /// Borne de pente.
    pub slew: Slew,
    /// Quantification.
    pub quantize: Quantize,
    /// Voie du limiteur.
    pub limiter: LimiterLane,
}
