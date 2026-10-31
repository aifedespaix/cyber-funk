//! Injecteurs de pannes (feature `fault-injection`, jamais dans le binaire livré).
//!
//! Servent aux tests de J9-02 et au banc manuel : abîmer un fichier de look, faire paniquer un bloc. Chaque
//! repli du skill `live-safety` est vérifié avec l'un d'eux.

/// Façon d'abîmer le texte d'un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corruption {
    /// Fichier vide (écriture interrompue).
    Empty,
    /// Coupé au milieu (disque plein, copie interrompue).
    Truncated,
    /// Octets de remplissage au milieu (secteur défectueux).
    Garbled,
}

fn middle(text: &str) -> usize {
    let mut at = text.len() / 2;
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Renvoie `text` abîmé de la façon demandée.
#[must_use]
pub fn corrupt(text: &str, how: Corruption) -> String {
    match how {
        Corruption::Empty => String::new(),
        Corruption::Truncated => text[..middle(text)].to_owned(),
        Corruption::Garbled => {
            let mut garbled = text.to_owned();
            garbled.insert_str(middle(text), "\u{fffd}\u{0}}}{{ ,,, ");
            garbled
        }
    }
}

/// Panique tout de suite : simule le bug d'un système.
///
/// # Panics
///
/// Toujours, c'est son rôle.
#[allow(clippy::panic, reason = "injecteur de panne : paniquer est le but")]
pub fn panic_now() {
    panic!("panne injectée (fault-injection)");
}
