//! Every user-facing text, from `desktop/design/strings.en.json` (embedded at
//! build time, parsed once at startup). That file is the only source: a text
//! the app needs is added there first, for all three desktop apps.
//!
//! Use [`tr`], [`trf`] and [`tr_list`] with literal keys only: a unit test
//! scans the sources for these calls and checks every key exists (so never
//! write a call with a quoted key inside a comment).

use serde_json::{Map, Value};
use std::sync::OnceLock;

/// The shared design strings.
pub const DESIGN_STRINGS_JSON: &str = include_str!("../core/desktop/design/strings.en.json");

pub struct Strings {
    design: Map<String, Value>,
}

impl Strings {
    /// Parse the embedded file. Panics only if it is malformed, which the
    /// unit tests catch at build time.
    pub fn load() -> Self {
        let design: Map<String, Value> = serde_json::from_str(DESIGN_STRINGS_JSON)
            .expect("desktop/design/strings.en.json is valid JSON");
        Self { design }
    }

    fn value(&self, key: &str) -> Option<&Value> {
        if key.starts_with('_') {
            return None; // "_note" and other comments
        }
        self.design.get(key)
    }

    pub fn has(&self, key: &str) -> bool {
        self.value(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.value(key).and_then(Value::as_str)
    }

    pub fn list(&self, key: &str) -> Vec<&str> {
        match self.value(key) {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

static STRINGS: OnceLock<Strings> = OnceLock::new();

/// The loaded strings (parsed on first use).
pub fn strings() -> &'static Strings {
    STRINGS.get_or_init(Strings::load)
}

/// The text for `key`. Keys are literals; a missing key shows the key itself
/// (and a debug build warns), so a gap is visible instead of silent.
pub fn tr(key: &'static str) -> &'static str {
    match strings().get(key) {
        Some(s) => s,
        None => {
            if cfg!(debug_assertions) {
                eprintln!("raven-desktop: missing string key {key}");
            }
            key
        }
    }
}

/// The text for `key` with `{placeholder}`s replaced.
pub fn trf(key: &'static str, args: &[(&str, &str)]) -> String {
    fill(tr(key), args)
}

/// A list value (for example the "Keep chats for" options).
pub fn tr_list(key: &'static str) -> Vec<&'static str> {
    strings().list(key)
}

/// Replace `{name}` style placeholders. Unknown placeholders stay as they are.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_design_file_parses() {
        let s = Strings::load();
        assert_eq!(s.get("app.name"), Some("Raven"));
        assert_eq!(
            s.list("settings.keepChats.options"),
            vec!["7 days", "30 days", "90 days", "Always"]
        );
        assert_eq!(s.list("settings.whenQuit.options").len(), 3);
        assert_eq!(s.list("time.weekdays").len(), 7);
        assert!(!s.has("_note"), "comments are not texts");
        assert!(!s.has("no.such.key"));
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            fill("In touch {when}", &[("when", "just now")]),
            "In touch just now"
        );
        assert_eq!(
            fill(
                "{n} waiting · until {time}",
                &[("n", "1"), ("time", "Tue 14:05")]
            ),
            "1 waiting · until Tue 14:05"
        );
        assert_eq!(fill("keep {unknown} and {", &[]), "keep {unknown} and {");
        assert_eq!(
            trf(
                "ticker.delivered",
                &[("name", "Ali"), ("path", "LAN direct"), ("secs", "0.4")]
            ),
            "Delivered to Ali via LAN direct in 0.4 s · receipt signed by Ali"
        );
    }

    /// Every key passed to the three lookup functions in the sources must
    /// exist in strings.en.json.
    #[test]
    fn every_key_used_in_the_sources_exists() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs(&src, &mut files);
        assert!(files.len() > 5, "found the sources");
        let s = Strings::load();
        let mut used = 0;
        let mut missing = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            for key in used_keys(&text) {
                used += 1;
                if !s.has(&key) {
                    missing.push(format!("{}: {key}", f.display()));
                }
            }
        }
        assert!(used > 50, "the scanner found the string calls ({used})");
        assert!(
            missing.is_empty(),
            "keys missing from strings.en.json:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn scanner_finds_calls() {
        // Escaped quotes keep this line itself out of the real scan.
        let text = "tr(\"a.b\"); trf(\"c.d\", &[]); tr_list(\"e\"); some_str(\"no\"); x.tr(\"y\")";
        assert_eq!(used_keys(text), vec!["a.b", "y", "c.d", "e"]);
    }

    fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                collect_rs(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }

    fn used_keys(text: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for pat in ["tr(\"", "trf(\"", "tr_list(\""] {
            let mut from = 0;
            while let Some(i) = text[from..].find(pat) {
                let at = from + i;
                let before = text[..at].chars().next_back();
                let ident_before = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
                let start = at + pat.len();
                if !ident_before {
                    if let Some(end) = text[start..].find('"') {
                        keys.push(text[start..start + end].to_string());
                    }
                }
                from = start;
            }
        }
        keys
    }
}
