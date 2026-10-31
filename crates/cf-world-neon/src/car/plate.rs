//! La plaque d'immatriculation : le nom du DJ du créneau en cours, qui défile.
//!
//! Même principe que le compteur et l'autoradio (`screens.rs`) : une image RGBA8 émissive, redessinée sur place. Une
//! police bitmap 5 × 7 est embarquée (lettres, chiffres, ponctuation courante, accents ramenés à leur lettre) : aucun
//! fichier de police, donc rien à charger pendant un show (règle 8). Une lettre inconnue laisse sa place vide.
//!
//! Le texte défile en boucle, à vitesse constante et toujours dans le même sens ; la luminosité ne change pas, donc le
//! limiteur de flashs (règle 7) n'a rien à borner.

use super::screens::Pen;

/// Largeur de l'image de la plaque, px.
pub const PLATE_W: i32 = 256;
/// Hauteur de l'image de la plaque, px.
pub const PLATE_H: i32 = 56;
/// Taille d'un point de la police, px.
const DOT: i32 = 6;
/// Avance d'un caractère : 5 colonnes plus une d'espacement.
const ADVANCE: i32 = 6 * DOT;
/// Espace entre la fin du nom et son retour, en caractères.
const GAP_CHARS: i32 = 4;
/// Vitesse de défilement, px/s.
pub const SCROLL_PX_PER_S: f32 = 44.0;
/// Longueur maximale d'un nom affiché, en caractères (comme l'enseigne du DJ).
const MAX_CHARS: usize = 24;

/// Glyphes 5 × 7 : une ligne par entier, bit 4 à gauche.
const GLYPHS: &[(char, [u8; 7])] = &[
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
    ('3', [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    ('.', [0, 0, 0, 0, 0, 0b01100, 0b01100]),
    ('-', [0, 0, 0, 0b11111, 0, 0, 0]),
    ('_', [0, 0, 0, 0, 0, 0, 0b11111]),
    ('!', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100]),
    ('?', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100]),
    ('&', [0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101]),
    ('\'', [0b00100, 0b00100, 0b01000, 0, 0, 0, 0]),
    ('+', [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0]),
    (':', [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0]),
    ('/', [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000]),
    ('#', [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010]),
    ('@', [0b01110, 0b10001, 0b10111, 0b10101, 0b10110, 0b10000, 0b01110]),
    ('*', [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0]),
    ('(', [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010]),
    (')', [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000]),
    ('$', [0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100]),
    ('%', [0b11000, 0b11001, 0b00010, 0b00100, 0b01000, 0b10011, 0b00011]),
    (',', [0, 0, 0, 0, 0b01100, 0b00100, 0b01000]),
    (' ', [0; 7]),
    // Ponctuation de séparation : un losange plein, pour « NOM ◆ NOM ».
    ('◆', [0, 0b00100, 0b01110, 0b11111, 0b01110, 0b00100, 0]),
    ('<', [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010]),
    ('>', [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000]),
    ('=', [0, 0, 0b11111, 0, 0b11111, 0, 0]),
    (';', [0, 0b01100, 0b01100, 0, 0b01100, 0b00100, 0b01000]),
    ('[', [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110]),
    (']', [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110]),
    ('|', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('^', [0b00100, 0b01010, 0b10001, 0, 0, 0, 0]),
    ('~', [0, 0, 0b01000, 0b10101, 0b00010, 0, 0]),
    ('"', [0b01010, 0b01010, 0, 0, 0, 0, 0]),
];

/// Un caractère ramené à celui que la police sait dessiner : accents retirés, minuscules en capitales.
fn plain(ch: char) -> char {
    match ch {
        'À' | 'Â' | 'Ä' => 'A',
        'Ç' => 'C',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Î' | 'Ï' => 'I',
        'Ô' | 'Ö' | 'Œ' => 'O',
        'Ù' | 'Û' | 'Ü' => 'U',
        'Ÿ' => 'Y',
        other => other,
    }
}

/// Le nom tel qu'il s'affiche : espaces réduits, capitales, accents ramenés, [`MAX_CHARS`] caractères au plus. Un nom
/// vide devient « DJ n » (même repli que les cartons de titre).
#[must_use]
pub fn plate_text(name: &str, slot: usize) -> String {
    let spaced: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let cleaned = spaced.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase();
    let cut: String = cleaned.chars().take(MAX_CHARS).map(plain).collect();
    let cut = cut.trim().to_owned();
    if cut.is_empty() {
        format!("DJ {}", slot + 1)
    } else {
        cut
    }
}

/// Lignes du glyphe d'un caractère, `None` s'il n'est pas dans la police.
fn glyph(ch: char) -> Option<&'static [u8; 7]> {
    GLYPHS.iter().find(|(c, _)| *c == ch).map(|(_, rows)| rows)
}

