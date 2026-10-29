//! The tray icon: a StatusNotifierItem (`org.kde.StatusNotifierItem`) with a
//! `com.canonical.dbusmenu` menu, exported on the app's own session-bus
//! connection with gio's D-Bus API (no extra crate). KDE Plasma, Xfce,
//! Cinnamon, MATE, LXQt, Budgie and waybar show it; GNOME shows it with the
//! AppIndicator extension.
//!
//! What the menu says and does is `raven_desktop::tray` (GTK-free, tested);
//! this file only turns it into GLib variants and back. Clicks are handled
//! after the D-Bus call returned (`idle_add_local_once`).
//!
//! Without a StatusNotifierWatcher on the bus (or without a host that shows
//! items), nothing is shown and the app works as before: closing the window
//! follows Settings › When I quit, and quitting says that Raven keeps
//! receiving. If the watcher goes away while the window is hidden, the
//! window comes back.

use gtk::gdk_pixbuf;
use gtk::gio;
use gtk::glib;
use gtk::glib::variant::ObjectPath;
use gtk::prelude::*;
use raven_desktop::strings::tr;
use raven_desktop::tray::{self, dbus, MenuState, Prop, TrayAction, ROOT_ID};
use raven_desktop::APP_ID;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::app::Ctx;

/// The app icon (the same picture as the installed hicolor icon), for hosts
/// that cannot find `IconName` in the icon theme.
const ICON_PNG: &[u8] =
    include_bytes!("../../data/icons/hicolor/64x64/apps/io.github.raven_ashco.Raven.png");

const ERROR_INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";
const ERROR_UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

pub struct Tray {
    ctx: Weak<Ctx>,
    me: Weak<Tray>,
    conn: gio::DBusConnection,
    state: RefCell<MenuState>,
    /// A watcher accepted the item and a host shows it.
    shown: Cell<bool>,
    watcher_up: Cell<bool>,
    name_owned: Cell<bool>,
    icon_pixmap: glib::Variant,
    // Kept for the life of the app (the tray lives as long as it).
    _registrations: RefCell<Vec<gio::RegistrationId>>,
    _owner: RefCell<Option<gio::OwnerId>>,
    _host_signal: RefCell<Option<gio::SignalSubscription>>,
}

