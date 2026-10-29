//! The tray menu, GTK-free and unit-tested: what the StatusNotifierItem
//! (`org.kde.StatusNotifierItem`) and its menu (`com.canonical.dbusmenu`)
//! show. The D-Bus glue in `src/ui/tray.rs` only turns this into GLib
//! variants and back.
//!
//! The menu is the one of the macOS menu bar and the Windows notification
//! area (DESIGN.md, "Menu bar / tray"): the status (listening or not, the
//! background program's heartbeat), the friends, Open Raven, Add a friend…,
//! Lock now (disabled until Lock can be used), Quit window (keeps receiving)
//! and Quit and stop the background program. On Linux the network state
//! shows there too (NETWORK_PARITY.md): the ways that are on, "Lab switches
//! on" in a developer build with switches set, and Network… to open the
//! Network page. Every text comes from strings.en.json.

use crate::api::{Contact, NodeStatus};
use crate::netview::NetworkSummary;
use crate::present::{self, NodeChip};
use crate::strings::{tr, trf};
use crate::theme::tokens;

/// The well-known names, paths and interfaces of the protocol.
pub mod dbus {
    pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
    pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
    pub const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
    pub const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
    pub const ITEM_PATH: &str = "/StatusNotifierItem";
    pub const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
    pub const MENU_PATH: &str = "/MenuBar";

    /// The item's bus name (StatusNotifierItem spec: `-{pid}-{id}`).
    pub fn item_name(pid: u32) -> String {
        format!("org.kde.StatusNotifierItem-{pid}-1")
    }

    /// Introspection data of both interfaces (only what Raven implements).
    pub const INTROSPECTION: &str = r#"<node>
  <interface name="org.kde.StatusNotifierItem">
    <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus"><arg name="status" type="s"/></signal>
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
  </interface>
  <interface name="com.canonical.dbusmenu">
    <method name="GetLayout">
      <arg name="parentId" type="i" direction="in"/>
      <arg name="recursionDepth" type="i" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="revision" type="u" direction="out"/>
      <arg name="layout" type="(ia{sv}av)" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg name="ids" type="ai" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="properties" type="a(ia{sv})" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg name="id" type="i" direction="in"/>
      <arg name="name" type="s" direction="in"/>
      <arg name="value" type="v" direction="out"/>
    </method>
    <method name="Event">
      <arg name="id" type="i" direction="in"/>
      <arg name="eventId" type="s" direction="in"/>
      <arg name="data" type="v" direction="in"/>
      <arg name="timestamp" type="u" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg name="events" type="a(isvu)" direction="in"/>
      <arg name="idErrors" type="ai" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg name="id" type="i" direction="in"/>
      <arg name="needUpdate" type="b" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg name="ids" type="ai" direction="in"/>
      <arg name="updatesNeeded" type="ai" direction="out"/>
      <arg name="idErrors" type="ai" direction="out"/>
    </method>
    <signal name="ItemsPropertiesUpdated">
      <arg name="updatedProps" type="a(ia{sv})"/>
      <arg name="removedProps" type="a(ias)"/>
    </signal>
    <signal name="LayoutUpdated">
      <arg name="revision" type="u"/>
      <arg name="parent" type="i"/>
    </signal>
    <signal name="ItemActivationRequested">
      <arg name="id" type="i"/>
      <arg name="timestamp" type="u"/>
    </signal>
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
  </interface>
</node>"#;
}

/// What a menu entry does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    /// Open Raven (also a click on the icon itself).
    Open,
    /// Open Raven on this friend's chat.
    OpenFriend(String),
    AddFriend,
    /// Open the Network page.
    Network,
    /// "Lock now": only enabled once Lock can be used (Lock v2 served by
    /// the background program, or the privacy pack's lock).
    Lock,
    /// Quit the window app; the background program keeps receiving.
    QuitWindow,
    /// Quit and stop the background program.
    QuitAll,
}

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    Separator,
    Entry {
        label: String,
        enabled: bool,
        /// `None`: a status line (shown, not clickable).
        action: Option<TrayAction>,
    },
}

impl MenuItem {
    fn status(label: String) -> Self {
        MenuItem::Entry {
            label,
            enabled: false,
            action: None,
        }
    }

    fn action(label: String, action: TrayAction, enabled: bool) -> Self {
        MenuItem::Entry {
            label,
            enabled,
            action: Some(action),
        }
    }
}