fn draw_char(pen: &mut Pen<'_>, x: i32, y: i32, ch: char, color: [u8; 3]) {
    let Some(rows) = glyph(ch) else { return };
    for (row, bits) in (0_i32..).zip(rows) {
        for col in 0..5_i32 {
            if bits & (1 << (4 - col)) != 0 {
                pen.rect(x + col * DOT, y + row * DOT, DOT, DOT, color);
            }
        }
    }
}

/// Largeur d'un tour de texte, px : le nom, puis un espace où passe un losange.
#[must_use]
pub fn loop_width(text: &str) -> i32 {
    let chars = i32::try_from(text.chars().count()).unwrap_or(0);
    (chars + GAP_CHARS) * ADVANCE
}

/// Décalage du texte à l'instant `seconds` : toujours vers la gauche, en boucle.
#[must_use]
pub fn scroll_offset(seconds: f32, text: &str) -> i32 {
    let width = loop_width(text).max(1);
    let offset = (seconds * SCROLL_PX_PER_S).rem_euclid(width as f32);
    offset as i32
}

/// Dessine la plaque : fond sombre, deux filets aux couleurs du second néon, nom en couleur de la voiture.
pub fn draw_plate(data: &mut [u8], text: &str, seconds: f32, accent: [u8; 3], edge: [u8; 3]) {
    let mut pen = Pen {
        data,
        width: PLATE_W,
        height: PLATE_H,
    };
    pen.clear([3, 2, 6]);
    let width = loop_width(text);
    let offset = scroll_offset(seconds, text);
    let y = (PLATE_H - 7 * DOT) / 2;
    // Autant de copies que nécessaire pour couvrir la plaque quand le nom est plus court qu'elle.
    let mut start = -offset;
    while start < PLATE_W {
        for (i, ch) in (0_i32..).zip(text.chars()) {
            let x = start + i * ADVANCE;
            if x + ADVANCE > 0 && x < PLATE_W {
                draw_char(&mut pen, x, y, plain(ch), accent);
            }
        }
        let diamond = start + (i32::try_from(text.chars().count()).unwrap_or(0) + GAP_CHARS / 2 - 1) * ADVANCE;
        if diamond + ADVANCE > 0 && diamond < PLATE_W {
            draw_char(&mut pen, diamond, y, '◆', edge);
        }
        start += width;
    }
    pen.rect(0, 0, PLATE_W, 2, edge);
    pen.rect(0, PLATE_H - 2, PLATE_W, 2, edge);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer() -> Vec<u8> {
        vec![0; usize::try_from(PLATE_W * PLATE_H * 4).unwrap_or(0)]
    }

    fn lit(data: &[u8]) -> usize {
        data.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 100 || p[1] > 100 || p[2] > 100)
            .count()
    }

    #[test]
    fn le_nom_est_normalise_et_un_nom_vide_devient_dj_n() {
        assert_eq!(plate_text("  aife   de  paix ", 0), "AIFE DE PAIX");
        assert_eq!(plate_text("Émile Ünal", 0), "EMILE UNAL");
        assert_eq!(plate_text("   ", 2), "DJ 3");
        assert_eq!(plate_text(&"x".repeat(60), 0).chars().count(), MAX_CHARS);
    }

    #[test]
    fn le_texte_defile_vers_la_gauche_en_boucle() {
        let text = "AIFE";
        let width = loop_width(text);
        assert_eq!(scroll_offset(0.0, text), 0);
        assert!(scroll_offset(1.0, text) > 0);
        // Un tour complet ramène au point de départ.
        let period = width as f32 / SCROLL_PX_PER_S;
        assert!(scroll_offset(period, text) <= 1);
        assert!(scroll_offset(period * 7.0 + 0.5, text) < width);
    }

    #[test]
    fn la_plaque_dessine_le_nom_et_change_en_defilant() {
        let (mut a, mut b) = (buffer(), buffer());
        draw_plate(&mut a, "AIFE", 0.0, [0, 230, 255], [255, 13, 115]);
        draw_plate(&mut b, "AIFE", 0.5, [0, 230, 255], [255, 13, 115]);
        assert!(lit(&a) > 500, "{}", lit(&a));
        assert_ne!(a, b, "le texte a bougé");
        assert!(a.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn une_lettre_inconnue_laisse_une_place_vide_sans_paniquer() {
        let mut data = buffer();
        draw_plate(&mut data, "Ω☃", 0.0, [0, 230, 255], [255, 13, 115]);
        // Aucune lettre dans la couleur du texte : seuls les filets et le losange (couleur du second néon) restent.
        let letters = data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] < 50 && p[1] > 200)
            .count();
        assert_eq!(letters, 0);
    }

    #[test]
    fn la_police_couvre_les_lettres_et_chiffres() {
        for ch in ('A'..='Z').chain('0'..='9') {
            assert!(glyph(ch).is_some(), "{ch}");
        }
    }
}
