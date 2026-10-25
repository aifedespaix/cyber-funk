//! Les dessins de la partie : envahisseurs, vaisseau, boss.

#[derive(Debug, Clone)]
pub(super) struct Sprite {
    pub(super) w: usize,
    pub(super) h: usize,
    pub(super) on: Vec<u8>,
}

/// Lit un dessin en texte (`#` allumé), haut du dessin en haut ; stocké l'axe y vers le haut.
pub(super) fn sprite(rows: &[&str]) -> Sprite {
    let w = rows.first().map_or(0, |r| r.len());
    let h = rows.len();
    let mut on = vec![0_u8; w * h];
    for (ry, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            on[(h - 1 - ry) * w + x] = u8::from(c == b'#');
        }
    }
    Sprite { w, h, on }
}

/// Le boss : une soucoupe à dôme, deux yeux creux à pupilles, une rangée de dents. Construit à partir d'un profil de
/// demi-largeurs.
pub(super) fn boss_sprite() -> Sprite {
    let half = [4_i32, 6, 8, 9, 10, 11, 11, 11, 11, 10, 9];
    let rows: Vec<String> = half
        .iter()
        .enumerate()
        .map(|(y, &hw)| {
            (0..23_i32)
                .map(|x| {
                    let d = (x - 11).abs();
                    if d > hw {
                        return '.';
                    }
                    let eye = (3..=6).contains(&y) && ((5..=8).contains(&x) || (14..=17).contains(&x));
                    let pupil = (4..=5).contains(&y) && ((6..=7).contains(&x) || (15..=16).contains(&x));
                    if eye {
                        return if pupil { '#' } else { '.' };
                    }
                    if y >= 9 && x % 2 == 0 && d <= 7 {
                        return '.';
                    }
                    '#'
                })
                .collect()
        })
        .collect();
    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
    sprite(&refs)
}
