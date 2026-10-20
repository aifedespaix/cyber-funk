//! Niveau 4 du culling : distance et niveaux de détail, par `VisibilityRange`.
//!
//! `VisibilityRange` borne la plage de distances caméra-objet où une entité existe pour une vue. Bevy la teste sur le
//! processeur, avant le frustum et sans rien dessiner (`check_visibility_ranges`) : au-delà de la plage, l'objet ne
//! coûte plus qu'un test de distance, pas un draw. La distance est celle de l'origine de l'entité, pas de son
//! `Aabb` : une grande boîte disparaît quand son centre franchit la limite, pas son bord.
//!
//! Deux emplois ici :
//!
//! - **rien au-delà de la brume** : [`until_fog`], plage abrupte qui s'arrête là où la brume de Sillage a tout
//!   avalé ([`FOG_FULL_M`]). Abrupte plutôt que fondue : l'objet est déjà de la couleur de la brume, un fondu ne se
//!   verrait pas, et il coûterait une variante de pipeline (le fondu passe par un dither dans le shader) en plus de
//!   celle des objets sans plage ;
//! - **fondre entre niveaux de détail** : [`lod_ranges`] donne la plage de chaque niveau d'une chaîne
//!   (détaillé → simplifié → imposteur), avec un fondu de `fade_m` entre deux niveaux. Les niveaux d'un même objet
//!   doivent être à la **même position** : le fondu superpose les deux.

use bevy::camera::visibility::VisibilityRange;

/// Distance à laquelle la brume de Sillage a tout couvert, en m (`smoothstep(90, 380, d)` du shader de façade,
/// `building-material.ts`, sans pluie). Sous la pluie la brume se resserre (jusqu'à 280 m) : cette borne est la plus
/// lointaine, donc la plus prudente.
pub const FOG_FULL_M: f32 = 380.0;

/// Plage abrupte `[0, end_m]` : l'objet n'est ni testé ni dessiné au-delà de `end_m` mètres de la caméra.
///
/// `end_m` est ramené à zéro s'il est négatif ou n'est pas un nombre.
#[must_use]
pub fn until_fog(end_m: f32) -> VisibilityRange {
    VisibilityRange::abrupt(0.0, if end_m.is_finite() { end_m.max(0.0) } else { 0.0 })
}

