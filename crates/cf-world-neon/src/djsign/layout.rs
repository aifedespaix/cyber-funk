//! Mise en page des lettres de l'enseigne du DJ (`dj-sign-layout.ts`) : pure (ni rendu ni police), donc testable. Les
//! largeurs de glyphes viennent d'une fonction de mesure (la police cuite en jeu, une table dans les tests), exprimées en
//! hauteurs de capitale : 1 = aussi large que la lettre est haute.

/// Au-delà, le nom est coupé : une enseigne de plus de vingt lettres ne se lirait plus.
pub const MAX_SIGN_CHARS: usize = 20;
/// Largeur d'une lettre, bornée pour qu'une police étrange ne casse pas la mise en page.
const MIN_ADVANCE: f32 = 0.3;
const MAX_ADVANCE: f32 = 1.3;

/// Les réglages de la mise en page (`LayoutOptions`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    /// Hauteur de capitale voulue, en mètres (avant réduction).
    pub height: f32,
    /// Largeur maximale de l'enseigne entière, en mètres : au-delà, tout est réduit d'un même facteur.
    pub max_width: f32,
    /// Espace entre deux lettres, en hauteurs de capitale.
    pub gap: f32,
    /// Largeur d'un espace entre deux mots, en hauteurs de capitale.
    pub space: f32,
    /// Nombre maximal de caractères du nom.
    pub max_chars: usize,
}

/// Les réglages de Sillage (`SIGN_LAYOUT`).
pub const SIGN_LAYOUT: LayoutOptions = LayoutOptions {
    height: 20.0,
    max_width: 132.0,
    gap: 0.16,
    space: 0.55,
    max_chars: MAX_SIGN_CHARS,
};

/// Une lettre posée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignLetter {
    /// Le caractère.
    pub ch: char,
    /// Rang parmi les lettres (les espaces ne comptent pas).
    pub index: usize,
    /// Centre de la lettre sur l'axe de l'enseigne, en mètres (0 au milieu).
    pub x: f32,
    /// Largeur du glyphe, en mètres.
    pub width: f32,
}

/// Le résultat de la mise en page.
#[derive(Debug, Clone, PartialEq)]
pub struct SignLayout {
    /// Le nom tel qu'il sera écrit : espaces nettoyés, majuscules, coupé à `max_chars`.
    pub name: String,
    /// Les lettres, de gauche à droite.
    pub letters: Vec<SignLetter>,
    /// Largeur totale occupée, en mètres.
    pub width: f32,
    /// Hauteur de capitale réelle, en mètres (réduite pour un nom long).
    pub height: f32,
    /// Facteur de réduction appliqué, 1 si le nom tient tel quel.
    pub scale: f32,
}

impl SignLayout {
    /// Une enseigne sans lettre.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            letters: Vec::new(),
            width: 0.0,
            height: SIGN_LAYOUT.height,
            scale: 1.0,
        }
    }
}

/// Majuscules, espaces simples, sans caractères de contrôle ; coupé à `max_chars` caractères.
///
/// Sillage normalisait aussi en NFC : les noms de la feuille de conduite arrivent déjà composés (RON, UTF-8), on ne
/// tire pas une crate d'Unicode pour ce seul cas.
#[must_use]
pub fn normalize_name(raw: &str, max_chars: usize) -> String {
    let spaced: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let cleaned = spaced.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase();
    let cut: String = cleaned.chars().take(max_chars).collect();
    cut.trim().to_owned()
}

fn clamp_advance(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(MIN_ADVANCE, MAX_ADVANCE)
    } else {
        MIN_ADVANCE
    }
}