/// Everything the menu shows, taken from the app's current state.
#[derive(Debug, Clone, Default)]
pub struct TrayInput {
    pub node: NodeStatus,
    /// Engine time of the last `status_ok` (0: none yet).
    pub last_ok_ms: u64,
    pub now_ms: u64,
    /// The main window exists (set up and not locked): friends and
    /// "Add a friend…" are offered.
    pub ready: bool,
    /// Friends in the engine's order, each with its unread count.
    pub friends: Vec<(Contact, u32)>,
    /// "Lock now" works (Lock v2 can lock, or `privacy.state` is available).
    pub can_lock: bool,
    /// The network line(s), once `network.status` answered.
    pub network: Option<NetworkSummary>,
}

/// True when Raven listens on the network (not only on this computer).
pub fn listening(node: &NodeStatus) -> bool {
    node.running && node.listening.is_some() && !node.is_local_only()
}

/// The first line of the menu, also the tooltip.
pub fn status_line(node: &NodeStatus) -> &'static str {
    if listening(node) {
        tr("tray.listening")
    } else {
        tr("tray.notListening")
    }
}

/// The second line: the background program's heartbeat.
pub fn heartbeat_line(input: &TrayInput) -> String {
    let interval = u64::from(tokens().durations.status_check_interval);
    match present::node_chip(&input.node, input.last_ok_ms, input.now_ms, interval) {
        NodeChip::Ok(_) => format!("\u{2665} {}", tr("tray.background")),
        NodeChip::Down(text) | NodeChip::Starting(text) => text,
    }
}

/// One friend line, as in the macOS menu: "Ali · In touch just now", or
/// "1 new from Rena" while there are unread messages.
pub fn friend_line(c: &Contact, unread: u32, now_ms: u64) -> String {
    if unread > 0 {
        return trf(
            "home.newFrom",
            &[("n", &unread.to_string()), ("name", &c.name)],
        );
    }
    format!(
        "{} \u{00B7} {}",
        c.name,
        present::contact_status_line(c, now_ms)
    )
}

/// The whole menu, top to bottom.
pub fn menu(input: &TrayInput) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::status(status_line(&input.node).to_string()),
        MenuItem::status(heartbeat_line(input)),
    ];
    if let Some(n) = input.network.as_ref().filter(|_| input.ready) {
        items.push(MenuItem::status(n.ways.clone()));
        if n.lab {
            items.push(MenuItem::status(tr("network.tray.lab").to_string()));
        }
    }
    items.push(MenuItem::Separator);
    if input.ready && !input.friends.is_empty() {
        for (c, unread) in &input.friends {
            items.push(MenuItem::action(
                friend_line(c, *unread, input.now_ms),
                TrayAction::OpenFriend(c.address.clone()),
                true,
            ));
        }
        items.push(MenuItem::Separator);
    }
    items.push(MenuItem::action(
        tr("tray.open").to_string(),
        TrayAction::Open,
        true,
    ));
    items.push(MenuItem::action(
        tr("tray.addFriend").to_string(),
        TrayAction::AddFriend,
        input.ready,
    ));
    items.push(MenuItem::action(
        tr("network.tray.open").to_string(),
        TrayAction::Network,
        input.ready,
    ));
    items.push(MenuItem::action(
        tr("tray.lock").to_string(),
        TrayAction::Lock,
        input.ready && input.can_lock,
    ));
    items.push(MenuItem::Separator);
    items.push(MenuItem::action(
        tr("tray.quitWindow").to_string(),
        TrayAction::QuitWindow,
        true,
    ));
    items.push(MenuItem::action(
        tr("tray.quitAll").to_string(),
        TrayAction::QuitAll,
        true,
    ));
    items
}

/// A dbusmenu property value (only the two kinds Raven uses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prop {
    Str(String),
    Bool(bool),
}

/// The root item's id (dbusmenu: 0 is the root).
pub const ROOT_ID: i32 = 0;

/// dbusmenu treats `_` as the mnemonic marker; a literal one is doubled.
pub fn escape_label(label: &str) -> String {
    label.replace('_', "__")
}

/// The published menu: items with stable ids for one layout revision.
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    revision: u32,
    items: Vec<MenuItem>,
    tooltip: String,
}

