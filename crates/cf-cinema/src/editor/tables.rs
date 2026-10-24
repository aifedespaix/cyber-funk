//! Les constantes réglées à la main de l'éditeur (`cinema/editor.ts`), reprises de Sillage.

/// Au-delà de cette occasion (0 → 1), le réalisateur bascule dès le prochain temps.
pub const OPPORTUNITY_THRESHOLD: f32 = 0.7;
/// Écart minimal (s) entre deux plans rares (ou épiques) : garde-fou de rareté.
pub const RARE_GAP: f64 = 75.0;
/// Écart minimal (s) entre deux plans épiques.
pub const EPIC_GAP: f64 = 240.0;
/// Aucun plan épique avant ce délai (s) depuis le départ du réalisateur.
pub const EPIC_NOT_BEFORE: f64 = 90.0;
/// Nombre de derniers plans écartés du tirage strict.
pub(super) const RECENT: usize = 3;
/// Une montée garde son influence ce temps (s), un break celui-ci.
pub(super) const BUILD_SECONDS: f64 = 14.0;
pub(super) const BREAK_SECONDS: f64 = 25.0;
/// Plans trop nerveux pour le mode `calm` (mouvement réduit).
pub(super) const CALM_BLOCK: &[&str] = &["strobe", "dutch", "flyby", "spiral", "vertigo", "wheel"];
/// Coups de pouce selon l'humeur du morceau.
pub(super) const BUILD_BONUS: &[(&str, f32)] = &[
    ("vertigo", 3.0),
    ("crane", 3.0),
    ("spiral", 3.0),
    ("push-in", 3.0),
    ("dolly-zoom", 3.0),
];
pub(super) const BREAK_BONUS: &[(&str, f32)] = &[
    ("god-view", 3.0),
    ("sky-tilt", 3.0),
    ("ad-glance", 2.5),
    ("moon-pass", 2.5),
    ("ringworld", 3.0),
    ("orbit", 1.5),
    ("side-track", 1.3),
    ("city-wide", 3.5),
];
/// Projecteur : les plans serrés et sombres passent moins souvent (poids), l'habitacle est plafonné à une part du
/// temps.
pub(super) const PROJECTOR_WEIGHT: &[(&str, f32)] = &[("wheel", 0.35), ("bumper", 0.5), ("cockpit", 0.5)];
/// Part d'écran maximale de l'habitacle en spectacle (0 → 1).
pub const COCKPIT_CAP: f64 = 0.08;
/// Respiration : plans lents et larges seulement.
pub(super) const BREATH_SHOTS: &[&str] = &[
    "chase",
    "orbit",
    "side-track",
    "crane",
    "sky-tilt",
    "god-view",
    "top-down",
    "city-wide",
    "ad-glance",
    "landmark-reveal",
    "ride-along",
    "moon-pass",
    "ringworld",
    "dj-sign",
    "push-in",
];
/// Coups de pouce d'un drop.
pub(super) const DROP_BONUS: &[(&str, f32)] = &[
    ("chase", 1.6),
    ("wheel", 2.0),
    ("side-track", 1.5),
    ("bumper", 1.5),
    ("flyby", 1.5),
    ("strobe", 2.0),
    ("orbit", 1.2),
];
/// Plans dont le rythme coupe l'image à chaque temps : coupés quand les flashs le sont.
pub(super) const STROBING: &[&str] = &["strobe"];

pub(super) fn lookup(table: &[(&str, f32)], id: &str) -> Option<f32> {
    table.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
}