/// Plages d'une chaîne de niveaux de détail : une de plus que de `boundaries`.
///
/// `boundaries` sont les distances (m, croissantes) où l'on passe d'un niveau au suivant ; `fade_m` la longueur du
/// fondu autour de chacune (il commence à la frontière et finit `fade_m` plus loin) ; `last_end_m` la distance où
/// le dernier niveau disparaît, abruptement. Le niveau 0 commence à 0.
///
/// Les frontières non croissantes sont ramenées à la précédente, et un fondu plus long que l'écart entre deux
/// frontières est raccourci : la chaîne reste cohérente (`end_margin` d'un niveau = `start_margin` du suivant, ce
/// que le fondu exige) au lieu de produire une plage que Bevy refuserait.
///
/// Allocation : à appeler au démarrage (création des pièces), pas dans l'image.
#[must_use]
pub fn lod_ranges(boundaries: &[f32], fade_m: f32, last_end_m: f32) -> Vec<VisibilityRange> {
    let fade_m = if fade_m.is_finite() { fade_m.max(0.0) } else { 0.0 };
    // Frontières propres : finies, positives, croissantes.
    let mut starts: Vec<f32> = Vec::with_capacity(boundaries.len());
    for &b in boundaries {
        let previous = starts.last().copied().unwrap_or(0.0);
        starts.push(if b.is_finite() { b.max(previous) } else { previous });
    }
    // Fin du fondu de chaque frontière : bornée par la frontière suivante (ou par la fin de la chaîne).
    let last_end = if last_end_m.is_finite() {
        last_end_m.max(starts.last().copied().unwrap_or(0.0))
    } else {
        starts.last().copied().unwrap_or(0.0)
    };
    let fades: Vec<f32> = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let limit = starts.get(i + 1).copied().unwrap_or(last_end);
            (start + fade_m).min(limit)
        })
        .collect();

    let mut ranges = Vec::with_capacity(starts.len() + 1);
    for level in 0..=starts.len() {
        let start_margin = match level {
            0 => 0.0..0.0,
            _ => starts[level - 1]..fades[level - 1],
        };
        let end_margin = match starts.get(level) {
            Some(&start) => start..fades[level],
            None => last_end..last_end,
        };
        ranges.push(VisibilityRange {
            start_margin,
            end_margin,
            use_aabb: false,
        });
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_plage_de_brume_est_abrupte_et_commence_a_zero() {
        let range = until_fog(FOG_FULL_M);
        assert!(range.is_abrupt());
        assert!(range.is_visible_at_all(0.0));
        assert!(range.is_visible_at_all(FOG_FULL_M - 1.0));
        assert!(range.is_culled(FOG_FULL_M + 1.0));
        for bad in [f32::NAN, -10.0] {
            assert!(until_fog(bad).is_culled(1.0), "{bad}");
        }
    }

    #[test]
    fn la_chaine_de_niveaux_reprend_la_table_de_bevy() {
        // La table de la documentation de `VisibilityRange` : 20 m puis 70 m, fondu de 5 m, fin à 150 m.
        let ranges = lod_ranges(&[20.0, 70.0], 5.0, 150.0);
        assert_eq!(ranges.len(), 3);
        assert_eq!(
            (ranges[0].start_margin.clone(), ranges[0].end_margin.clone()),
            (0.0..0.0, 20.0..25.0)
        );
        assert_eq!(
            (ranges[1].start_margin.clone(), ranges[1].end_margin.clone()),
            (20.0..25.0, 70.0..75.0)
        );
        assert_eq!(
            (ranges[2].start_margin.clone(), ranges[2].end_margin.clone()),
            (70.0..75.0, 150.0..150.0)
        );
    }

    #[test]
    fn chaque_niveau_finit_ou_le_suivant_commence() {
        let ranges = lod_ranges(&[10.0, 40.0, 100.0], 8.0, 300.0);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end_margin, pair[1].start_margin);
            assert!(pair[0].start_margin.end <= pair[0].end_margin.start);
        }
    }

    #[test]
    fn un_objet_est_toujours_dans_au_moins_un_niveau() {
        let ranges = lod_ranges(&[20.0, 70.0], 5.0, 150.0);
        for i in 0..=1500 {
            let d = i as f32 / 10.0;
            let any = ranges.iter().any(|r| r.is_visible_at_all(d));
            // Bevy cache à partir de la borne haute elle-même (`d < end`).
            assert_eq!(any, d < 150.0, "d = {d}");
        }
    }

    #[test]
    fn des_frontieres_incoherentes_donnent_quand_meme_une_chaine_valide() {
        // Décroissantes, trop proches pour le fondu demandé, non finies.
        for boundaries in [&[50.0, 20.0][..], &[10.0, 12.0][..], &[f32::NAN, 30.0][..], &[][..]] {
            let ranges = lod_ranges(boundaries, 20.0, 100.0);
            assert_eq!(ranges.len(), boundaries.len() + 1);
            for r in &ranges {
                assert!(
                    r.start_margin.start <= r.start_margin.end,
                    "{:?} {:?}",
                    r.start_margin,
                    r.end_margin
                );
                assert!(
                    r.start_margin.end <= r.end_margin.start,
                    "{:?} {:?}",
                    r.start_margin,
                    r.end_margin
                );
                assert!(
                    r.end_margin.start <= r.end_margin.end,
                    "{:?} {:?}",
                    r.start_margin,
                    r.end_margin
                );
            }
            for pair in ranges.windows(2) {
                assert_eq!(pair[0].end_margin, pair[1].start_margin);
            }
        }
    }
}
