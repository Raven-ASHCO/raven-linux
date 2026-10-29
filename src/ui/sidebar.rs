//! The Calm sidebar (272 points, the `sidebar` colour): the mark and
//! **Raven** with the heartbeat ("your node: ok · now"), **You** with your
//! fingerprint, Search, FRIENDS (avatar with its presence dot, name, status
//! line, the path icon of the last delivery or an unread badge), GROUPS
//! (stacked avatars, the newest line, an unread badge), Add a friend and
//! New group, and at the bottom Network ("n ways on"), Security words,
//! Security and Settings.
//!
//! Widgets only: the main window fills them from the engine's answers and
//! reacts to their signals.

use gtk::prelude::*;
use raven_desktop::api::{Contact, Group, GroupMember};
use raven_desktop::groupview;
use raven_desktop::icons::Icon;
use raven_desktop::present::{self, PathStyle};
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use std::rc::Rc;

use super::calm::{self, IconView, MarkView};
use super::style::Style;
use super::widgets::{hbox, label, line, vbox, Avatar, AvatarSize};

pub struct FriendRow {
    pub address: String,
    pub row: gtk::ListBoxRow,
    pub avatar: Rc<Avatar>,
    pub name: gtk::Label,
    unverified: gtk::Label,
    status: gtk::Label,
    badge: gtk::Label,
    via: IconView,
}

impl FriendRow {
    pub fn new(c: &Contact) -> Self {
        let avatar = Avatar::new(&c.name, &c.address);
        let name = line(&c.name, &["row-name"]);
        let unverified = line(tr("home.notVerified"), &["unverified-tag"]);
        let name_row = hbox(5);
        name_row.append(&name);
        name_row.append(&unverified);
        let status = line("", &["row-status"]);
        let texts = vbox(1);
        texts.set_hexpand(true);
        texts.set_valign(gtk::Align::Center);
        texts.append(&name_row);
        texts.append(&status);
        let badge = gtk::Label::new(None);
        badge.add_css_class("badge");
        badge.set_valign(gtk::Align::Center);
        badge.set_visible(false);
        let via = IconView::new(Icon::Route, 16);
        via.area.add_css_class("via");
        via.area.set_visible(false);
        let h = hbox(10);
        h.append(&avatar.root);
        h.append(&texts);
        h.append(&badge);
        h.append(&via.area);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&h));
        Self {
            address: c.address.clone(),
            row,
            avatar,
            name,
            unverified,
            status,
            badge,
            via,
        }
    }

    /// Name, status line (warning colour while messages wait), and the
    /// accessible text of the whole row.
    pub fn update(&self, c: &Contact, now_ms: u64, style: &Style) {
        self.name.set_text(&c.name);
        self.avatar.set_name(&c.name);
        self.unverified.set_visible(!c.verified);
        let status = present::contact_status_line(c, now_ms);
        self.status.set_text(&status);
        if c.waiting.is_some_and(|w| w.count > 0) {
            self.status.add_css_class("wait");
        } else {
            self.status.remove_css_class("wait");
        }
        self.avatar.set_presence(c.presence, false, style);
        super::a11y::set_label(&self.row, &format!("{}, {status}", c.name));
    }

    /// The unread badge, else the path icon of the last delivery (only while
    /// the friend is in touch, and only when the engine said the path).
    pub fn set_right(&self, unread: u32, path: Option<PathStyle>, in_touch: bool) {
        if unread > 0 {
            self.badge.set_text(&unread.to_string());
            self.badge
                .set_tooltip_text(Some(&trf("home.unread", &[("n", &unread.to_string())])));
            self.badge.set_visible(true);
            self.via.area.set_visible(false);
            return;
        }
        self.badge.set_visible(false);
        match path.filter(|p| in_touch && p.class.is_some()) {
            Some(p) => {
                self.via.set(p.icon);
                self.via.area.set_visible(true);
            }
            None => self.via.area.set_visible(false),
        }
    }
}

pub struct GroupRow {
    pub id: String,
    pub row: gtk::ListBoxRow,
    stack: gtk::Fixed,
    name: gtk::Label,
    status: gtk::Label,
    badge: gtk::Label,
}