impl Tray {
    /// Export the item and its menu and look for a watcher. `None` when the
    /// app has no session bus (then there is simply no tray).
    pub fn start(ctx: &Rc<Ctx>) -> Option<Rc<Tray>> {
        let conn = ctx.app.dbus_connection()?;
        let info = gio::DBusNodeInfo::for_xml(dbus::INTROSPECTION).ok()?;
        let item_iface = info.lookup_interface(dbus::ITEM_INTERFACE)?;
        let menu_iface = info.lookup_interface(dbus::MENU_INTERFACE)?;
        let tray = Rc::new_cyclic(|me| Tray {
            ctx: Rc::downgrade(ctx),
            me: me.clone(),
            conn: conn.clone(),
            state: RefCell::new(MenuState::default()),
            shown: Cell::new(false),
            watcher_up: Cell::new(false),
            name_owned: Cell::new(false),
            icon_pixmap: icon_pixmap(),
            _registrations: RefCell::new(Vec::new()),
            _owner: RefCell::new(None),
            _host_signal: RefCell::new(None),
        });
        tray.refresh();

        let (t1, t2) = (tray.clone(), tray.clone());
        let item = conn
            .register_object(dbus::ITEM_PATH, &item_iface)
            .method_call(move |_, _, _, _, method, _params, invocation| {
                t1.item_method(method, invocation)
            })
            .property(move |_, _, _, _, name| t2.item_property(name))
            .build()
            .ok()?;
        let (t1, t2) = (tray.clone(), tray.clone());
        let menu = conn
            .register_object(dbus::MENU_PATH, &menu_iface)
            .method_call(move |_, _, _, _, method, params, invocation| {
                t1.menu_method(method, &params, invocation)
            })
            .property(move |_, _, _, _, name| t2.menu_property(name))
            .build()
            .ok()?;
        tray._registrations.replace(vec![item, menu]);

        // Our own item name; registering needs it and a watcher.
        let (t1, t2) = (tray.clone(), tray.clone());
        let owner = gio::bus_own_name_on_connection(
            &conn,
            &dbus::item_name(std::process::id()),
            gio::BusNameOwnerFlags::NONE,
            move |_, _| {
                t1.name_owned.set(true);
                t1.register_with_watcher();
            },
            move |_, _| {
                t2.name_owned.set(false);
                t2.set_shown(false);
            },
        );
        tray._owner.replace(Some(owner));

        // The watcher may come and go (a panel restarts): register again.
        // The watch lasts as long as the app (its id is never unwatched).
        let (t1, t2) = (tray.clone(), tray.clone());
        gio::bus_watch_name_on_connection(
            &conn,
            dbus::WATCHER_NAME,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| {
                t1.watcher_up.set(true);
                t1.register_with_watcher();
            },
            move |_, _| {
                t2.watcher_up.set(false);
                t2.set_shown(false);
            },
        );

        // A host (the panel that draws items) may start after us.
        let t1 = tray.clone();
        let host = conn.subscribe_to_signal(
            Some(dbus::WATCHER_NAME),
            Some(dbus::WATCHER_INTERFACE),
            Some("StatusNotifierHostRegistered"),
            Some(dbus::WATCHER_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_| {
                if t1.watcher_up.get() && t1.name_owned.get() {
                    t1.set_shown(true);
                }
            },
        );
        tray._host_signal.replace(Some(host));
        Some(tray)
    }

    /// True while a panel shows the icon: closing the window hides Raven
    /// to it instead of quitting.
    pub fn shown(&self) -> bool {
        self.shown.get()
    }

    /// Recompute the menu from the app's state; emits `LayoutUpdated` and
    /// `NewToolTip` only when something changed. True when the menu changed.
    pub fn refresh(&self) -> bool {
        let Some(ctx) = self.ctx.upgrade() else {
            return false;
        };
        let input = ctx.tray_input();
        let (changed, revision, tooltip_changed) = {
            let mut s = self.state.borrow_mut();
            let changed = s.update(tray::menu(&input));
            let tooltip_changed = s.set_tooltip(tray::status_line(&input.node));
            (changed, s.revision(), tooltip_changed)
        };
        if changed {
            self.emit(
                dbus::MENU_PATH,
                dbus::MENU_INTERFACE,
                "LayoutUpdated",
                Some(&(revision, ROOT_ID).to_variant()),
            );
        }
        if tooltip_changed {
            self.emit(dbus::ITEM_PATH, dbus::ITEM_INTERFACE, "NewToolTip", None);
        }
        changed
    }

    fn emit(&self, path: &str, interface: &str, signal: &str, args: Option<&glib::Variant>) {
        // Fails only when the connection is closed (the app is quitting).
        let _ = self.conn.emit_signal(None, path, interface, signal, args);
    }

    fn set_shown(&self, shown: bool) {
        if self.shown.replace(shown) == shown {
            return;
        }
        let ctx = self.ctx.clone();
        glib::idle_add_local_once(move || {
            if let Some(ctx) = ctx.upgrade() {
                ctx.tray_shown_changed(shown);
            }
        });
    }

    /// `RegisterStatusNotifierItem` once we own our name and a watcher is
    /// there; then ask whether a host shows items at all.
    fn register_with_watcher(&self) {
        if !(self.watcher_up.get() && self.name_owned.get()) {
            return;
        }
        let me = self.me.clone();
        self.conn.call(
            Some(dbus::WATCHER_NAME),
            dbus::WATCHER_PATH,
            dbus::WATCHER_INTERFACE,
            "RegisterStatusNotifierItem",
            Some(&(dbus::item_name(std::process::id()),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            move |result| {
                let Some(t) = me.upgrade() else { return };
                match result {
                    Ok(_) => t.check_host(),
                    Err(_) => t.set_shown(false),
                }
            },
        );
    }

    fn check_host(&self) {
        let me = self.me.clone();
        self.conn.call(
            Some(dbus::WATCHER_NAME),
            dbus::WATCHER_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(dbus::WATCHER_INTERFACE, "IsStatusNotifierHostRegistered").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            move |result| {
                let Some(t) = me.upgrade() else { return };
                // A watcher without the property: assume its host shows us.
                let host = result
                    .ok()
                    .and_then(|v| v.child_value(0).as_variant())
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(true);
                t.set_shown(host);
            },
        );
    }

    /// Run a menu action after the D-Bus call returned.
    fn act(&self, action: TrayAction) {
        let ctx = self.ctx.clone();
        glib::idle_add_local_once(move || {
            if let Some(ctx) = ctx.upgrade() {
                ctx.tray_action(action);
            }
        });
    }

    // ------------------------------------------------------------ the item

    fn item_method(&self, method: &str, invocation: gio::DBusMethodInvocation) {
        match method {
            // A click on the icon opens Raven; the host shows the menu itself
            // on ContextMenu (from the Menu property); scrolling does nothing.
            "Activate" | "SecondaryActivate" => self.act(TrayAction::Open),
            "ContextMenu" | "Scroll" => {}
            _ => return invocation.return_dbus_error(ERROR_UNKNOWN_METHOD, method),
        }
        invocation.return_value(None);
    }

    fn item_property(&self, name: &str) -> glib::Variant {
        match name {
            "Category" => "Communications".to_variant(),
            "Id" => APP_ID.to_variant(),
            "Title" => tr("app.name").to_variant(),
            "Status" => "Active".to_variant(),
            "WindowId" => 0i32.to_variant(),
            "IconName" => APP_ID.to_variant(),
            "IconThemePath" => icon_theme_path().to_variant(),
            "IconPixmap" => self.icon_pixmap.clone(),
            "ToolTip" => glib::Variant::tuple_from_iter([
                "".to_variant(),
                no_pixmaps(),
                tr("app.name").to_variant(),
                self.state.borrow().tooltip().to_variant(),
            ]),
            "ItemIsMenu" => false.to_variant(),
            "Menu" => ObjectPath::try_from(dbus::MENU_PATH)
                .expect("a valid object path")
                .to_variant(),
            // OverlayIconName, AttentionIconName (GLib asks only for
            // properties in the introspection data, all of them strings).
            _ => "".to_variant(),
        }
    }

    // ------------------------------------------------------------ the menu

    fn menu_method(
        &self,
        method: &str,
        params: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        match method {
            "GetLayout" => {
                let Some((parent, depth, names)) = params.get::<(i32, i32, Vec<String>)>() else {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, method);
                };
                let s = self.state.borrow();
                if !s.has(parent) {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, "no such item");
                }
                // A flat menu: only the root has children (depth 0: none
                // asked for; -1: all).
                let children = if parent == ROOT_ID && depth != 0 {
                    s.ids()
                        .into_iter()
                        .map(|id| layout_node(&s, id, &names, &[]))
                        .collect()
                } else {
                    Vec::new()
                };
                let layout = layout_node(&s, parent, &names, &children);
                invocation.return_value(Some(&glib::Variant::tuple_from_iter([
                    s.revision().to_variant(),
                    layout,
                ])));
            }
            "GetGroupProperties" => {
                let Some((ids, names)) = params.get::<(Vec<i32>, Vec<String>)>() else {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, method);
                };
                let s = self.state.borrow();
                let ids = if ids.is_empty() { s.ids() } else { ids };
                let entries: Vec<glib::Variant> = ids
                    .into_iter()
                    .filter(|id| s.has(*id))
                    .map(|id| {
                        glib::Variant::tuple_from_iter([
                            id.to_variant(),
                            props_dict(s.properties(id, &names)),
                        ])
                    })
                    .collect();
                let ty = glib::VariantTy::new("(ia{sv})").expect("a valid type");
                invocation.return_value(Some(&glib::Variant::tuple_from_iter([
                    glib::Variant::array_from_iter_with_type(ty, entries),
                ])));
            }
            "GetProperty" => {
                let Some((id, name)) = params.get::<(i32, String)>() else {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, method);
                };
                let value = self
                    .state
                    .borrow()
                    .properties(id, std::slice::from_ref(&name))
                    .into_iter()
                    .next();
                match value {
                    Some((_, v)) => {
                        invocation.return_value(Some(&glib::Variant::tuple_from_iter([
                            glib::Variant::from_variant(&prop_variant(&v)),
                        ])))
                    }
                    None => invocation.return_dbus_error(ERROR_INVALID_ARGS, "no such property"),
                }
            }
            "Event" => {
                let Some((id, event, _data, _time)) =
                    params.get::<(i32, String, glib::Variant, u32)>()
                else {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, method);
                };
                self.menu_event(id, &event);
                invocation.return_value(None);
            }
            "EventGroup" => {
                let Some((events,)) = params.get::<(Vec<(i32, String, glib::Variant, u32)>,)>()
                else {
                    return invocation.return_dbus_error(ERROR_INVALID_ARGS, method);
                };
                let mut unknown = Vec::new();
                for (id, event, _, _) in events {
                    if self.state.borrow().has(id) {
                        self.menu_event(id, &event);
                    } else {
                        unknown.push(id);
                    }
                }
                invocation.return_value(Some(&(unknown,).to_variant()));
            }
            "AboutToShow" => {
                // Fresh texts ("In touch just now") right before it opens.
                let changed = self.refresh();
                invocation.return_value(Some(&(changed,).to_variant()));
            }
            "AboutToShowGroup" => {
                self.refresh();
                let none: Vec<i32> = Vec::new();
                invocation.return_value(Some(&(none.clone(), none).to_variant()));
            }
            _ => invocation.return_dbus_error(ERROR_UNKNOWN_METHOD, method),
        }
    }

    fn menu_event(&self, id: i32, event: &str) {
        if event != "clicked" {
            return;
        }
        let action = self.state.borrow().action(id).cloned();
        if let Some(a) = action {
            self.act(a);
        }
    }

    fn menu_property(&self, name: &str) -> glib::Variant {
        match name {
            "Version" => 3u32.to_variant(),
            "TextDirection" => "ltr".to_variant(),
            "Status" => "normal".to_variant(),
            "IconThemePath" => {
                let path = icon_theme_path();
                let paths: Vec<String> = if path.is_empty() {
                    Vec::new()
                } else {
                    vec![path.to_string()]
                };
                paths.to_variant()
            }
            _ => "".to_variant(),
        }
    }
}

