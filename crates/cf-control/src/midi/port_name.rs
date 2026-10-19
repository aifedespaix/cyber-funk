//! Correspondance entre le nom d'un port MIDI écrit dans un profil et le nom que l'hôte donne au port branché.
//!
//! Le même contrôleur s'appelle différemment selon la machine, la prise USB et le moment :
//!
//! | Hôte | Nom vu | Ce qui change |
//! | --- | --- | --- |
//! | Linux (ALSA) | `DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0` | les numéros `28:0` (client:port) à chaque branchement |
//! | Windows | `2- Launchpad`, `MIDIIN2 (Launchpad)` | le rang devant le nom, selon la prise USB |
//! | macOS | `DDJ-FLX4` | rien |
//!
//! L'égalité exacte ferait perdre le profil à chaque changement de prise. La correspondance ci-dessous ne touche
//! jamais au tas : elle compare des tranches du nom d'origine, car elle sert dans [`Mapper::map`](crate::Mapper::map).

/// La part stable d'un nom de port : sans les numéros `client:port` d'ALSA, sans le rang `N- ` ou l'enveloppe
/// `MIDIINn (…)` de Windows. C'est ce nom que le MIDI learn écrit dans les profils.
#[must_use]
pub fn canonical_port_name(name: &str) -> &str {
    let mut s = name.trim();
    // Windows ancien : « MIDIIN2 (Launchpad) ».
    if let Some(inner) = windows_wrapped(s) {
        s = inner.trim();
    }
    // Windows : « 2- Launchpad ».
    if let Some((rank, rest)) = s.split_once("- ")
        && !rank.is_empty()
        && rank.bytes().all(|b| b.is_ascii_digit())
    {
        s = rest.trim_start();
    }
    // ALSA : « … MIDI 1 28:0 ».
    if let Some((head, tail)) = s.rsplit_once(' ')
        && let Some((client, port)) = tail.split_once(':')
        && is_number(client)
        && is_number(port)
    {
        s = head.trim_end();
    }
    s
}

fn windows_wrapped(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("MIDIIN").or_else(|| s.strip_prefix("MIDIOUT"))?;
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    rest.strip_prefix(" (")?.strip_suffix(')')
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// À quel point un nom de profil correspond à un nom de port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortMatch {
    /// Mêmes noms une fois nettoyés (casse ignorée).
    Exact,
    /// Le nom du profil est contenu, mot entier, dans le nom du port (`DDJ-FLX4` dans `DDJ-FLX4:DDJ-FLX4 MIDI 1`).
    Contained,
}

/// Le nom du profil `wanted` désigne-t-il le port `actual` ? Aucun nom vide ne correspond à rien.
///
/// Limite connue : `X1` correspond aussi à `X1 MK2`, puisque le mot y figure. Quand plusieurs ports conviennent,
/// l'appelant qui choisit un port (ouverture) préfère [`PortMatch::Exact`] ; un binding, lui, accepte les deux.
#[must_use]
pub fn match_port_name(wanted: &str, actual: &str) -> Option<PortMatch> {
    let wanted = canonical_port_name(wanted);
    let actual = canonical_port_name(actual);
    if wanted.is_empty() {
        return None;
    }
    if wanted.eq_ignore_ascii_case(actual) {
        Some(PortMatch::Exact)
    } else if contains_word(actual, wanted) {
        Some(PortMatch::Contained)
    } else {
        None
    }
}

/// Raccourci de [`match_port_name`] quand le degré de correspondance n'importe pas.
#[must_use]
pub fn port_names_match(wanted: &str, actual: &str) -> bool {
    match_port_name(wanted, actual).is_some()
}

/// `needle` figure-t-il dans `hay` comme suite de mots entiers, casse ASCII ignorée ? Sans allocation.
fn contains_word(hay: &str, needle: &str) -> bool {
    let (hay, needle) = (hay.as_bytes(), needle.as_bytes());
    if needle.is_empty() {
        return false;
    }
    // Les octets ≥ 0x80 (UTF-8) comptent comme des lettres : on ne coupe jamais au milieu d'un caractère.
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b >= 0x80;
    hay.windows(needle.len()).enumerate().any(|(i, window)| {
        window.eq_ignore_ascii_case(needle)
            && (i == 0 || !is_word(hay[i - 1]))
            && hay.get(i + needle.len()).is_none_or(|&b| !is_word(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_strips_host_decorations() {
        assert_eq!(canonical_port_name("DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0"), "DDJ-FLX4:DDJ-FLX4 MIDI 1");
        assert_eq!(canonical_port_name("2- Launchpad"), "Launchpad");
        assert_eq!(canonical_port_name("MIDIIN2 (Launchpad)"), "Launchpad");
        assert_eq!(canonical_port_name("  MPK mini 3  "), "MPK mini 3");
        assert_eq!(canonical_port_name("Pad 1: démo"), "Pad 1: démo");
    }

    #[test]
    fn same_device_on_another_port_still_matches() {
        let alsa_before = "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0";
        let alsa_after = "DDJ-FLX4:DDJ-FLX4 MIDI 1 32:0";
        // Le nom écrit dans le profil par le MIDI learn, puis celui de la machine après rebranchement.
        let learned = canonical_port_name(alsa_before);
        assert_eq!(match_port_name(learned, alsa_after), Some(PortMatch::Exact));
        assert_eq!(match_port_name("2- Launchpad", "Launchpad"), Some(PortMatch::Exact));
        assert_eq!(match_port_name("launchpad", "MIDIIN3 (Launchpad)"), Some(PortMatch::Exact));
    }

    #[test]
    fn short_profile_name_matches_longer_host_name_by_whole_words() {
        assert_eq!(match_port_name("DDJ-FLX4", alsa()), Some(PortMatch::Contained));
        assert_eq!(match_port_name("X1", "Traktor Kontrol X1 MK2"), Some(PortMatch::Contained));
        // Un morceau de mot ne suffit pas.
        assert_eq!(match_port_name("X1", "Traktor Kontrol X10"), None);
        assert_eq!(match_port_name("LAUNCH", "Launchpad"), None);
    }

    fn alsa() -> &'static str {
        "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0"
    }

    #[test]
    fn unrelated_and_empty_names_never_match() {
        assert!(!port_names_match("Launchpad", "DDJ-FLX4"));
        assert!(!port_names_match("", "Launchpad"));
        assert!(!port_names_match("   ", "Launchpad"));
        assert!(!port_names_match("Launchpad", ""));
    }

    #[test]
    fn exact_ranks_before_contained() {
        assert!(PortMatch::Exact < PortMatch::Contained);
    }
}