impl GroupRow {
    pub fn new(g: &Group) -> Self {
        let stack = gtk::Fixed::new();
        stack.set_size_request(36, 36);
        stack.set_valign(gtk::Align::Center);
        stack.set_can_target(false);
        let name = line(&g.name, &["row-name"]);
        let status = line("", &["row-status"]);
        let texts = vbox(1);
        texts.set_hexpand(true);
        texts.set_valign(gtk::Align::Center);
        texts.append(&name);
        texts.append(&status);
        let badge = gtk::Label::new(None);
        badge.add_css_class("badge");
        badge.set_valign(gtk::Align::Center);
        badge.set_visible(false);
        let h = hbox(10);
        h.append(&stack);
        h.append(&texts);
        h.append(&badge);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&h));
        Self {
            id: g.id.clone(),
            row,
            stack,
            name,
            status,
            badge,
        }
    }

    pub fn update(&self, g: &Group) {
        self.name.set_text(&g.name);
        let line = groupview::sidebar_line(g);
        self.status.set_text(&line);
        if g.unread > 0 {
            self.badge.set_text(&g.unread.to_string());
            self.badge.set_visible(true);
        } else {
            self.badge.set_visible(false);
        }
        super::a11y::set_label(&self.row, &format!("{}, {line}", g.name));
    }

    /// Up to three members' avatars, stacked (once their names are known).
    pub fn set_members(&self, members: &[GroupMember]) {
        while let Some(c) = self.stack.first_child() {
            self.stack.remove(&c);
        }
        let spots = [(0.0, 1.0), (15.0, 1.0), (7.5, 15.0)];
        for (m, (x, y)) in members.iter().filter(|m| !m.me).zip(spots) {
            let a = Avatar::sized(&m.name, &m.address, AvatarSize::Stacked);
            a.stacked();
            self.stack.put(&a.root, x, y);
        }
    }
}

/// One entry of the bottom list.
pub struct NavRow {
    pub row: gtk::ListBoxRow,
    pub meta: gtk::Label,
}

fn nav_row(text: &str, icon: Icon) -> NavRow {
    let h = hbox(10);
    h.append(&calm::icon(icon, 16));
    let l = line(text, &[]);
    l.set_hexpand(true);
    h.append(&l);
    let meta = line("", &["row-meta"]);
    meta.set_visible(false);
    h.append(&meta);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&h));
    NavRow { row, meta }
}

pub struct Sidebar {
    pub root: gtk::Box,
    pub heart: gtk::Box,
    pub node_text: gtk::Label,
    pub node_button: gtk::Button,
    pub you_button: gtk::Button,
    pub you_title: gtk::Label,
    pub you_fingerprint: gtk::Label,
    pub search: gtk::SearchEntry,
    pub friends: gtk::ListBox,
    pub groups_title: gtk::Label,
    pub groups: gtk::ListBox,
    pub new_group: gtk::Button,
    pub nav: gtk::ListBox,
    pub nav_network: NavRow,
}

/// Index of each entry of the bottom list.
pub const NAV_NETWORK: i32 = 0;
pub const NAV_WORDS: i32 = 1;
pub const NAV_SECURITY: i32 = 2;
pub const NAV_SETTINGS: i32 = 3;

