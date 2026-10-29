//! Theme and motion for the GTK app.
//!
//! Colour scheme: plain GTK 4 (no libadwaita) does not follow the desktop's
//! light/dark preference by itself before GTK 4.20, so the app reads it from
//! the XDG desktop portal (`org.freedesktop.appearance` / `color-scheme`, the
//! setting GNOME, KDE and others publish), follows its `SettingChanged`
//! signal, and falls back to GTK's own settings (`gtk-theme-name` ending in
//! `-dark`, or `gtk-application-prefer-dark-theme=1` in settings.ini). The
//! chosen scheme selects the generated stylesheet and sets
//! `gtk-application-prefer-dark-theme` so GTK's built-in widgets match.
//!
//! Settings > Appearance > Theme (System, Light, Dark; the app's own file,
//! `appprefs`) can set the scheme instead of following the system.
//!
//! Motion: `gtk-enable-animations` (GNOME's Animations switch) plus Raven's
//! own Reduced motion setting. With either off, nothing moves: the
//! stylesheet is loaded without any animation or transition, Raven's own
//! Reduced motion also sets this process's `gtk-enable-animations` to false
//! (so GTK's own widgets stop moving too; it follows the desktop again when
//! the switch goes off), and the `reduced-motion` class on every window is a
//! backstop.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use raven_desktop::appprefs::{AppPrefs, ThemeChoice};
use raven_desktop::motion::{CubicBezier, MotionPolicy};
use raven_desktop::theme::{generate_css, tokens, Rgba, Scheme};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";

type Listener = dyn Fn() -> bool;

