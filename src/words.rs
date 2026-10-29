//! Typing a friend's security words (the "Check words" field).
//!
//! The app only splits what is typed into tokens; whether a token is one of
//! Raven's words comes from the engine (`verify.word_hint`) and whether the
//! ten words fit comes from `verify.check_words`. Nothing here knows the
//! friend's words, so nothing here can mark a word right or wrong.
//!
//! Input tolerance (security-words design, "Tokens"): separators include
//! white space and common punctuation; Persian, Arabic-Indic and full-width
//! digits fold to ASCII and tokens made only of digits are position numbers
//! ("1. turtle 2. frog") and are dropped; zero-width characters inside a token
//! are dropped; full-width Latin letters fold to a-z.

/// Number of security words in a set.
pub const WORD_COUNT: usize = 10;
/// Letters after which the engine completes a word.
pub const MIN_PREFIX: usize = 3;

fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '-' | '\u{2010}'
                | '\u{2011}'
                | '\u{2012}'
                | '\u{2013}'
                | '\u{2014}'
                | '\u{2015}'
                | '\u{2212}'
                | ','
                | ';'
                | '.'
                | '\u{00B7}'
                | '\u{2022}'
                | '/'
                | '|'
                | ':'
                | '('
                | ')'
                | '['
                | ']'
                | '"'
                | '\''
                | '`'
                | '\u{060C}' // Arabic comma
                | '\u{061B}' // Arabic semicolon
        )
}

fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}' | '\u{061C}' | '\u{00AD}'
    )
}

/// Fold one character: other-script digits to ASCII, full-width and ASCII
/// capitals to lower case.
fn fold(c: char) -> char {
    match c {
        '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
        '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
        '\u{FF10}'..='\u{FF19}' => char::from(b'0' + (c as u32 - 0xFF10) as u8),
        '\u{FF21}'..='\u{FF3A}' => char::from(b'a' + (c as u32 - 0xFF21) as u8),
        '\u{FF41}'..='\u{FF5A}' => char::from(b'a' + (c as u32 - 0xFF41) as u8),
        c => c.to_ascii_lowercase(),
    }
}

/// Normalise one token; `None` when it is a position number or empty.
fn normalise(token: &str) -> Option<String> {
    let t: String = token
        .chars()
        .filter(|c| !is_invisible(*c))
        .map(fold)
        .collect();
    if t.is_empty() || t.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(t)
    }
}

/// The old letters-and-digits code as typed, ready to compare: separators
/// and white space dropped, invisible characters dropped, Persian,
/// Arabic-Indic and full-width digits folded to ASCII. Letters keep their
/// case: the old code is case-sensitive (security-words design, "Old code").
pub fn normalise_old_code(code: &str) -> String {
    code.chars()
        .filter(|c| !is_separator(*c) && !is_invisible(*c))
        .map(|c| match c {
            '\u{06F0}'..='\u{06F9}' | '\u{0660}'..='\u{0669}' | '\u{FF10}'..='\u{FF19}' => fold(c),
            c => c,
        })
        .collect()
}

/// Your own old code as shown to read aloud: groups of four separated by
/// spaces, as in the security-words design ("If4x-36FU-omFi" becomes
/// "If4x 36FU omFi"). Only the display changes: letters keep their case, and
/// the engine ignores the spaces when a friend types the code.
pub fn group_old_code(code: &str) -> String {
    let key: Vec<char> = normalise_old_code(code).chars().collect();
    key.chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The intro of Check words: how to compare the words, or the old code.
pub fn check_intro_key(old_code: bool) -> &'static str {
    if old_code {
        "check.oldCode.intro"
    } else {
        "check.body"
    }
}

/// Split the field into finished tokens and the token still being typed.
/// A token is finished when a separator follows it.
pub fn split_entry(text: &str) -> (Vec<String>, String) {
    let mut done = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_separator(c) {
            if let Some(t) = normalise(&current) {
                done.push(t);
            }
            current.clear();
        } else {
            current.push(c);
        }
    }
    let partial = normalise(&current).unwrap_or_default();
    (done, partial)
}

/// What the field should say about one token, from the typed text alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenShape {
    /// a-z only and at least three letters: ask the engine.
    Lookup,
    /// a-z only but fewer than three letters: keep typing.
    TooShort,
    /// Other letters: "Type the words with English letters."
    NotLatin,
}

