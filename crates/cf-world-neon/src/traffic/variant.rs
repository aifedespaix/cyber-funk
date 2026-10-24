//! Les modèles de véhicules (`crafts.ts`) : les huit engins du Space Kit de Kenney (CC0), la voiture de police (le
//! même modèle que le racer, plus une rampe de gyrophares) et la moto lumineuse procédurale.

/// Les fichiers de `assets/models/crafts/craft_<nom>.glb`, dans l'ordre des variantes 0 à 7. Ils regardent vers −Z,
/// comme la voiture. Les matériaux d'origine servent de code : « metalRed » (l'orange) devient la pièce d'accent
/// teintée par instance.
pub const CRAFT_FILES: [&str; 8] = [
    "speederA", "speederB", "speederC", "speederD", "racer", "miner", "cargoA", "cargoB",
];

/// Nombre de modèles (`VARIANT_COUNT`) : les huit fichiers, la police, la moto.
pub const VARIANT_COUNT: usize = 10;

/// Profilés, rapides : le gros du trafic.
pub const SLEEK: [u8; 5] = [0, 1, 2, 3, 4];
/// Gros porteurs : mineur et cargos.
pub const HEAVY: [u8; 3] = [5, 6, 7];
/// Cargos.
pub const CARGO: [u8; 2] = [6, 7];
/// La voiture de police : le modèle du racer, plus ses gyrophares.
pub const POLICE: u8 = 8;
/// La moto lumineuse : modèle procédural, ajouté après les fichiers.
pub const BIKE: u8 = 9;

/// Le fichier d'une variante. La police réutilise le racer ; la moto n'a pas de fichier.
#[must_use]
pub const fn file_of(variant: u8) -> Option<&'static str> {
    match variant {
        0..=7 => Some(CRAFT_FILES[variant as usize]),
        POLICE => Some("racer"),
        _ => None,
    }
}

/// Nombre d'instances par modèle : large pour les profilés, serré pour le reste (`CAPACITY`).
pub const CAPACITY: [usize; VARIANT_COUNT] = [34, 34, 34, 34, 34, 18, 6, 6, 6, 6];
/// Nombre de véhicules vivants en même temps, toutes variantes confondues (`POOL`).
pub const POOL: usize = 220;

/// Longueur normalisée d'un modèle de véhicule : multipliée par l'échelle, la longueur réelle en mètres
/// (`CRAFT_LENGTH` de `director.ts`, `LENGTH` de `crafts.ts`).
pub const CRAFT_LENGTH: f32 = 2.4;

/// Les surfaces d'un véhicule, écrites par sommet dans le canal rouge de la couleur du maillage (`KIND` de
/// `vehicle-material.ts`). Les teintes viennent de l'instance (`MeshTag`) ou de la palette : aucune couleur en dur.
pub mod kind {
    /// Coque claire.
    pub const HULL: f32 = 0.0;
    /// Coque sombre.
    pub const HULL_DARK: f32 = 1.0;
    /// Pièce d'accent, à la teinte de l'instance.
    pub const ACCENT: f32 = 2.0;
    /// Bande néon, à la teinte de l'instance.
    pub const NEON: f32 = 3.0;
    /// Vitre.
    pub const GLASS: f32 = 4.0;
    /// Phare.
    pub const HEAD: f32 = 5.0;
    /// Gyrophare gauche (rouge).
    pub const BEACON_LEFT: f32 = 6.0;
    /// Gyrophare droit (bleu).
    pub const BEACON_RIGHT: f32 = 7.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_capacites_tiennent_dans_le_pool() {
        assert!(CAPACITY.iter().sum::<usize>() <= POOL);
        assert_eq!(CAPACITY.len(), VARIANT_COUNT);
    }

    #[test]
    fn la_police_est_un_racer_et_la_moto_n_a_pas_de_fichier() {
        assert_eq!(file_of(POLICE), Some("racer"));
        assert_eq!(file_of(BIKE), None);
        assert_eq!(file_of(0), Some("speederA"));
    }
}
