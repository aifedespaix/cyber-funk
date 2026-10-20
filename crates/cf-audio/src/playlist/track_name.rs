//! Nom de fichier → artiste et titre propres (portage de `track-name.ts` de Sillage).
//!
//! Sillage passe par des expressions régulières ; ici on les déroule à la main pour ne pas tirer `regex` pour quatre
//! motifs. Chaque fonction cite le motif d'origine.

/// Longueur d'un identifiant de vidéo `YouTube` : « `[HW8Hu17MpWA]` » (11 caractères, sans espace).
const VIDEO_ID_LEN: usize = 11;

/// Séparateurs usuels entre l'artiste et le titre : « Artiste - Titre », « Artiste ~ Titre », « Artiste – Titre ».
const SEPARATORS: [char; 4] = ['-', '–', '—', '~'];

/// Nom de fichier (sans extension) → `(artiste, titre)` propres, au format « Artiste - Titre ». Sans artiste reconnu,
/// l'artiste reste vide et le nom entier sert de titre.
#[must_use]
pub fn parse_track_name(stem: &str) -> (String, String) {
    let cleaned = collapse_whitespace(&strip_video_ids(stem));
    match find_separator(&cleaned) {
        Some((start, end)) => (tidy(&cleaned[..start]), tidy(&cleaned[end..])),
        None => (String::new(), tidy(&cleaned)),
    }
}

/// `/\s*\[[A-Za-z0-9_-]{11}\]/g` : retire les identifiants de vidéo que les téléchargeurs ajoutent au nom, avec les
/// espaces qui les précèdent.
fn strip_video_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let is_id = after.len() > VIDEO_ID_LEN
            && after.as_bytes()[VIDEO_ID_LEN] == b']'
            && after.as_bytes()[..VIDEO_ID_LEN]
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-');
        if is_id {
            out.push_str(rest[..open].trim_end());
            rest = &after[VIDEO_ID_LEN + 1..];
        } else {
            out.push_str(&rest[..=open]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// `/\s+[-–—~]\s+/` : premier séparateur entouré d'espaces. Rend les bornes (en octets) du séparateur avec ses espaces.
fn find_separator(text: &str) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(_, c)) in chars.iter().enumerate() {
        if !SEPARATORS.contains(&c) {
            continue;
        }
        let before = chars[..i].iter().rev().take_while(|(_, ch)| ch.is_whitespace()).count();
        let after = chars[i + 1..].iter().take_while(|(_, ch)| ch.is_whitespace()).count();
        if before > 0 && after > 0 {
            let start = chars[i - before].0;
            let end = chars.get(i + 1 + after).map_or(text.len(), |&(pos, _)| pos);
            return Some((start, end));
        }
    }
    None
}

/// `.replace(/\s+/g, " ").trim()`.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Nettoyage d'un morceau du nom : identifiants, deux-points pleins, soulignés, espace avant une parenthèse collée.
fn tidy(text: &str) -> String {
    let base = strip_video_ids(text).replace('：', ":").replace('_', " ");
    // « DJ ESP(Woody) » → « DJ ESP (Woody) » : `/(\p{L}|\d)\(/gu`.
    let mut spaced = String::with_capacity(base.len() + 4);
    let mut previous: Option<char> = None;
    for c in base.chars() {
        if c == '(' && previous.is_some_and(|p| p.is_alphabetic() || p.is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(c);
        previous = Some(c);
    }
    tidy_case(&collapse_whitespace(&spaced))
}

/// Tout en majuscules (« RALLY HOUSE ») devient « Rally House » ; sinon seule la première lettre passe en capitale.
fn tidy_case(text: &str) -> String {
    let letters: String = text.chars().filter(|c| c.is_alphabetic()).collect();
    let shouting = letters.chars().count() > 3 && letters == letters.to_uppercase();
    let base = if shouting {
        // `/(^|[\s(\[-])(\p{L})/gu` : une lettre en début de texte ou après un espace, « ( », « [ » ou « - ».
        let mut out = String::with_capacity(text.len());
        let mut boundary = true;
        for c in text.to_lowercase().chars() {
            if boundary && c.is_alphabetic() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            boundary = c.is_whitespace() || matches!(c, '(' | '[' | '-');
        }
        out
    } else {
        text.to_owned()
    };
    let mut chars = base.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(stem: &str) -> (String, String) {
        parse_track_name(stem)
    }

    #[test]
    fn splits_artist_and_title() {
        assert_eq!(
            parse("Snabisch - Night Prowler"),
            ("Snabisch".into(), "Night Prowler".into())
        );
        assert_eq!(
            parse("enprogames - Robotic City"),
            ("Enprogames".into(), "Robotic City".into())
        );
    }

    #[test]
    fn first_separator_wins_and_variants_are_accepted() {
        assert_eq!(parse("A ~ B - C"), ("A".into(), "B - C".into()));
        assert_eq!(parse("Artiste – Titre"), ("Artiste".into(), "Titre".into()));
        assert_eq!(parse("Artiste — Titre"), ("Artiste".into(), "Titre".into()));
    }

    #[test]
    fn dash_without_spaces_is_not_a_separator() {
        assert_eq!(parse("lo-fi beat"), (String::new(), "Lo-fi beat".into()));
    }

    #[test]
    fn strips_video_ids_and_extra_spaces() {
        assert_eq!(
            parse("Artiste  -  Titre [HW8Hu17MpWA]"),
            ("Artiste".into(), "Titre".into())
        );
        // Un crochet qui n'est pas un identifiant reste.
        assert_eq!(parse("Titre [live]"), (String::new(), "Titre [live]".into()));
    }

    #[test]
    fn shouting_becomes_title_case() {
        assert_eq!(parse("DJ X - RALLY HOUSE"), ("DJ X".into(), "Rally House".into()));
        assert_eq!(
            parse("THE BAND (LIVE-MIX)"),
            (String::new(), "The Band (Live-Mix)".into())
        );
    }

    #[test]
    fn tidies_underscores_colons_and_parentheses() {
        assert_eq!(
            parse("dj_esp(Woody) - mix：one"),
            ("Dj esp (Woody)".into(), "Mix:one".into())
        );
    }

    #[test]
    fn empty_name_stays_empty() {
        assert_eq!(parse(""), (String::new(), String::new()));
    }
}
