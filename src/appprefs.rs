//! The app's own preferences, kept on this computer by the app (not by
//! Raven's engine, which knows nothing about how a window looks): the theme
//! (System, Light or Dark) and "Show technical details". They live in a
//! small text file in the user's config folder
//! (`$XDG_CONFIG_HOME/io.github.raven_ashco.Raven/app.conf`), one
//! `key=value` per line; anything else in the file is ignored. Raven's
//! profile (`RAVEN_DATA_DIR`, `~/.raven`) is never touched.

use std::path::{Path, PathBuf};

/// Settings > Appearance > Theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Follow this computer's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn key(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// Its place in the segmented control (System, Light, Dark).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppPrefs {
    pub theme: ThemeChoice,
    /// Show the engine's exact path words and the technical lines.
    pub details: bool,
}

impl AppPrefs {
    pub fn parse(text: &str) -> Self {
        let mut p = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match (k.trim(), v.trim()) {
                ("theme", v) => p.theme = ThemeChoice::from_key(v).unwrap_or_default(),
                ("details", v) => p.details = v == "true",
                _ => {}
            }
        }
        p
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Raven for Linux: how the app looks (not Raven's settings).\ntheme={}\ndetails={}\n",
            self.theme.key(),
            self.details
        )
    }

    /// The file under a config folder.
    pub fn path_in(config_dir: &Path) -> PathBuf {
        config_dir.join(crate::APP_ID).join("app.conf")
    }

    /// Read the file; a missing or unreadable file is the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// Write the file (its folder is made when missing), replacing it whole.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let p = AppPrefs {
            theme: ThemeChoice::Dark,
            details: true,
        };
        assert_eq!(AppPrefs::parse(&p.to_text()), p);
        assert_eq!(AppPrefs::parse(""), AppPrefs::default());
        assert_eq!(
            AppPrefs::parse("theme=purple\ndetails=yes\nnoise\n"),
            AppPrefs::default()
        );
        assert_eq!(AppPrefs::parse(" theme = light ").theme, ThemeChoice::Light);
        assert_eq!(ThemeChoice::Dark.index(), 2);
    }

    #[test]
    fn file_in_a_folder_of_its_own() {
        let dir = std::env::temp_dir().join(format!("raven-appprefs-{}", std::process::id()));
        let path = AppPrefs::path_in(&dir);
        assert!(path.ends_with("io.github.raven_ashco.Raven/app.conf"));
        assert_eq!(AppPrefs::load(&path), AppPrefs::default(), "missing file");
        let p = AppPrefs {
            theme: ThemeChoice::Light,
            details: false,
        };
        p.save(&path).unwrap();
        assert_eq!(AppPrefs::load(&path), p);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
