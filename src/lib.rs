//! GTK-free core of the Raven desktop app for Linux.
//!
//! Everything here builds and tests without the GTK system libraries
//! (`cargo test --lib --no-default-features`): the engine contract, its mock
//! and the engine thread, strings, design tokens and the generated CSS, the
//! display sanitiser, time texts and the honest-motion policy. The GTK front end lives in the
//! `raven-desktop` binary (`src/main.rs`, `src/ui/`).

pub mod api;
pub mod appprefs;
pub mod client;
pub mod groupview;
pub mod icons;
pub mod mark;
#[cfg(debug_assertions)]
pub mod mock;
pub mod motion;
pub mod netflow;
pub mod netview;
pub mod present;
pub mod routesview;
pub mod sanitize;
pub mod secview;
pub mod settingsview;
pub mod strings;
pub mod theme;
pub mod timefmt;
pub mod tray;
pub mod words;
pub mod worker;

/// Application id (reverse DNS), also the icon name, the .desktop file name
/// and the D-Bus name. Linux spelling of the one Raven app id
/// (desktop/README.md, "App identity"): Flathub turns the `-` of the GitHub
/// organisation into `_`; macOS and Windows use `io.github.raven-ashco.Raven`.
pub const APP_ID: &str = "io.github.raven_ashco.Raven";