impl MenuState {
    /// Take a new menu. True when it differs from the published one (the
    /// revision goes up and the glue emits `LayoutUpdated`).
    pub fn update(&mut self, items: Vec<MenuItem>) -> bool {
        if items == self.items {
            return false;
        }
        self.items = items;
        self.revision = self.revision.wrapping_add(1).max(1);
        true
    }

    /// Take a new tooltip text. True when it changed (`NewToolTip`).
    pub fn set_tooltip(&mut self, text: &str) -> bool {
        if self.tooltip == text {
            return false;
        }
        self.tooltip = text.to_string();
        true
    }

    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Item ids, top to bottom: 1, 2, …
    pub fn ids(&self) -> Vec<i32> {
        (1..=self.items.len() as i32).collect()
    }

    pub fn item(&self, id: i32) -> Option<&MenuItem> {
        usize::try_from(id)
            .ok()
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| self.items.get(i))
    }

    /// True for the root and every item id.
    pub fn has(&self, id: i32) -> bool {
        id == ROOT_ID || self.item(id).is_some()
    }

    /// What a click on `id` does: nothing for status lines, separators,
    /// disabled entries and unknown ids.
    pub fn action(&self, id: i32) -> Option<&TrayAction> {
        match self.item(id)? {
            MenuItem::Entry {
                action: Some(a),
                enabled: true,
                ..
            } => Some(a),
            _ => None,
        }
    }

    /// The properties of one item, defaults left out (dbusmenu: absent means
    /// default), filtered by `names` (empty: all).
    pub fn properties(&self, id: i32, names: &[String]) -> Vec<(&'static str, Prop)> {
        let all: Vec<(&'static str, Prop)> = if id == ROOT_ID {
            vec![("children-display", Prop::Str("submenu".into()))]
        } else {
            match self.item(id) {
                None => Vec::new(),
                Some(MenuItem::Separator) => vec![("type", Prop::Str("separator".into()))],
                Some(MenuItem::Entry { label, enabled, .. }) => {
                    let mut p = vec![("label", Prop::Str(escape_label(label)))];
                    if !enabled {
                        p.push(("enabled", Prop::Bool(false)));
                    }
                    p
                }
            }
        };
        all.into_iter()
            .filter(|(k, _)| names.is_empty() || names.iter().any(|n| n == k))
            .collect()
    }
}

/// An RGBA image (GdkPixbuf layout) as a StatusNotifierItem `IconPixmap`
/// entry: ARGB32 in network byte order (A, R, G, B per pixel), rows packed.
/// `None` when the buffer is too short for the given size.
pub fn argb32_pixmap(
    width: usize,
    height: usize,
    rowstride: usize,
    channels: usize,
    pixels: &[u8],
) -> Option<Vec<u8>> {
    if !(channels == 3 || channels == 4) || width == 0 || height == 0 {
        return None;
    }
    let needed = (height - 1) * rowstride + width * channels;
    if pixels.len() < needed || rowstride < width * channels {
        return None;
    }
    let mut out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &pixels[y * rowstride..y * rowstride + width * channels];
        for px in row.chunks_exact(channels) {
            let a = if channels == 4 { px[3] } else { 0xFF };
            out.extend_from_slice(&[a, px[0], px[1], px[2]]);
        }
    }
    Some(out)
}

/// When the main window is closed: with a tray icon on screen Raven hides
/// to it (as on Windows); without one it follows Settings › When I quit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    HideToTray,
    FollowWhenQuit,
}

