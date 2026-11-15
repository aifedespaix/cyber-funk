//! Le cadrage de la passation de DJ : ce que le monde dit au réalisateur pour poser la caméra devant la voiture qui roule
//! seule, et regarder arriver la suivante.
//!
//! Le monde (`cf-world-neon::run`) tire un cadrage différent à chaque passation ; le réalisateur (`cf-cinema`) le lit
//! dans `CinemaWorld` sans rien connaître du séquenceur.

use bevy::math::DVec3;
use bevy::prelude::Resource;

/// Le nombre de styles de cadrage.
pub const FRAMING_STYLES: u8 = 5;

/// Comment la caméra se tient devant la voiture seule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    /// Le style (0 à [`FRAMING_STYLES`] − 1) : décide de la forme du mouvement, pas des valeurs.
    pub style: u8,
    /// Distance de l'œil devant la voiture (m).
    pub ahead_m: f32,
    /// Hauteur de l'œil au-dessus de la route (m).
    pub height_m: f32,
    /// Écart latéral de l'œil, positif vers la droite (m).
    pub lateral_m: f32,
    /// Balayage latéral de l'œil au cours du plan (m), pour le style « orbite ».
    pub sweep_m: f32,
    /// Distance derrière la voiture du point visé (m) : on regarde la route qu'elle laisse et d'où vient la suivante.
    pub look_back_m: f32,
    /// Focale (°).
    pub fov_deg: f32,
}

impl Framing {
    /// Un cadrage neutre : bas, devant, à 20 m.
    pub const NEUTRAL: Self = Self {
        style: 0,
        ahead_m: 20.0,
        height_m: 1.2,
        lateral_m: 1.5,
        sweep_m: 0.0,
        look_back_m: 60.0,
        fov_deg: 46.0,
    };
}

/// Ce que le réalisateur voit de la passation : la voiture qui roule seule et le cadrage voulu. `None` hors passation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandoverView {
    /// Position absolue de la voiture qui roule seule (m).
    pub lead: DVec3,
    /// Sa vitesse le long de la route (m/s).
    pub speed_mps: f32,
    /// Le cadrage de cette passation.
    pub framing: Framing,
}

/// La passation vue par le réalisateur. Écrite par le monde seul (`None` hors passation), lue pour composer
/// `CinemaWorld`.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct HandoverFeed(pub Option<HandoverView>);

/// La séquence scénarisée en cours : la caméra y suit un découpage écrit, pas le montage automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staging {
    /// L'ouverture : la voiture entre dans le champ, passe sous le portique, le quartier se lève.
    Opening,
    /// La clôture : la voiture file vers l'horizon, le quartier rentre sous terre, la route reste.
    Closing,
}

/// Ce que le réalisateur voit d'une séquence scénarisée (plans `opening` et `closing`). `None` hors séquence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StagingView {
    /// La séquence jouée.
    pub kind: Staging,
    /// Secondes écoulées depuis son début.
    pub t_s: f32,
    /// Durée totale de la séquence (s).
    pub duration_s: f32,
    /// Secondes écoulées depuis que le quartier a commencé à se lever (négatif avant) : la grue de l'ouverture s'y cale.
    pub city_s: f32,
    /// Position du portique sur la route (milieu de la travée, au sol), s'il est posé.
    pub gantry: Option<DVec3>,
    /// La voiture est visible (présence non nulle) : la clôture ne la regarde plus une fois disparue.
    pub car_visible: bool,
}

/// La séquence scénarisée vue par le réalisateur. Écrite par le monde seul, lue pour composer `CinemaWorld`.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct StagingFeed(pub Option<StagingView>);
