//! Les couronnes d'immeubles de part et d'autre de la route (`LAYERS` de `scenery.ts`).

/// Couronnes de premier plan : trapues, collées à la route, et deux couronnes de tours plus hautes.
pub(super) const NEAR_LAYERS: usize = 3;

/// Une couronne d'immeubles : où elle commence, ses plages de tailles, la part d'emplacements vides.
pub(super) struct Layer {
    /// Distance à l'axe de la route du bord intérieur, en m.
    pub(super) x: f64,
    /// Étalement aléatoire du bord intérieur, en m.
    pub(super) spread: f64,
    pub(super) width: [f64; 2],
    pub(super) depth: [f64; 2],
    pub(super) height: [f64; 2],
    /// Part d'emplacements laissés vides.
    pub(super) skip: f64,
    /// Part de tours qui portent un projecteur.
    pub(super) spots: f64,
    /// Couronne d'arrière-plan : un corps seul, sans couronne, ombrage simplifié.
    pub(super) far: bool,
}

pub(super) const LAYERS: [Layer; 6] = [
    // Premier plan : trapus, collés à la route. Puis deux couronnes de tours plus hautes.
    Layer {
        x: 13.5,
        spread: 3.0,
        width: [8.0, 13.0],
        depth: [8.0, 12.0],
        height: [9.0, 28.0],
        skip: 0.22,
        spots: 0.55,
        far: false,
    },
    Layer {
        x: 34.0,
        spread: 6.0,
        width: [10.0, 18.0],
        depth: [10.0, 16.0],
        height: [20.0, 50.0],
        skip: 0.25,
        spots: 0.35,
        far: false,
    },
    Layer {
        x: 58.0,
        spread: 10.0,
        width: [12.0, 22.0],
        depth: [12.0, 20.0],
        height: [38.0, 85.0],
        skip: 0.1,
        spots: 0.0,
        far: false,
    },
    // Arrière-plan : trois couronnes de plus en plus hautes et larges, qui dessinent la ligne d'horizon de la ville.
    // Elles sont presque noyées dans la brume : un corps seul, sans couronne, avec un ombrage simplifié (tier 3).
    Layer {
        x: 98.0,
        spread: 14.0,
        width: [16.0, 30.0],
        depth: [16.0, 24.0],
        height: [70.0, 140.0],
        skip: 0.12,
        spots: 0.0,
        far: true,
    },
    Layer {
        x: 150.0,
        spread: 20.0,
        width: [20.0, 36.0],
        depth: [18.0, 26.0],
        height: [100.0, 200.0],
        skip: 0.1,
        spots: 0.0,
        far: true,
    },
    Layer {
        x: 214.0,
        spread: 30.0,
        width: [26.0, 46.0],
        depth: [20.0, 28.0],
        height: [140.0, 280.0],
        skip: 0.1,
        spots: 0.0,
        far: true,
    },
];