pub fn close_action(tray_shown: bool) -> CloseAction {
    if tray_shown {
        CloseAction::HideToTray
    } else {
        CloseAction::FollowWhenQuit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Presence, Waiting};

    const NOW: u64 = 1_792_505_100_000;

    fn node(listening: Option<&str>) -> NodeStatus {
        NodeStatus {
            running: true,
            receive: true,
            listening: listening.map(str::to_string),
            ..Default::default()
        }
    }

    fn friend(name: &str, presence: Presence, verified: bool) -> Contact {
        Contact {
            address: format!("rvn1q{}", name.to_lowercase()),
            name: name.into(),
            short_address: String::new(),
            verified,
            presence,
            last_contact_ms: Some(NOW - 5 * 60_000),
            waiting: None,
            avatar_index: 0,
            lan: None,
            unread: 0,
        }
    }

    fn input() -> TrayInput {
        crate::timefmt::set_local_offset_secs(0);
        let mut mina = friend("Mina", Presence::Waiting, true);
        mina.waiting = Some(Waiting {
            count: 1,
            until_ms: NOW + 20 * 3_600_000,
        });
        TrayInput {
            node: node(Some("0.0.0.0:7420")),
            last_ok_ms: NOW - 1_000,
            now_ms: NOW,
            ready: true,
            friends: vec![
                (friend("Ali", Presence::InTouch, true), 0),
                (mina, 0),
                (friend("Rena", Presence::NoRecent, false), 2),
            ],
            can_lock: false,
            network: None,
        }
    }

    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                MenuItem::Separator => "--".to_string(),
                MenuItem::Entry { label, .. } => label.clone(),
            })
            .collect()
    }

    #[test]
    fn the_menu_matches_macos_and_windows() {
        let items = menu(&input());
        assert_eq!(
            labels(&items),
            vec![
                "Raven is listening on Wi-Fi",
                "\u{2665} background program ok",
                "--",
                "Ali \u{00B7} In touch 5 min ago",
                "Mina \u{00B7} 1 waiting \u{00B7} until Wed 10:05 (UTC)",
                "2 new from Rena",
                "--",
                "Open Raven",
                "Add a friend\u{2026}",
                "Network\u{2026}",
                "Lock now",
                "--",
                "Quit window (keeps receiving)",
                "Quit and stop the background program",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
        // Status lines are not clickable; Lock waits for the privacy pack.
        let mut s = MenuState::default();
        s.update(items);
        assert_eq!(s.action(1), None);
        assert_eq!(s.action(2), None);
        assert_eq!(s.action(3), None, "a separator");
        assert_eq!(
            s.action(4),
            Some(&TrayAction::OpenFriend("rvn1qali".into()))
        );
        assert_eq!(s.action(8), Some(&TrayAction::Open));
        assert_eq!(s.action(9), Some(&TrayAction::AddFriend));
        assert_eq!(s.action(10), Some(&TrayAction::Network));
        assert_eq!(s.action(11), None, "Lock now is disabled");
        assert_eq!(s.action(13), Some(&TrayAction::QuitWindow));
        assert_eq!(s.action(14), Some(&TrayAction::QuitAll));
        assert_eq!(s.action(15), None);
        assert_eq!(s.action(0), None, "the root");
        assert_eq!(s.action(-1), None);
    }

    #[test]
    fn lock_is_offered_once_it_can_lock() {
        let mut i = input();
        i.can_lock = true;
        let mut s = MenuState::default();
        s.update(menu(&i));
        assert_eq!(s.action(11), Some(&TrayAction::Lock));
    }

    #[test]
    fn the_network_state_shows_in_the_menu() {
        let mut i = input();
        i.network = Some(NetworkSummary {
            ways: "Ways on: LAN \u{00B7} p2p (lab)".into(),
            on: 2,
            lab: true,
        });
        let items = menu(&i);
        assert_eq!(
            labels(&items)[..5],
            [
                "Raven is listening on Wi-Fi".to_string(),
                "\u{2665} background program ok".to_string(),
                "Ways on: LAN \u{00B7} p2p (lab)".to_string(),
                "Lab switches on".to_string(),
                "--".to_string()
            ]
        );
        let mut s = MenuState::default();
        s.update(items);
        assert_eq!(s.action(3), None, "status lines are not clickable");
        i.network.as_mut().unwrap().lab = false;
        assert_eq!(labels(&menu(&i))[3], "--", "no lab line without switches");
        i.ready = false;
        assert_eq!(labels(&menu(&i))[2], "--", "not before setup");
    }

    #[test]
    fn before_setup_only_open_and_quit_work() {
        let i = TrayInput {
            ready: false,
            friends: Vec::new(),
            now_ms: NOW,
            ..Default::default()
        };
        let items = menu(&i);
        assert_eq!(
            labels(&items)[..3],
            [
                "Raven isn't receiving".to_string(),
                "Raven's background program isn't responding".to_string(),
                "--".to_string()
            ]
        );
        let mut s = MenuState::default();
        s.update(items);
        let enabled: Vec<_> = s.ids().into_iter().filter_map(|id| s.action(id)).collect();
        assert_eq!(
            enabled,
            vec![
                &TrayAction::Open,
                &TrayAction::QuitWindow,
                &TrayAction::QuitAll
            ],
            "Network… waits for setup too"
        );
    }

    #[test]
    fn status_lines_follow_the_node() {
        assert_eq!(
            status_line(&node(Some("0.0.0.0:7420"))),
            "Raven is listening on Wi-Fi"
        );
        assert_eq!(
            status_line(&node(Some("127.0.0.1:7420"))),
            "Raven isn't receiving"
        );
        assert_eq!(status_line(&node(None)), "Raven isn't receiving");
        let mut i = input();
        i.last_ok_ms = NOW - 60_000; // overdue
        assert_eq!(
            heartbeat_line(&i),
            "Raven's background program isn't responding"
        );
        i.node = node(None); // running, receiving, not listening yet
        assert_eq!(heartbeat_line(&i), "Starting Raven\u{2026}");
    }

    #[test]
    fn layout_revision_changes_only_with_the_menu() {
        let mut s = MenuState::default();
        assert!(s.update(menu(&input())));
        let r = s.revision();
        assert!(r > 0);
        assert!(!s.update(menu(&input())), "same texts: no LayoutUpdated");
        assert_eq!(s.revision(), r);
        let mut later = input();
        later.now_ms += 10 * 60_000; // "In touch 15 min ago"
        assert!(s.update(menu(&later)));
        assert_eq!(s.revision(), r + 1);
        assert!(s.set_tooltip("a"));
        assert!(!s.set_tooltip("a"));
        assert_eq!(s.tooltip(), "a");
    }

    #[test]
    fn dbusmenu_properties() {
        let mut s = MenuState::default();
        s.update(vec![
            MenuItem::status("Raven isn't receiving".into()),
            MenuItem::Separator,
            MenuItem::action("snake_case friend".into(), TrayAction::Open, true),
        ]);
        assert_eq!(
            s.properties(ROOT_ID, &[]),
            vec![("children-display", Prop::Str("submenu".into()))]
        );
        assert_eq!(
            s.properties(1, &[]),
            vec![
                ("label", Prop::Str("Raven isn't receiving".into())),
                ("enabled", Prop::Bool(false))
            ]
        );
        assert_eq!(
            s.properties(2, &[]),
            vec![("type", Prop::Str("separator".into()))]
        );
        assert_eq!(
            s.properties(3, &[]),
            vec![("label", Prop::Str("snake__case friend".into()))],
            "a literal underscore is not a mnemonic; enabled is the default"
        );
        assert_eq!(
            s.properties(1, &["enabled".to_string()]),
            vec![("enabled", Prop::Bool(false))]
        );
        assert!(s.properties(9, &[]).is_empty());
        assert!(s.has(0) && s.has(3) && !s.has(4));
        assert_eq!(s.ids(), vec![1, 2, 3]);
    }

    #[test]
    fn pixmaps_are_argb_in_network_order() {
        // 2x1 RGBA with a padded row (rowstride 12).
        let px = [10, 20, 30, 40, 50, 60, 70, 80, 0, 0, 0, 0];
        assert_eq!(
            argb32_pixmap(2, 1, 12, 4, &px),
            Some(vec![40, 10, 20, 30, 80, 50, 60, 70])
        );
        // RGB without alpha is opaque.
        assert_eq!(
            argb32_pixmap(1, 2, 3, 3, &[1, 2, 3, 4, 5, 6]),
            Some(vec![255, 1, 2, 3, 255, 4, 5, 6])
        );
        assert_eq!(argb32_pixmap(2, 2, 8, 4, &[0; 8]), None, "too short");
        assert_eq!(argb32_pixmap(0, 1, 4, 4, &[0; 4]), None);
    }

    #[test]
    fn closing_hides_to_the_tray_only_when_it_is_shown() {
        assert_eq!(close_action(true), CloseAction::HideToTray);
        assert_eq!(close_action(false), CloseAction::FollowWhenQuit);
    }

    #[test]
    fn names_and_introspection() {
        assert_eq!(dbus::item_name(4242), "org.kde.StatusNotifierItem-4242-1");
        assert!(dbus::INTROSPECTION.contains(r#"<interface name="org.kde.StatusNotifierItem">"#));
        assert!(dbus::INTROSPECTION.contains(r#"<interface name="com.canonical.dbusmenu">"#));
    }
}
