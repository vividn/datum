//! Month abbreviations, by language (da-9rda8): what a table prints a date
//! in this year as (`Nov 6`, `Okt 3`), and what `-d` reads back.
//!
//! Each row is glibc's `abmon` for the language's main locale
//! (`/usr/share/i18n/locales`), so a table says what `date +%b` says there.
//! A language not listed is English. The table is ours rather than chrono's
//! `unstable-locales`: that is a new dependency (`pure-rust-locales`, every
//! glibc locale's every word) for these fifteen rows.

/// `(language, its twelve months)`, the language as a locale names it.
#[rustfmt::skip]
const MONTHS: &[(&str, [&str; 12])] = &[
    ("en", ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]),
    ("de", ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"]),
    ("fr", ["janv.", "févr.", "mars", "avril", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]),
    ("es", ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"]),
    ("it", ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"]),
    ("pt", ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"]),
    ("nl", ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"]),
    ("sv", ["jan", "feb", "mar", "apr", "maj", "jun", "jul", "aug", "sep", "okt", "nov", "dec"]),
    ("da", ["jan", "feb", "mar", "apr", "maj", "jun", "jul", "aug", "sep", "okt", "nov", "dec"]),
    ("nb", ["jan.", "feb.", "mars", "april", "mai", "juni", "juli", "aug.", "sep.", "okt.", "nov.", "des."]),
    ("nn", ["jan.", "feb.", "mars", "april", "mai", "juni", "juli", "aug.", "sep.", "okt.", "nov.", "des."]),
    ("no", ["jan.", "feb.", "mars", "april", "mai", "juni", "juli", "aug.", "sep.", "okt.", "nov.", "des."]),
    ("fi", ["tammi", "helmi", "maalis", "huhti", "touko", "kesä", "heinä", "elo", "syys", "loka", "marras", "joulu"]),
    ("pl", ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"]),
    ("cs", ["led", "úno", "bře", "dub", "kvě", "čen", "čec", "srp", "zář", "říj", "lis", "pro"]),
];

/// The language of a locale value as `LANG` spells it (`de_DE.UTF-8`) or a
/// browser does (`de-DE`): what comes before `_`, `-`, `.` or `@`.
pub fn language(locale: &str) -> &str {
    locale.split(['_', '-', '.', '@']).next().unwrap_or("")
}

/// Month `month` (1–12) as `locale`'s language abbreviates it; English for
/// a language the table lacks, `C` and `POSIX` among them.
pub fn month_abbr(locale: &str, month: u32) -> &'static str {
    let lang = language(locale).to_ascii_lowercase();
    let row = MONTHS
        .iter()
        .find(|(l, _)| *l == lang)
        .unwrap_or(&MONTHS[0]);
    row.1[(month as usize).clamp(1, 12) - 1]
}

/// The month a word names in any of the languages, case-insensitive, a
/// trailing `.` optional: `nov`, `Okt`, `mär`, `déc.`. No word names two
/// months (`no_word_names_two_months`), so no locale need be known.
pub fn month_from_word(word: &str) -> Option<u32> {
    let word = word.strip_suffix('.').unwrap_or(word).to_lowercase();
    MONTHS.iter().find_map(|(_, months)| {
        let i = months
            .iter()
            .position(|m| m.strip_suffix('.').unwrap_or(m).to_lowercase() == word)?;
        Some(i as u32 + 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_word_names_two_months() {
        for (lang, months) in MONTHS {
            for (i, m) in months.iter().enumerate() {
                assert_eq!(month_from_word(m), Some(i as u32 + 1), "{lang} {m}");
            }
        }
    }

    #[test]
    fn a_locale_picks_its_language_and_falls_back_to_english() {
        assert_eq!(month_abbr("de_DE.UTF-8", 10), "Okt");
        assert_eq!(month_abbr("de-DE", 3), "Mär");
        assert_eq!(month_abbr("C", 11), "Nov");
        assert_eq!(month_abbr("", 11), "Nov");
        assert_eq!(month_abbr("ja_JP.UTF-8", 11), "Nov");
        assert_eq!(month_from_word("OKT"), Some(10));
        assert_eq!(month_from_word("dec"), Some(12));
        assert_eq!(month_from_word("déc"), Some(12));
        assert_eq!(month_from_word("no"), None);
    }
}
