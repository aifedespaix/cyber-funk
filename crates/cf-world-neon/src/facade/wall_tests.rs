//! Le texte du shader de façade : la peinture murale n'est appelée que pour une pièce marquée, sans jamais suivre l'audio.
//! Pas de banc GPU (les shaders ne se compilent qu'avec un rendu) : on vérifie le texte assemblé.

use super::material::FACADE_SOURCE;

#[test]
fn le_shader_ne_peint_que_les_pieces_marquees() {
    assert!(
        FACADE_SOURCE.contains("fn paint_wall("),
        "facade_paint.wesl n'est pas concaténé"
    );
    assert!(
        FACADE_SOURCE.contains("if in.graffiti != 0u && !far {"),
        "la peinture doit être gardée par le tag et exclure les immeubles lointains"
    );
    assert!(FACADE_SOURCE.contains("out.graffiti = tag >> 27u;"));
}

#[test]
fn la_peinture_ne_suit_pas_l_audio() {
    // Règle 7 : ni pouls, ni basses, ni flash, ni transitoire dans la fonction de peinture.
    let paint = FACADE_SOURCE.split("fn paint_wall(").nth(1).unwrap_or_default();
    assert!(!paint.is_empty(), "paint_wall absent");
    for forbidden in ["pulse", "bass", "flash", "onset", "hat", "beat", "facade."] {
        assert!(!paint.contains(forbidden), "paint_wall lit « {forbidden} »");
    }
}