pub struct Style {
    provider: gtk::CssProvider,
    gtk_settings: Option<gtk::Settings>,
    scheme: Cell<Scheme>,
    portal_scheme: Cell<Option<Scheme>>,
    initial_prefer_dark: bool,
    app_reduced: Cell<bool>,
    /// This app set `gtk-enable-animations` to false (Raven's Reduced motion).
    overriding_animations: Cell<bool>,
    /// The stylesheet now loaded: scheme and motion.
    loaded: Cell<Option<(Scheme, bool)>>,
    prefs: RefCell<AppPrefs>,
    prefs_path: Option<PathBuf>,
    /// Called after a scheme, motion or appearance change; a listener that
    /// answers false (its owner is gone) is dropped.
    listeners: RefCell<Vec<Rc<Listener>>>,
    portal_subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Style {
    /// Install the stylesheet on the default display and start following
    /// the system colour scheme and animation setting.
    pub fn install() -> Rc<Self> {
        let gtk_settings = gtk::Settings::default();
        let initial_prefer_dark = gtk_settings
            .as_ref()
            .is_some_and(|s| s.is_gtk_application_prefer_dark_theme());
        let prefs_path = Some(AppPrefs::path_in(&glib::user_config_dir()));
        let prefs = prefs_path
            .as_deref()
            .map(AppPrefs::load)
            .unwrap_or_default();
        let style = Rc::new(Self {
            provider: gtk::CssProvider::new(),
            gtk_settings,
            scheme: Cell::new(Scheme::Light),
            portal_scheme: Cell::new(None),
            initial_prefer_dark,
            app_reduced: Cell::new(false),
            overriding_animations: Cell::new(false),
            loaded: Cell::new(None),
            prefs: RefCell::new(prefs),
            prefs_path,
            listeners: RefCell::new(Vec::new()),
            portal_subscription: RefCell::new(None),
        });
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &style.provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        style.refresh_scheme(true);
        if let Some(s) = &style.gtk_settings {
            let weak = Rc::downgrade(&style);
            s.connect_gtk_theme_name_notify(move |_| {
                if let Some(style) = weak.upgrade() {
                    style.refresh_scheme(false);
                }
            });
            let weak = Rc::downgrade(&style);
            s.connect_gtk_enable_animations_notify(move |_| {
                if let Some(style) = weak.upgrade() {
                    style.motion_changed();
                }
            });
        }
        watch_portal(&style);
        style
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme.get()
    }

    /// A token colour for the current scheme (for cairo drawing).
    pub fn color(&self, name: &str) -> Rgba {
        tokens().color(name, self.scheme()).unwrap_or(Rgba {
            r: 128,
            g: 128,
            b: 128,
            a: 255,
        })
    }

    pub fn motion(&self) -> MotionPolicy {
        MotionPolicy {
            system_animations: self
                .gtk_settings
                .as_ref()
                .is_none_or(|s| s.is_gtk_enable_animations()),
            app_reduced_motion: self.app_reduced.get(),
        }
    }

    pub fn motion_allowed(&self) -> bool {
        self.motion().allowed()
    }

    /// A transition length, or 0 under reduced motion.
    pub fn duration(&self, ms: u32) -> u32 {
        self.motion().duration(ms)
    }

    /// A named curve of the tokens (`lift`, `glide`, …).
    pub fn curve(&self, name: &str) -> CubicBezier {
        CubicBezier::named(name)
    }

    pub fn set_app_reduced_motion(&self, reduced: bool) {
        if self.app_reduced.replace(reduced) == reduced {
            return;
        }
        if let Some(s) = &self.gtk_settings {
            if reduced {
                // GTK's own widgets (switches, stacks, revealers, spinners)
                // stop moving in this app too.
                self.overriding_animations.set(true);
                s.set_gtk_enable_animations(false);
            } else if self.overriding_animations.replace(false) {
                // Follow the desktop's Animations setting again.
                s.reset_property("gtk-enable-animations");
            }
        }
        self.motion_changed();
    }

    /// Settings > Appearance > Theme.
    pub fn theme(&self) -> ThemeChoice {
        self.prefs.borrow().theme
    }

    pub fn set_theme(&self, theme: ThemeChoice) {
        if self.prefs.borrow().theme == theme {
            return;
        }
        self.prefs.borrow_mut().theme = theme;
        self.save_prefs();
        self.refresh_scheme(false);
    }

    /// Settings > Appearance > Show technical details.
    pub fn details(&self) -> bool {
        self.prefs.borrow().details
    }

    pub fn set_details(&self, on: bool) {
        if self.prefs.borrow().details == on {
            return;
        }
        self.prefs.borrow_mut().details = on;
        self.save_prefs();
        self.notify();
    }

    fn save_prefs(&self) {
        if let Some(path) = &self.prefs_path {
            if let Err(e) = self.prefs.borrow().save(path) {
                // Only how the app looks: keep going with the choice in memory.
                eprintln!("raven-desktop: could not save {}: {e}", path.display());
            }
        }
    }

    /// `f` runs after a scheme, motion or appearance change (redraw,
    /// re-apply classes) for as long as `owner` lives; then it is dropped.
    pub fn watch<T: 'static>(&self, owner: &Rc<T>, f: impl Fn(&Rc<T>) + 'static) {
        let weak = Rc::downgrade(owner);
        self.listeners
            .borrow_mut()
            .push(Rc::new(move || match weak.upgrade() {
                Some(o) => {
                    f(&o);
                    true
                }
                None => false,
            }));
    }

    /// Every app window gets the base class and the motion class.
    pub fn decorate_window(&self, window: &impl IsA<gtk::Widget>) {
        let w = window.upcast_ref::<gtk::Widget>();
        w.add_css_class("raven-window");
        self.apply_motion_class(w);
    }

    fn apply_motion_class(&self, w: &gtk::Widget) {
        if self.motion_allowed() {
            w.remove_css_class("reduced-motion");
        } else {
            w.add_css_class("reduced-motion");
        }
    }

    fn motion_changed(&self) {
        self.load_css();
        for w in gtk::Window::list_toplevels() {
            if w.has_css_class("raven-window") {
                self.apply_motion_class(&w);
            }
        }
        self.notify();
    }

    fn notify(&self) {
        // No borrow is held while listeners run: they may add listeners
        // (a new message row) or run code that does.
        let list: Vec<Rc<Listener>> = self.listeners.borrow().clone();
        let gone: Vec<Rc<Listener>> = list.into_iter().filter(|f| !f()).collect();
        if !gone.is_empty() {
            self.listeners
                .borrow_mut()
                .retain(|f| !gone.iter().any(|g| Rc::ptr_eq(f, g)));
        }
    }

