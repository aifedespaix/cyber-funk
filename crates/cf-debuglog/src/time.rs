//! Horodatage ISO 8601 en UTC, sans dépendance : le journal doit rester lisible par n'importe quel outil.

use std::time::{SystemTime, UNIX_EPOCH};

/// Millisecondes écoulées depuis l'époque Unix (0 si l'horloge est avant 1970).
#[must_use]
pub fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// `2026-11-07T14:03:22.481Z` pour un nombre de millisecondes depuis l'époque Unix.
#[must_use]
pub fn iso_utc(epoch_ms: u64) -> String {
    let secs = epoch_ms / 1000;
    let ms = epoch_ms % 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Version compacte pour les noms de dossier : `20261107-140322`.
#[must_use]
pub fn compact_utc(epoch_ms: u64) -> String {
    iso_utc(epoch_ms)
        .chars()
        .filter(char::is_ascii_digit)
        .take(14)
        .enumerate()
        .flat_map(|(i, c)| if i == 8 { vec!['-', c] } else { vec![c] })
        .collect()
}

/// Algorithme de Howard Hinnant : jours depuis 1970-01-01 vers (année, mois, jour) du calendrier grégorien.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_epoque_et_une_date_connue() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000Z");
        // 2026-11-07T14:03:22.481Z
        assert_eq!(iso_utc(1_794_060_202_481), "2026-11-07T14:03:22.481Z");
        assert_eq!(compact_utc(1_794_060_202_481), "20261107-140322");
    }

    #[test]
    fn le_29_fevrier_d_une_annee_bissextile() {
        // 2024-02-29T00:00:00Z
        assert_eq!(iso_utc(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
    }
}
