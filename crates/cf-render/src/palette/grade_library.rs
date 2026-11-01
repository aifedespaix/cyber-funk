//! Les étalonnages livrés (`district_grades.ron`, compilé dans le binaire : rien à charger pendant un show) et leur
//! validation (bornes, 6 à 8 variantes par type, Neon 0 neutre).

use cf_show::district::{DistrictId, DistrictKind};
use serde::Deserialize;

use super::grade::{ACCENT_ROLES, DistrictGrade, VARIANTS_RANGE};
use super::scene::PaletteError;

/// Les étalonnages livrés, compilés dans le binaire.
const BUILTIN_GRADES: &str = include_str!("district_grades.ron");

/// Une ligne du RON : une variante.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GradeRow {
    hue_deg: f32,
    saturation: f32,
    lightness: f32,
    mint: f32,
    rose: f32,
    amber: f32,
    ice: f32,
}

impl GradeRow {
    fn into_grade(self) -> DistrictGrade {
        DistrictGrade {
            hue_deg: self.hue_deg,
            saturation: self.saturation,
            lightness: self.lightness,
            accents: [
                (ACCENT_ROLES[0], self.mint),
                (ACCENT_ROLES[1], self.rose),
                (ACCENT_ROLES[2], self.amber),
                (ACCENT_ROLES[3], self.ice),
            ],
        }
    }
}

/// Le fichier RON : les variantes de chaque type.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GradeFile {
    neon: Vec<GradeRow>,
    suburb: Vec<GradeRow>,
    residential: Vec<GradeRow>,
}

const fn kind_name(kind: DistrictKind) -> &'static str {
    match kind {
        DistrictKind::Neon => "neon",
        DistrictKind::Suburb => "suburb",
        DistrictKind::Residential => "residential",
    }
}

const fn kind_index(kind: DistrictKind) -> usize {
    match kind {
        DistrictKind::Neon => 0,
        DistrictKind::Suburb => 1,
        DistrictKind::Residential => 2,
    }
}

/// Les étalonnages de tous les quartiers, lus une fois à l'ouverture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GradeLibrary {
    kinds: [Vec<DistrictGrade>; 3],
}

impl GradeLibrary {
    /// Les étalonnages compilés dans le binaire (`district_grades.ron`).
    ///
    /// # Errors
    ///
    /// Fichier illisible, borne dépassée, nombre de variantes hors de 6 à 8, ou Neon n'a pas le neutre en tête.
    pub fn builtin() -> Result<Self, PaletteError> {
        Self::from_ron(BUILTIN_GRADES)
    }

    /// Les étalonnages d'un texte RON (même format que `district_grades.ron`).
    ///
    /// # Errors
    ///
    /// Voir [`GradeLibrary::builtin`].
    pub fn from_ron(text: &str) -> Result<Self, PaletteError> {
        let file: GradeFile = ron::from_str(text).map_err(PaletteError::GradesRon)?;
        let rows = [file.neon, file.suburb, file.residential];
        let mut kinds: [Vec<DistrictGrade>; 3] = Default::default();
        for ((kind, rows), slot) in DistrictKind::ALL.into_iter().zip(rows).zip(&mut kinds) {
            let bad = |variant: usize, reason: &str| PaletteError::BadGrade {
                kind: kind_name(kind),
                variant,
                reason: reason.to_owned(),
            };
            if !(VARIANTS_RANGE.0..=VARIANTS_RANGE.1).contains(&rows.len()) {
                return Err(bad(0, &format!("{} variantes, il en faut de 6 à 8", rows.len())));
            }
            for (variant, row) in rows.into_iter().enumerate() {
                let grade = row.into_grade();
                if !grade.within_bounds() {
                    return Err(bad(variant, "valeur hors des bornes"));
                }
                slot.push(grade);
            }
        }
        if !kinds[0][0].is_neutral() {
            return Err(PaletteError::BadGrade {
                kind: "neon",
                variant: 0,
                reason: "la variante 0 doit être l'étalonnage neutre".to_owned(),
            });
        }
        Ok(Self { kinds })
    }

    /// L'étalonnage d'un quartier. Une variante hors liste retombe sur le modulo (jamais de panique en plein show) ;
    /// une bibliothèque vide (échec de lecture) rend le neutre.
    #[must_use]
    pub fn grade(&self, id: DistrictId) -> DistrictGrade {
        let list = &self.kinds[kind_index(id.kind)];
        if list.is_empty() {
            return DistrictGrade::neutral();
        }
        list[usize::from(id.variant) % list.len()]
    }

    /// Nombre de variantes d'un type.
    #[must_use]
    pub fn variants(&self, kind: DistrictKind) -> u8 {
        u8::try_from(self.kinds[kind_index(kind)].len()).unwrap_or(u8::MAX)
    }
}