    fn system_scheme(&self) -> Scheme {
        match self.prefs.borrow().theme {
            ThemeChoice::Light => return Scheme::Light,
            ThemeChoice::Dark => return Scheme::Dark,
            ThemeChoice::System => {}
        }
        if let Some(s) = self.portal_scheme.get() {
            return s;
        }
        let theme_is_dark = self
            .gtk_settings
            .as_ref()
            .and_then(|s| s.gtk_theme_name())
            .is_some_and(|n| n.to_ascii_lowercase().ends_with("-dark") || n.ends_with(":dark"));
        if theme_is_dark || self.initial_prefer_dark {
            Scheme::Dark
        } else {
            Scheme::Light
        }
    }

    fn refresh_scheme(&self, force: bool) {
        let scheme = self.system_scheme();
        if !force && scheme == self.scheme.get() {
            return;
        }
        self.scheme.set(scheme);
        self.load_css();
        if let Some(s) = &self.gtk_settings {
            let dark = scheme == Scheme::Dark;
            if s.is_gtk_application_prefer_dark_theme() != dark {
                s.set_gtk_application_prefer_dark_theme(dark);
            }
        }
        if !force {
            self.notify();
        }
    }

    /// Load the stylesheet for the current scheme and motion (only when
    /// one of them changed).
    fn load_css(&self) {
        let want = (self.scheme.get(), self.motion_allowed());
        if self.loaded.replace(Some(want)) == Some(want) {
            return;
        }
        // load_from_data is the GTK 4.0 call (deprecated only from 4.12).
        #[allow(deprecated)]
        self.provider
            .load_from_data(&generate_css(tokens(), want.0, want.1));
    }

    fn set_portal_scheme(&self, scheme: Option<Scheme>) {
        self.portal_scheme.set(scheme);
        self.refresh_scheme(false);
    }
}

/// Portal value: 0 no preference, 1 prefer dark, 2 prefer light. The value
/// may arrive wrapped in a tuple and one or two variants (`Read` vs
/// `ReadOne`), so unwrap until a `u32` appears.
fn scheme_from_variant(v: &glib::Variant) -> Option<Scheme> {
    let mut cur = v.clone();
    for _ in 0..4 {
        if let Some(n) = cur.get::<u32>() {
            return match n {
                1 => Some(Scheme::Dark),
                2 => Some(Scheme::Light),
                _ => None,
            };
        }
        if let Some(inner) = cur.as_variant() {
            cur = inner;
        } else if cur.is_container() && cur.n_children() > 0 {
            cur = cur.child_value(0);
        } else {
            break;
        }
    }
    None
}

fn watch_portal(style: &Rc<Style>) {
    let weak = Rc::downgrade(style);
    glib::spawn_future_local(async move {
        let Ok(conn) = gio::bus_get_future(gio::BusType::Session).await else {
            return;
        };
        let key = (APPEARANCE, COLOR_SCHEME).to_variant();
        let mut value = conn
            .call_future(
                Some(PORTAL_BUS),
                PORTAL_PATH,
                PORTAL_SETTINGS,
                "ReadOne",
                Some(&key),
                None,
                gio::DBusCallFlags::NONE,
                2_000,
            )
            .await;
        if value.is_err() {
            // Older portals only have the deprecated `Read`.
            value = conn
                .call_future(
                    Some(PORTAL_BUS),
                    PORTAL_PATH,
                    PORTAL_SETTINGS,
                    "Read",
                    Some(&key),
                    None,
                    gio::DBusCallFlags::NONE,
                    2_000,
                )
                .await;
        }
        let Some(style) = weak.upgrade() else { return };
        if let Ok(v) = value {
            style.set_portal_scheme(scheme_from_variant(&v));
        }
        let weak_inner = Rc::downgrade(&style);
        let subscription = conn.subscribe_to_signal(
            Some(PORTAL_BUS),
            Some(PORTAL_SETTINGS),
            Some("SettingChanged"),
            Some(PORTAL_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some(style) = weak_inner.upgrade() else {
                    return;
                };
                let p = signal.parameters;
                if p.n_children() != 3 {
                    return;
                }
                let namespace = p.child_value(0);
                let name = p.child_value(1);
                if namespace.str() == Some(APPEARANCE) && name.str() == Some(COLOR_SCHEME) {
                    style.set_portal_scheme(scheme_from_variant(&p.child_value(2)));
                }
            },
        );
        style.portal_subscription.replace(Some(subscription));
    });
}
