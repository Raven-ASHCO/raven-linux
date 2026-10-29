//! Raven for Linux: the GTK 4 desktop app.
//!
//! Debug builds run on `MockRaven` (demo data, see README.md); release builds
//! use the real engine (`raven-client`), stubbed until that crate exists.

mod ui;

fn main() -> gtk::glib::ExitCode {
    ui::app::run()
}
