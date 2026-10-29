//! Display sanitiser for text that comes from the engine (and so, in the end,
//! from friends): names, messages, paths, error texts.
//!
//! The engine already sanitises display text (API.md); the app does it again
//! before anything reaches a label, so a name or a message can never reorder
//! or hide the text around it. Removed:
//!
//! - C0 and C1 control characters and DEL (a newline is kept in multi-line
//!   text, a tab becomes a space);
//! - bidirectional controls: ALM, LRM, RLM, LRE, RLE, PDF, LRO, RLO, LRI, RLI,
//!   FSI, PDI;
//! - zero-width and invisible characters: ZWSP, ZWNJ, ZWJ, word joiner and the
//!   invisible operators, BOM, soft hyphen, Mongolian vowel separator,
//!   deprecated format characters, interlinear annotation marks and Unicode
//!   tag characters.
//!
//! Labels are always filled with plain text (never Pango markup), so `<` and
//! `&` need no escaping here.

/// True for characters that must never reach the screen.
pub fn is_hidden_char(c: char) -> bool {
    matches!(c,
        '\u{0000}'..='\u{001F}'
        | '\u{007F}'..='\u{009F}'
        | '\u{00AD}'                // soft hyphen
        | '\u{061C}'                // Arabic letter mark
        | '\u{180E}'                // Mongolian vowel separator
        | '\u{200B}'..='\u{200F}'   // ZWSP, ZWNJ, ZWJ, LRM, RLM
        | '\u{202A}'..='\u{202E}'   // LRE, RLE, PDF, LRO, RLO
        | '\u{2060}'..='\u{2064}'   // word joiner, invisible operators
        | '\u{2066}'..='\u{206F}'   // LRI, RLI, FSI, PDI, deprecated format chars
        | '\u{FEFF}'                // BOM / zero-width no-break space
        | '\u{FFF9}'..='\u{FFFB}'   // interlinear annotation
        | '\u{E0000}'..='\u{E007F}' // tag characters
    )
}

/// Sanitise single-line text (names, statuses, paths, error messages).
/// Line breaks and tabs become one space; leading and trailing white space is
/// trimmed; runs of spaces created by the replacement are collapsed.
pub fn sanitize_inline(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = false;
    for c in input.chars() {
        let c = match c {
            '\n' | '\r' | '\t' | '\u{2028}' | '\u{2029}' | '\u{0085}' => ' ',
            c if is_hidden_char(c) => continue,
            c => c,
        };
        if c == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// Sanitise multi-line text (message bodies, technical details). Newlines are
/// kept (CR LF and the Unicode line and paragraph separators become `\n`),
/// tabs become a space, at most two empty lines in a row are kept.
pub fn sanitize_multiline(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut newlines = 0usize;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let c = match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                '\n'
            }
            '\u{2028}' | '\u{2029}' | '\u{0085}' => '\n',
            '\t' => ' ',
            '\n' => '\n',
            c if is_hidden_char(c) => continue,
            c => c,
        };
        if c == '\n' {
            newlines += 1;
            if newlines > 3 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(c);
    }
    out.trim_matches(|c: char| c == '\n' || c == ' ')
        .to_string()
}

/// Sanitise an optional single-line field in place.
pub fn inline_opt(value: &mut Option<String>) {
    if let Some(v) = value.as_mut() {
        *v = sanitize_inline(v);
    }
}

/// Sanitise a single-line field in place.
pub fn inline(value: &mut String) {
    *value = sanitize_inline(value);
}

/// Sanitise a multi-line field in place.
pub fn multiline(value: &mut String) {
    *value = sanitize_multiline(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(sanitize_inline("Ali"), "Ali");
        assert_eq!(
            sanitize_inline("Great. Can you bring the charger?"),
            "Great. Can you bring the charger?"
        );
        // Other scripts are text, not controls: they stay.
        assert_eq!(
            sanitize_inline("Mina \u{0645}\u{06CC}\u{0646}\u{0627}"),
            "Mina \u{0645}\u{06CC}\u{0646}\u{0627}"
        );
        // Emoji and variation selectors stay.
        assert_eq!(
            sanitize_inline("hi \u{2764}\u{FE0F}"),
            "hi \u{2764}\u{FE0F}"
        );
    }

    #[test]
    fn bidi_overrides_are_removed() {
        // The classic "right-to-left override" spoof.
        assert_eq!(sanitize_inline("Eve\u{202E}gnp.exe"), "Evegnp.exe");
        assert_eq!(
            sanitize_inline("\u{2066}a\u{2067}b\u{2068}c\u{2069}"),
            "abc"
        );
        assert_eq!(sanitize_inline("x\u{200E}y\u{200F}z\u{061C}"), "xyz");
        assert_eq!(sanitize_inline("\u{202A}\u{202B}\u{202C}\u{202D}ok"), "ok");
    }

    #[test]
    fn zero_width_and_invisible_are_removed() {
        assert_eq!(sanitize_inline("A\u{200B}l\u{200C}i\u{200D}"), "Ali");
        assert_eq!(sanitize_inline("\u{FEFF}Rena\u{2060}"), "Rena");
        assert_eq!(sanitize_inline("so\u{00AD}ft"), "soft");
        assert_eq!(sanitize_inline("tag\u{E0041}\u{E007F}"), "tag");
        assert_eq!(sanitize_inline("a\u{180E}b\u{FFF9}c"), "abc");
    }

    #[test]
    fn controls_are_removed_and_lines_joined_inline() {
        assert_eq!(
            sanitize_inline("a\u{0007}b\u{001B}[31mc\u{007F}\u{0085}d"),
            "ab[31mc d"
        );
        assert_eq!(
            sanitize_inline("  two\nlines\r\n\tand tab  "),
            "two lines and tab"
        );
        assert_eq!(sanitize_inline("a\u{2028}b\u{2029}c"), "a b c");
        assert_eq!(sanitize_inline("\u{009B}x"), "x");
    }

    #[test]
    fn multiline_keeps_newlines_only() {
        assert_eq!(
            sanitize_multiline("one\r\ntwo\rthree\u{2028}four"),
            "one\ntwo\nthree\nfour"
        );
        assert_eq!(sanitize_multiline("a\tb\u{202E}c\u{0000}"), "a bc");
        assert_eq!(sanitize_multiline("a\n\n\n\n\n\nb"), "a\n\n\nb");
        assert_eq!(sanitize_multiline("\n\n text \n\n"), "text");
    }

    #[test]
    fn in_place_helpers() {
        let mut s = String::from("Ali\u{202E}");
        inline(&mut s);
        assert_eq!(s, "Ali");
        let mut o = Some(String::from("\u{200B}x"));
        inline_opt(&mut o);
        assert_eq!(o.as_deref(), Some("x"));
        let mut m = String::from("l1\r\nl2\u{200F}");
        multiline(&mut m);
        assert_eq!(m, "l1\nl2");
    }
}