pub fn token_shape(token: &str) -> TokenShape {
    if !token.chars().all(|c| c.is_ascii_lowercase()) {
        TokenShape::NotLatin
    } else if token.chars().count() < MIN_PREFIX {
        TokenShape::TooShort
    } else {
        TokenShape::Lookup
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_spaces_and_punctuation() {
        assert_eq!(
            split_entry("turtle frog "),
            (vec!["turtle".into(), "frog".into()], String::new())
        );
        assert_eq!(
            split_entry("turtle, frog;bread tur"),
            (
                vec!["turtle".into(), "frog".into(), "bread".into()],
                "tur".into()
            )
        );
        assert_eq!(
            split_entry("turtle\u{2014}frog\u{00A0}x"),
            (vec!["turtle".into(), "frog".into()], "x".into())
        );
        assert_eq!(split_entry(""), (vec![], String::new()));
    }

    #[test]
    fn position_numbers_are_dropped_in_any_digits() {
        assert_eq!(
            split_entry("1. turtle 2. frog 3."),
            (vec!["turtle".into(), "frog".into()], String::new())
        );
        // Persian and Arabic-Indic digits.
        assert_eq!(
            split_entry("\u{06F1} turtle \u{0662} frog "),
            (vec!["turtle".into(), "frog".into()], String::new())
        );
        assert_eq!(
            split_entry("\u{FF13}) wolf "),
            (vec!["wolf".into()], String::new())
        );
    }

    #[test]
    fn folding_and_invisible_characters() {
        assert_eq!(
            split_entry("TURTLE Fr\u{200B}og "),
            (vec!["turtle".into(), "frog".into()], String::new())
        );
        assert_eq!(
            split_entry("\u{FF34}\u{FF35}\u{FF32}"),
            (vec![], "tur".into())
        );
    }

    #[test]
    fn old_code_normalising() {
        assert_eq!(normalise_old_code("If4x-36FU-omFi"), "If4x36FUomFi");
        assert_eq!(
            normalise_old_code(" If4x 36FU\u{00A0}omFi "),
            "If4x36FUomFi"
        );
        assert_eq!(
            normalise_old_code("If\u{06F4}x\u{2013}\u{0663}\u{06F6}FU\u{200C}omFi"),
            "If4x36FUomFi",
            "Persian and Arabic-Indic digits fold, dashes and ZWNJ drop"
        );
        assert_eq!(normalise_old_code("\u{FF13}\u{FF16}"), "36");
        assert_eq!(normalise_old_code("if4x"), "if4x", "case is kept");
        assert_eq!(normalise_old_code(" - . "), "");
    }

    #[test]
    fn old_code_is_shown_in_groups_of_four() {
        assert_eq!(group_old_code("If4x-36FU-omFi"), "If4x 36FU omFi");
        assert_eq!(group_old_code("If4x36FUomFi"), "If4x 36FU omFi");
        assert_eq!(
            group_old_code(" If4x - 36FU\u{200B}omFi "),
            "If4x 36FU omFi"
        );
        assert_eq!(group_old_code("Tq7m-29Xa-hKe"), "Tq7m 29Xa hKe");
        assert_eq!(group_old_code(""), "");
        // What is shown is what a friend may type: the same key, case kept.
        assert_eq!(
            normalise_old_code(&group_old_code("If4x-36FU-omFi")),
            "If4x36FUomFi"
        );
    }

    #[test]
    fn old_code_mode_has_its_own_intro() {
        use crate::strings::trf;
        let words = trf(check_intro_key(false), &[("name", "Rena")]);
        let code = trf(check_intro_key(true), &[("name", "Rena")]);
        assert!(words.contains("your own words"));
        assert!(code.contains("old code") && !code.contains("your own words"));
        assert!(code.contains("Rena"));
    }

    #[test]
    fn token_shapes() {
        assert_eq!(token_shape("tur"), TokenShape::Lookup);
        assert_eq!(token_shape("tu"), TokenShape::TooShort);
        assert_eq!(
            token_shape("\u{0644}\u{0627}\u{06A9}"),
            TokenShape::NotLatin
        );
        assert_eq!(token_shape("caf\u{00E9}"), TokenShape::NotLatin);
    }
}