impl Sidebar {
    pub fn new() -> Self {
        let t = tokens();
        let root = vbox(6);
        root.add_css_class("raven-sidebar");
        root.set_size_request(t.layout.sidebar_width as i32, -1);

        // The mark and Raven, with the heartbeat (opens the Network page).
        let mark = MarkView::new(26);
        let brand_name = line(tr("app.name"), &["t-brand"]);
        let heart = hbox(0);
        heart.add_css_class("heart");
        heart.set_valign(gtk::Align::Center);
        let node_text = line("", &["t-small", "dim"]);
        let hb = hbox(6);
        hb.set_hexpand(true);
        hb.set_halign(gtk::Align::End);
        hb.append(&heart);
        hb.append(&node_text);
        let brand = hbox(10);
        brand.add_css_class("brand");
        brand.append(&mark.area);
        brand.append(&brand_name);
        brand.append(&hb);
        let node_button = gtk::Button::new();
        node_button.add_css_class("raven-flat");
        node_button.set_child(Some(&brand));
        node_button.set_tooltip_text(Some(tr("net.title")));
        root.append(&node_button);

        // You.
        let you_avatar = Avatar::you(tr("you.title"));
        let you_title = line(tr("you.title"), &["row-name"]);
        let you_fingerprint = line("", &["row-status"]);
        let you_texts = vbox(1);
        you_texts.set_valign(gtk::Align::Center);
        you_texts.append(&you_title);
        you_texts.append(&you_fingerprint);
        let you_row = hbox(10);
        you_row.append(&you_avatar.root);
        you_row.append(&you_texts);
        let you_button = gtk::Button::new();
        you_button.add_css_class("raven-flat");
        you_button.set_child(Some(&you_row));
        root.append(&you_button);

        let search = gtk::SearchEntry::new();
        search.add_css_class("raven-search");
        search.set_placeholder_text(Some(tr("home.search")));
        root.append(&search);

        // Friends and groups scroll together.
        let lists = vbox(2);
        let friends_title = label(&tr("home.friends").to_uppercase(), &["section-label"]);
        lists.append(&friends_title);
        let friends = gtk::ListBox::new();
        friends.add_css_class("raven-list");
        friends.set_selection_mode(gtk::SelectionMode::Single);
        super::a11y::set_label(&friends, tr("home.friends"));
        lists.append(&friends);
        let groups_title = label(&tr("home.groups").to_uppercase(), &["section-label"]);
        groups_title.set_visible(false);
        lists.append(&groups_title);
        let groups = gtk::ListBox::new();
        groups.add_css_class("raven-list");
        groups.set_selection_mode(gtk::SelectionMode::Single);
        groups.set_visible(false);
        super::a11y::set_label(&groups, tr("home.groups"));
        lists.append(&groups);
        let add_friend = add_button(tr("home.addFriend"));
        add_friend.set_action_name(Some("app.add-friend"));
        lists.append(&add_friend);
        let new_group = add_button(tr("home.newGroup"));
        new_group.set_visible(false);
        lists.append(&new_group);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&lists));
        root.append(&scroll);

        // Network, Security words, Security, Settings.
        let nav = gtk::ListBox::new();
        nav.add_css_class("raven-nav");
        nav.set_selection_mode(gtk::SelectionMode::Single);
        let nav_network = nav_row(tr("network.title"), Icon::Net);
        nav.append(&nav_network.row);
        nav.append(&nav_row(tr("nav.securityWords"), Icon::Words).row);
        nav.append(&nav_row(tr("security.title"), Icon::Lock).row);
        nav.append(&nav_row(tr("settings.title"), Icon::Gear).row);
        let nav_box = vbox(0);
        nav_box.add_css_class("nav-top");
        nav_box.append(&nav);
        root.append(&nav_box);

        Self {
            root,
            heart,
            node_text,
            node_button,
            you_button,
            you_title,
            you_fingerprint,
            search,
            friends,
            groups_title,
            groups,
            new_group,
            nav,
            nav_network,
        }
    }

    /// "6 ways on" beside Network.
    pub fn set_ways_on(&self, n: Option<usize>) {
        match n {
            Some(n) => {
                self.nav_network
                    .meta
                    .set_text(&trf("nav.waysOn", &[("n", &n.to_string())]));
                self.nav_network.meta.set_visible(true);
            }
            None => self.nav_network.meta.set_visible(false),
        }
    }

    /// Groups exist in this build (`groups.list` said `available`).
    pub fn set_groups_available(&self, on: bool) {
        self.groups_title.set_visible(on);
        self.groups.set_visible(on);
        self.new_group.set_visible(on);
    }
}

fn add_button(text: &str) -> gtk::Button {
    let row = hbox(8);
    row.append(&calm::icon(Icon::Plus, 16));
    row.append(&gtk::Label::new(Some(text)));
    let b = gtk::Button::new();
    b.add_css_class("add-friend");
    b.set_child(Some(&row));
    b.set_halign(gtk::Align::Start);
    super::a11y::set_label(&b, text);
    b
}