/// Place les lettres de `raw` : centrées sur 0, espacées, et réduites d'un même facteur si le nom dépasse `max_width`.
#[must_use]
pub fn layout_sign(raw: &str, measure: &dyn Fn(char) -> f32, options: &LayoutOptions) -> SignLayout {
    let name = normalize_name(raw, options.max_chars);
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() {
        return SignLayout {
            name,
            ..SignLayout::empty()
        };
    }
    let advances: Vec<f32> = chars
        .iter()
        .map(|&c| {
            if c == ' ' {
                options.space
            } else {
                clamp_advance(measure(c))
            }
        })
        .collect();
    // L'espace entre deux lettres n'existe que si aucune des deux n'est une espace.
    let gap_after = |i: usize| i + 1 < chars.len() && chars[i] != ' ' && chars[i + 1] != ' ';
    let total: f32 = (0..chars.len())
        .map(|i| advances[i] + if gap_after(i) { options.gap } else { 0.0 })
        .sum();

    let natural = total * options.height;
    let scale = if natural > options.max_width {
        options.max_width / natural
    } else {
        1.0
    };
    let unit = options.height * scale;
    let mut letters = Vec::with_capacity(chars.len());
    let mut cursor = 0.0;
    for (i, &ch) in chars.iter().enumerate() {
        let advance = advances[i];
        if ch != ' ' {
            letters.push(SignLetter {
                ch,
                index: letters.len(),
                x: (cursor + advance / 2.0 - total / 2.0) * unit,
                width: advance * unit,
            });
        }
        cursor += advance;
        if gap_after(i) {
            cursor += options.gap;
        }
    }
    SignLayout {
        name,
        letters,
        width: total * unit,
        height: unit,
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: char) -> f32 {
        0.7
    }

    #[test]
    fn le_nom_est_nettoye() {
        assert_eq!(normalize_name("  dj \t neon\n", 20), "DJ NEON");
        assert_eq!(normalize_name("é\u{7}a", 20), "É A");
        assert_eq!(normalize_name("abcdefghijklmnopqrstuvwxyz", 20).chars().count(), 20);
        assert_eq!(normalize_name("   ", 20), "");
    }

    #[test]
    fn un_nom_vide_ne_donne_aucune_lettre() {
        let l = layout_sign("", &flat, &SIGN_LAYOUT);
        assert!(l.letters.is_empty(), "pas de nom, pas de lettre");
        assert!(l.width.abs() < f32::EPSILON);
    }

    #[test]
    fn les_lettres_sont_centrees_et_les_espaces_ne_comptent_pas() {
        let l = layout_sign("AB C", &flat, &SIGN_LAYOUT);
        assert_eq!(l.letters.len(), 3);
        assert_eq!(l.letters.iter().map(|x| x.index).collect::<Vec<_>>(), [0, 1, 2]);
        let first = l.letters[0];
        let last = l.letters[2];
        // Symétrique : le bord gauche de la première est l'opposé du bord droit de la dernière.
        assert!(((first.x - first.width / 2.0) + (last.x + last.width / 2.0)).abs() < 1e-3);
        assert!(l.letters.windows(2).all(|w| w[0].x < w[1].x));
    }

    #[test]
    fn un_nom_long_est_reduit_a_la_largeur_max() {
        let l = layout_sign("ABCDEFGHIJKLMNOPQRST", &flat, &SIGN_LAYOUT);
        assert!(l.scale < 1.0);
        assert!((l.width - SIGN_LAYOUT.max_width).abs() < 1e-2, "{}", l.width);
        assert!((l.height - SIGN_LAYOUT.height * l.scale).abs() < 1e-4);
    }

    #[test]
    fn un_nom_court_garde_sa_taille() {
        let l = layout_sign("DJ", &flat, &SIGN_LAYOUT);
        assert!((l.scale - 1.0).abs() < f32::EPSILON);
        assert!((l.height - SIGN_LAYOUT.height).abs() < f32::EPSILON);
        // 2 × 0,7 + 1 interstice de 0,16, en hauteurs de 20 m.
        assert!((l.width - (1.4 + 0.16) * 20.0).abs() < 1e-3);
    }

    #[test]
    fn une_mesure_folle_est_bornee() {
        let l = layout_sign("A", &|_| f32::NAN, &SIGN_LAYOUT);
        assert!((l.letters[0].width - MIN_ADVANCE * 20.0).abs() < 1e-3);
        let l = layout_sign("A", &|_| 99.0, &SIGN_LAYOUT);
        assert!((l.letters[0].width - MAX_ADVANCE * 20.0).abs() < 1e-3);
    }
}