/// `(ia{sv}av)`: one item, its properties and its children.
fn layout_node(
    s: &MenuState,
    id: i32,
    names: &[String],
    children: &[glib::Variant],
) -> glib::Variant {
    let children = glib::Variant::array_from_iter_with_type(
        glib::VariantTy::VARIANT,
        children.iter().map(glib::Variant::from_variant),
    );
    glib::Variant::tuple_from_iter([
        id.to_variant(),
        props_dict(s.properties(id, names)),
        children,
    ])
}

/// `a{sv}`
fn props_dict(props: Vec<(&'static str, Prop)>) -> glib::Variant {
    let dict = glib::VariantDict::new(None);
    for (key, value) in props {
        dict.insert_value(key, &prop_variant(&value));
    }
    dict.end()
}

fn prop_variant(p: &Prop) -> glib::Variant {
    match p {
        Prop::Str(s) => s.to_variant(),
        Prop::Bool(b) => b.to_variant(),
    }
}

/// Running from the source tree, the icon is not installed: point the
/// host at `data/icons` (debug builds only).
fn icon_theme_path() -> &'static str {
    if cfg!(debug_assertions) {
        concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons")
    } else {
        ""
    }
}

fn no_pixmaps() -> glib::Variant {
    let ty = glib::VariantTy::new("(iiay)").expect("a valid type");
    glib::Variant::array_from_iter_with_type(ty, Vec::<glib::Variant>::new())
}

/// `a(iiay)`: the app icon at 32 and 64 px as ARGB32 (network byte order).
fn icon_pixmap() -> glib::Variant {
    let ty = glib::VariantTy::new("(iiay)").expect("a valid type");
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(ICON_PNG));
    let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE) else {
        return no_pixmaps();
    };
    let entries: Vec<glib::Variant> = [32, 64]
        .into_iter()
        .filter_map(|px| {
            let p = pixbuf.scale_simple(px, px, gdk_pixbuf::InterpType::Bilinear)?;
            let bytes = p.read_pixel_bytes();
            let argb = tray::argb32_pixmap(
                p.width() as usize,
                p.height() as usize,
                p.rowstride() as usize,
                p.n_channels() as usize,
                &bytes,
            )?;
            Some(glib::Variant::tuple_from_iter([
                p.width().to_variant(),
                p.height().to_variant(),
                glib::Variant::array_from_fixed_array(&argb),
            ]))
        })
        .collect();
    glib::Variant::array_from_iter_with_type(ty, entries)
}
