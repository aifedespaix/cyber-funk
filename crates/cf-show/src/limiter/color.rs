//! Couleurs du limiteur : de simples triplets `f32`, sans type de moteur (cf-show ne dépend pas de Bevy).

/// Couleur linéaire RGB, composantes en lumière relative (0 → 1, au-delà possible en HDR).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgb {
    /// Rouge, 0 → 1.
    pub r: f32,
    /// Vert, 0 → 1.
    pub g: f32,
    /// Bleu, 0 → 1.
    pub b: f32,
}

impl Rgb {
    /// Construit une couleur à partir de ses trois composantes.
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Une composante non finie ou négative n'a pas de sens pour une lumière ajoutée : elle devient 0.
    fn sanitized(self) -> Self {
        let clean = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
        Self::new(clean(self.r), clean(self.g), clean(self.b))
    }
}

/// Au-dessus de ce rouge, une teinte peut être jugée à dominante rouge.
const RED_FLOOR: f32 = 0.15;
/// Rapport rouge / (vert ou bleu) au-delà duquel la teinte est à dominante rouge.
const RED_DOMINANCE: f32 = 1.4;

/// Vrai si la couleur est un rouge saturé : celle que `defuse_red` refuse de laisser passer plein écran.
#[must_use]
pub fn is_saturated_red(color: Rgb) -> bool {
    color.r > RED_FLOOR && color.r > color.g.max(color.b) * RED_DOMINANCE
}

/// Un rouge saturé plein écran est interdit : une teinte à dominante rouge est ramenée vers un blanc froid.
///
/// Les composantes non finies ou négatives sont d'abord ramenées à 0 (écart de robustesse par rapport à Sillage, où
/// elles ne peuvent pas se présenter).
#[must_use]
pub fn defuse_red(color: Rgb) -> Rgb {
    let mut c = color.sanitized();
    if is_saturated_red(c) {
        let other = c.g.max(c.b);
        let grey = c.r * 0.55;
        let k = ((c.r / other.max(1e-4) - RED_DOMINANCE) / RED_DOMINANCE + 0.6).min(1.0);
        let target = other.max(grey);
        c.r += (grey - c.r) * k;
        c.g += (target - c.g) * k;
        c.b += (target - c.b) * k;
    }
    c
}

/// Couleur de flash passée par `defuse_red` : seul moyen d'en obtenir une, donc impossible de peindre un rouge saturé
/// plein écran sans contourner ce type. Les consommateurs (post-process) n'acceptent que celui-ci.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeColor(Rgb);

impl SafeColor {
    /// Désamorce le rouge saturé éventuel de `color` puis l'enveloppe.
    #[must_use]
    pub fn new(color: Rgb) -> Self {
        Self(defuse_red(color))
    }

    /// La couleur, garantie sans rouge saturé.
    #[must_use]
    pub const fn rgb(self) -> Rgb {
        self.0
    }
}
