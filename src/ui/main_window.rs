//! The main window (Calm): the sidebar (`sidebar.rs`) and, beside it, the
//! page the person chose: a chat with a friend or a group (`chat_view.rs`),
//! the Network page, Security, Settings or Security words. The header bar
//! has the sidebar toggle, the page's name and the app menu.
//!
//! Everything that moves here is caused by an engine event
//! (`handle_event`) or by the person's own action: the heartbeat on
//! `status_ok`, the route line on `send_started`, `receipt` and `incoming`,
//! the check that fills on a receipt, the bubble that rises in on
//! `incoming` (yours lifts out of the composer when `messages.send`
//! answered), the presence blink on `presence`, the ring on `retry`, a
//! group's ring on `group_changed`. The 1-second clock only updates texts.
//!
//! Engine calls go to the engine thread (`MainWindow::call`); their answers
//! come back on the main loop, in the order the calls were made.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::{
    ApiError, Contact, Direction, Event, EventKind, Group, GroupMember, GroupMessage, Message,
    MessageState, Presence, RavenEngine,
};
use raven_desktop::groupview;
use raven_desktop::icons::Icon;
use raven_desktop::motion::Packet;
use raven_desktop::present::{self, NodeChip, PathStyle};
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use raven_desktop::timefmt;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::calm;
use super::chat_view::{self, AwayCard, ChatView, MsgRow, Toast};
use super::network_page::NetworkPage;
use super::security_page::SecurityPage;
use super::sidebar::{GroupRow, Sidebar, NAV_NETWORK, NAV_SECURITY, NAV_SETTINGS, NAV_WORDS};
use super::style::Style;
use super::widgets::{self, label, vbox, Avatar, AvatarSize};
use super::words_page::WordsPage;
use super::{check_words, group_sheets, message_details, routes, settings};

const HISTORY_LIMIT: u32 = 50;
const MAX_ROWS: usize = 300;

/// What the right side of the window shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Chat,
    Network,
    Security,
    Settings,
    Words,
}

/// The chat on screen.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Open {
    Friend(String),
    Group(String),
}

pub struct MainWindow {
    ctx: Weak<Ctx>,
    /// This window, for engine answers that arrive later.
    me: Weak<MainWindow>,
    style: Rc<Style>,
    pub window: gtk::ApplicationWindow,
    header: gtk::HeaderBar,
    title: gtk::Label,
    sidebar_revealer: gtk::Revealer,
    sidebar_toggle: gtk::Button,
    side: Sidebar,
    rows: RefCell<Vec<super::sidebar::FriendRow>>,
    group_rows: RefCell<Vec<GroupRow>>,
    content: gtk::Stack,
    chat: Rc<ChatView>,
    away: Rc<AwayCard>,
    /// The pill on pages other than a chat.
    page_toast: Rc<Toast>,
    network_page: Rc<NetworkPage>,
    security_page: Rc<SecurityPage>,
    settings_page: Rc<settings::SettingsPage>,
    words_page: Rc<WordsPage>,
    page: Cell<Page>,
    contacts: RefCell<Vec<Contact>>,
    groups: RefCell<Vec<Group>>,
    groups_available: Cell<bool>,
    /// Members by group (`groups.get`).
    members: RefCell<HashMap<String, Vec<GroupMember>>>,
    open: RefCell<Option<Open>>,
    /// Rows of the messages on screen, by id.
    shown: RefCell<HashMap<String, Rc<MsgRow>>>,
    /// Every 1:1 message this window has seen, by id.
    known: RefCell<HashMap<String, Message>>,
    /// The open group's messages, by id.
    group_known: RefCell<HashMap<String, GroupMessage>>,
    /// Unread messages by friend, as the engine counts them.
    unread: RefCell<HashMap<String, u32>>,
    /// How the last message with a friend travelled: the engine's words
    /// and its path kind (shown only as the engine said them).
    last_path: RefCell<HashMap<String, (String, Option<String>)>>,
    /// The local day of the last row (for day lines).
    last_day: Cell<Option<i64>>,
    rows_count: Cell<usize>,
    last_ok_ms: Cell<u64>,
    stick_bottom: Rc<Cell<bool>>,
    selecting: Cell<bool>,
    /// A message is on its way to the engine (no double send).
    sending: Cell<bool>,
    /// The newest waiting message of the open friend (the away card acts
    /// on it).
    away_message: RefCell<Option<String>>,
}

impl MainWindow {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Self> {
        let style = ctx.style.clone();
        let t = tokens();
        let window = gtk::ApplicationWindow::new(&ctx.app);
        window.set_title(Some(tr("app.name")));
        window.set_size_request(t.min_window.0 as i32, t.min_window.1 as i32);
        window.set_default_size(1040, 700);
        style.decorate_window(&window);

        // Header bar: sidebar toggle, the page's name, the app menu.
        let header = gtk::HeaderBar::new();
        header.add_css_class("raven-header");
        header.add_css_class("split");
        let title = gtk::Label::new(Some(tr("app.name")));
        title.add_css_class("ttl");
        header.set_title_widget(Some(&title));
        let sidebar_toggle = gtk::Button::new();
        sidebar_toggle.add_css_class("raven-flat");
        sidebar_toggle.add_css_class("sidebar-toggle");
        sidebar_toggle.set_child(Some(&calm::icon(Icon::Sidebar, 16)));
        sidebar_toggle.set_tooltip_text(Some(tr("view.hideSidebar")));
        super::a11y::set_label(&sidebar_toggle, tr("view.hideSidebar"));
        header.pack_start(&sidebar_toggle);
        let menu = gio::Menu::new();
        let first = gio::Menu::new();
        first.append(Some(tr("tray.addFriend")), Some("app.add-friend"));
        first.append(Some(tr("net.title")), Some("app.network"));
        first.append(Some(tr("settings.title")), Some("app.preferences"));
        let second = gio::Menu::new();
        second.append(Some(tr("tray.quitWindow")), Some("app.quit-window"));
        second.append(Some(tr("tray.quitAll")), Some("app.quit-all"));
        menu.append_section(None, &first);
        menu.append_section(None, &second);
        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        menu_button.set_menu_model(Some(&menu));
        header.pack_end(&menu_button);
        window.set_titlebar(Some(&header));

        let side = Sidebar::new();
        let sidebar_revealer = gtk::Revealer::new();
        sidebar_revealer.set_transition_type(gtk::RevealerTransitionType::SlideRight);
        sidebar_revealer.set_transition_duration(style.duration(t.calm.view_in));
        sidebar_revealer.set_child(Some(&side.root));
        sidebar_revealer.set_reveal_child(true);

        // The pages.
        let chat = ChatView::new(&style);
        let away = AwayCard::new(&style);
        let none = vbox(12);
        none.set_valign(gtk::Align::Center);
        let none_label = label(tr("home.noFriends"), &["dim"]);
        none_label.set_xalign(0.5);
        none.append(&none_label);
        let none_action = widgets::primary_button(tr("home.noFriends.action"));
        none_action.set_halign(gtk::Align::Center);
        none_action.set_action_name(Some("app.add-friend"));
        none.append(&none_action);
        let network_page = NetworkPage::new(ctx);
        let security_page = SecurityPage::new(ctx);
        let settings_page = settings::SettingsPage::new(ctx, &network_page);
        let words_page = WordsPage::new(ctx);
        let content = gtk::Stack::new();
        content.add_named(&chat.root, Some("chat"));
        content.add_named(&none, Some("none"));
        content.add_named(&network_page.root, Some("network"));
        content.add_named(&security_page.root, Some("security"));
        content.add_named(&settings_page.root, Some("settings"));
        content.add_named(&words_page.root, Some("words"));
        content.set_hexpand(true);
        content.add_css_class("raven-main");
        let page_toast = Toast::new(&style);
        page_toast.revealer.set_margin_bottom(22);
        let content_overlay = gtk::Overlay::new();
        content_overlay.set_child(Some(&content));
        content_overlay.add_overlay(&page_toast.revealer);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&sidebar_revealer);
        body.append(&content_overlay);
        window.set_child(Some(&body));

        let mw = Rc::new_cyclic(|me| Self {
            ctx: Rc::downgrade(ctx),
            me: me.clone(),
            style: style.clone(),
            window,
            header,
            title,
            sidebar_revealer,
            sidebar_toggle,
            side,
            rows: RefCell::new(Vec::new()),
            group_rows: RefCell::new(Vec::new()),
            content,
            chat,
            away,
            page_toast,
            network_page,
            security_page,
            settings_page,
            words_page,
            page: Cell::new(Page::Chat),
            contacts: RefCell::new(Vec::new()),
            groups: RefCell::new(Vec::new()),
            groups_available: Cell::new(false),
            members: RefCell::new(HashMap::new()),
            open: RefCell::new(None),
            shown: RefCell::new(HashMap::new()),
            known: RefCell::new(HashMap::new()),
            group_known: RefCell::new(HashMap::new()),
            unread: RefCell::new(HashMap::new()),
            last_path: RefCell::new(HashMap::new()),
            last_day: Cell::new(None),
            rows_count: Cell::new(0),
            last_ok_ms: Cell::new(ctx.node.borrow().last_ok_ms),
            stick_bottom: Rc::new(Cell::new(true)),
            selecting: Cell::new(false),
            sending: Cell::new(false),
            away_message: RefCell::new(None),
        });
        mw.wire();
        mw.refresh_contacts_then(|m| {
            let first = m.contacts.borrow().first().map(|c| c.address.clone());
            if let Some(a) = first {
                m.select_row_for(&a);
            }
        });
        mw.refresh_groups();
        mw.load_me();
        mw.update_node_chip();
        mw
    }

    fn wire(self: &Rc<Self>) {
        let ctx = self.ctx.clone();
        self.window.connect_close_request(move |_| {
            if let Some(ctx) = ctx.upgrade() {
                ctx.close_main();
            }
            glib::Propagation::Stop
        });

        // Shown again (from the tray menu): the open chat is on screen.
        let me = self.me.clone();
        self.window.connect_visible_notify(move |_| {
            if let Some(m) = me.upgrade() {
                m.read_open_chat();
            }
        });

        let me = self.me.clone();
        self.sidebar_toggle.connect_clicked(move |b| {
            let Some(m) = me.upgrade() else { return };
            let show = !m.sidebar_revealer.reveals_child();
            m.sidebar_revealer.set_reveal_child(show);
            let text = if show {
                tr("view.hideSidebar")
            } else {
                tr("view.showSidebar")
            };
            b.set_tooltip_text(Some(text));
            super::a11y::set_label(b, text);
            if show {
                m.header.add_css_class("split");
            } else {
                m.header.remove_css_class("split");
            }
        });

        // Friends.
        let me = self.me.clone();
        self.side.friends.connect_row_selected(move |_, row| {
            let (Some(m), Some(row)) = (me.upgrade(), row) else {
                return;
            };
            if m.selecting.get() {
                return;
            }
            let address = m
                .rows
                .borrow()
                .iter()
                .find(|r| &r.row == row)
                .map(|r| r.address.clone());
            if let Some(a) = address {
                m.select(&a);
            }
        });
        // Groups.
        let me = self.me.clone();
        self.side.groups.connect_row_selected(move |_, row| {
            let (Some(m), Some(row)) = (me.upgrade(), row) else {
                return;
            };
            if m.selecting.get() {
                return;
            }
            let id = m
                .group_rows
                .borrow()
                .iter()
                .find(|r| &r.row == row)
                .map(|r| r.id.clone());
            if let Some(id) = id {
                m.select_group(&id);
            }
        });
        // Network, Security words, Security, Settings.
        let me = self.me.clone();
        self.side.nav.connect_row_selected(move |_, row| {
            let (Some(m), Some(row)) = (me.upgrade(), row) else {
                return;
            };
            if m.selecting.get() {
                return;
            }
            match row.index() {
                NAV_NETWORK => m.show_page(Page::Network),
                NAV_WORDS => m.show_page(Page::Words),
                NAV_SECURITY => m.show_page(Page::Security),
                NAV_SETTINGS => m.show_page(Page::Settings),
                _ => {}
            }
        });
        let me = self.me.clone();
        self.side.node_button.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.show_network();
            }
        });
        let me = self.me.clone();
        self.side.you_button.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.select_nav(NAV_WORDS, Page::Words);
            }
        });
        let me = self.me.clone();
        self.side.new_group.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.new_group();
            }
        });

        // Search filters friends and groups.
        let me = self.me.clone();
        self.side.friends.set_filter_func(move |row| {
            let Some(m) = me.upgrade() else { return true };
            let query = m.side.search.text().to_lowercase();
            if query.is_empty() {
                return true;
            }
            let rows = m.rows.borrow();
            rows.iter()
                .find(|r| &r.row == row)
                .is_none_or(|r| r.name.text().to_lowercase().contains(&query))
        });
        let me = self.me.clone();
        self.side.groups.set_filter_func(move |row| {
            let Some(m) = me.upgrade() else { return true };
            let query = m.side.search.text().to_lowercase();
            if query.is_empty() {
                return true;
            }
            let rows = m.group_rows.borrow();
            let groups = m.groups.borrow();
            rows.iter().find(|r| &r.row == row).is_none_or(|r| {
                groups
                    .iter()
                    .find(|g| g.id == r.id)
                    .is_none_or(|g| g.name.to_lowercase().contains(&query))
            })
        });
        let (friends, groups) = (self.side.friends.clone(), self.side.groups.clone());
        self.side.search.connect_search_changed(move |_| {
            friends.invalidate_filter();
            groups.invalidate_filter();
        });

        // Chat header buttons.
        let me = self.me.clone();
        self.chat.routes.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.open_routes();
            }
        });
        let me = self.me.clone();
        self.chat.members.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.open_members();
            }
        });
        let me = self.me.clone();
        self.chat.banner_action.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.open_check_words();
            }
        });

        // Composer.
        let me = self.me.clone();
        self.chat.entry.connect_activate(move |_| {
            if let Some(m) = me.upgrade() {
                m.send();
            }
        });
        let me = self.me.clone();
        self.chat.send.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.send();
            }
        });

        // The away card.
        let me = self.me.clone();
        self.away.try_now.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.try_now();
            }
        });
        let me = self.me.clone();
        self.away.stop.connect_clicked(move |_| {
            if let Some(m) = me.upgrade() {
                m.stop_trying();
            }
        });

        // Keep the newest message in view unless the person scrolled up;
        // "New messages below" goes once they are back at the bottom.
        let adj = self.chat.scroller.vadjustment();
        let stick = self.stick_bottom.clone();
        adj.connect_changed(move |a| {
            if stick.get() {
                a.set_value(a.upper() - a.page_size());
            }
        });
        let stick = self.stick_bottom.clone();
        let toast = self.chat.toast.clone();
        adj.connect_value_changed(move |a| {
            let bottom = a.value() >= a.upper() - a.page_size() - 24.0;
            stick.set(bottom);
            if bottom && toast.jump.get() {
                toast.hide();
            }
        });
        let me = self.me.clone();
        self.chat.toast.connect_clicked(move || {
            if let Some(m) = me.upgrade() {
                if m.chat.toast.jump.get() {
                    m.chat.toast.hide();
                    m.scroll_to_bottom();
                }
            }
        });

        self.style.watch(self, |m| {
            for r in m.rows.borrow().iter() {
                r.avatar.refresh_motion(&m.style);
            }
            if !m.style.motion_allowed() {
                m.chat.route.flush();
            }
            m.update_node_chip();
            // Technical details may have changed: chips say more or less.
            let ids: Vec<String> = m.shown.borrow().keys().cloned().collect();
            for id in ids {
                m.update_row(&id, false);
            }
        });
    }

    /// Run `job` on the engine thread; `done` gets the answer on the main
    /// loop if this window still exists.
    fn call<T, J, D>(&self, job: J, done: D)
    where
        T: Send + 'static,
        J: FnOnce(&dyn RavenEngine) -> Result<T, ApiError> + Send + 'static,
        D: FnOnce(&Rc<MainWindow>, Result<T, ApiError>) + 'static,
    {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.call(job, move |_, result| {
            if let Some(m) = me.upgrade() {
                done(&m, result);
            }
        });
    }

    fn contact(&self, address: &str) -> Option<Contact> {
        self.contacts
            .borrow()
            .iter()
            .find(|c| c.address == address)
            .cloned()
    }

    fn group(&self, id: &str) -> Option<Group> {
        self.groups.borrow().iter().find(|g| g.id == id).cloned()
    }

    fn name_of(&self, address: &str) -> String {
        self.contact(address).map(|c| c.name).unwrap_or_default()
    }

    fn open_friend(&self) -> Option<String> {
        match self.open.borrow().as_ref() {
            Some(Open::Friend(a)) => Some(a.clone()),
            _ => None,
        }
    }

    fn open_group(&self) -> Option<String> {
        match self.open.borrow().as_ref() {
            Some(Open::Group(g)) => Some(g.clone()),
            _ => None,
        }
    }

    // ------------------------------------------------------------ you

    /// Your fingerprint under "You" (the old code, as the other apps show it).
    fn load_me(&self) {
        self.call(
            |e| e.identity_me(),
            |m, result| {
                if let Ok(me) = result {
                    if let Some(n) = me.display_name.as_deref().filter(|n| !n.is_empty()) {
                        m.side.you_title.set_text(&trf("you.row", &[("name", n)]));
                    }
                    let fp = trf("you.fingerprint", &[("code", &me.old_code)]);
                    m.side.you_fingerprint.set_text(&fp);
                    super::a11y::set_label(
                        &m.side.you_button,
                        &format!("{}, {fp}", m.side.you_title.text()),
                    );
                    m.words_page.set_me(&me);
                    m.settings_page.set_me(&me);
                }
            },
        );
    }

    // ------------------------------------------------------------ friends

    /// Re-read the friends from the engine and update the rows in place
    /// (rebuild only when friends were added or removed).
    pub fn refresh_contacts(&self) {
        self.refresh_contacts_then(|_| {});
    }

    /// The same, then `then` once the new list is on screen (also after an
    /// error, with the old list).
    fn refresh_contacts_then(&self, then: impl FnOnce(&Rc<MainWindow>) + 'static) {
        self.call(
            |e| e.contacts_list(),
            move |m, result| {
                match result {
                    Ok(list) => m.apply_contacts(list),
                    Err(e) => m.toast(&e.message, "bad"),
                }
                then(m);
            },
        );
    }

    fn apply_contacts(&self, list: Vec<Contact>) {
        let same = {
            let rows = self.rows.borrow();
            rows.len() == list.len() && rows.iter().zip(&list).all(|(r, c)| r.address == c.address)
        };
        if !same {
            self.selecting.set(true);
            // No borrow is held while GTK runs: removing the selected row
            // emits row-selected, appending runs the filter function.
            let old: Vec<_> = self.rows.borrow_mut().drain(..).collect();
            for r in old {
                self.side.friends.remove(&r.row);
            }
            for c in &list {
                let r = super::sidebar::FriendRow::new(c);
                self.side.friends.append(&r.row);
                self.rows.borrow_mut().push(r);
            }
            if let Some(sel) = self.open_friend().filter(|_| self.page.get() == Page::Chat) {
                let row = self
                    .rows
                    .borrow()
                    .iter()
                    .find(|r| r.address == sel)
                    .map(|r| r.row.clone());
                if let Some(row) = row {
                    self.side.friends.select_row(Some(&row));
                }
            }
            self.selecting.set(false);
            // A search typed meanwhile applies to the new rows too.
            self.side.friends.invalidate_filter();
        }
        let empty = list.is_empty();
        self.unread
            .replace(list.iter().map(|c| (c.address.clone(), c.unread)).collect());
        // The open friend was removed (here or in the terminal): no chat.
        let gone = self
            .open_friend()
            .is_some_and(|a| !list.iter().any(|c| c.address == a));
        self.contacts.replace(list);
        if gone {
            self.open.replace(None);
            self.clear_chat();
        }
        self.update_row_texts();
        if self.page.get() == Page::Chat && self.open_group().is_none() {
            self.content
                .set_visible_child_name(if empty && self.open.borrow().is_none() {
                    "none"
                } else {
                    "chat"
                });
        }
        if self.open_friend().is_some() {
            self.update_header();
            self.update_away();
        }
        self.update_badges();
        if gone && self.page.get() == Page::Chat {
            let first = self.contacts.borrow().first().map(|c| c.address.clone());
            match first {
                Some(a) => self.select_row_for(&a),
                None => self.content.set_visible_child_name("none"),
            }
            return;
        }
        self.read_open_chat();
    }

    fn update_row_texts(&self) {
        let now = timefmt::now_ms();
        let contacts = self.contacts.borrow();
        for (r, c) in self.rows.borrow().iter().zip(contacts.iter()) {
            r.update(c, now, &self.style);
        }
    }

    /// The open chat is on screen: it counts as read up to its newest
    /// message, here and in the terminal app (`messages.mark_read`,
    /// `groups.mark_read`).
    fn read_open_chat(&self) {
        if !self.window.is_visible() || self.page.get() != Page::Chat {
            return;
        }
        if let Some(id) = self.open_group() {
            let unread = self.group(&id).map_or(0, |g| g.unread);
            if unread == 0 {
                return;
            }
            if let Some(g) = self.groups.borrow_mut().iter_mut().find(|g| g.id == id) {
                g.unread = 0;
            }
            self.update_group_rows();
            self.call(move |e| e.groups_mark_read(&id), |_, _| {});
            return;
        }
        let Some(address) = self.open_friend() else {
            return;
        };
        if self.unread.borrow().get(&address).copied().unwrap_or(0) == 0 {
            return;
        }
        let newest = self
            .known
            .borrow()
            .values()
            .filter(|m| m.address == address)
            .max_by_key(|m| m.at_ms)
            .map(|m| m.id.clone());
        let Some(newest) = newest else {
            return;
        };
        self.unread.borrow_mut().insert(address.clone(), 0);
        self.update_badges();
        self.call(
            move |e| e.messages_mark_read(&address, Some(&newest)),
            |_, _| {},
        );
    }

    fn update_badges(&self) {
        let unread = self.unread.borrow();
        let contacts = self.contacts.borrow();
        let paths = self.last_path.borrow();
        for r in self.rows.borrow().iter() {
            let in_touch = contacts
                .iter()
                .find(|c| c.address == r.address)
                .is_some_and(|c| c.presence == Presence::InTouch);
            let path = paths
                .get(&r.address)
                .map(|(_, kind)| present::path_style(kind.as_deref()));
            r.set_right(unread.get(&r.address).copied().unwrap_or(0), path, in_touch);
        }
    }

    // ------------------------------------------------------------ groups

    /// Read the groups (`groups.list`). An engine without groups, or a
    /// release build (`available: false`), shows no Groups section.
    pub fn refresh_groups(&self) {
        self.refresh_groups_then(|_| {});
    }

    /// The same, then `then` once the list is on screen.
    fn refresh_groups_then(&self, then: impl FnOnce(&Rc<MainWindow>) + 'static) {
        self.call(
            |e| e.groups_list(),
            move |m, result| {
                match result {
                    Ok(l) if l.available => {
                        m.groups_available.set(true);
                        m.side.set_groups_available(true);
                        m.apply_groups(l.groups);
                    }
                    _ => {
                        m.groups_available.set(false);
                        m.side.set_groups_available(false);
                    }
                }
                then(m);
            },
        );
    }

    fn apply_groups(&self, list: Vec<Group>) {
        let same = {
            let rows = self.group_rows.borrow();
            rows.len() == list.len() && rows.iter().zip(&list).all(|(r, g)| r.id == g.id)
        };
        if !same {
            self.selecting.set(true);
            let old: Vec<_> = self.group_rows.borrow_mut().drain(..).collect();
            for r in old {
                self.side.groups.remove(&r.row);
            }
            for g in &list {
                let r = GroupRow::new(g);
                if let Some(members) = self.members.borrow().get(&g.id) {
                    r.set_members(members);
                }
                self.side.groups.append(&r.row);
                self.group_rows.borrow_mut().push(r);
            }
            if let Some(sel) = self.open_group().filter(|_| self.page.get() == Page::Chat) {
                let row = self
                    .group_rows
                    .borrow()
                    .iter()
                    .find(|r| r.id == sel)
                    .map(|r| r.row.clone());
                if let Some(row) = row {
                    self.side.groups.select_row(Some(&row));
                }
            }
            self.selecting.set(false);
            self.side.groups.invalidate_filter();
            // Stacked avatars need the members.
            let missing: Vec<String> = list
                .iter()
                .filter(|g| !self.members.borrow().contains_key(&g.id))
                .map(|g| g.id.clone())
                .collect();
            for id in missing {
                self.load_members(&id, false);
            }
        }
        self.groups.replace(list);
        self.update_group_rows();
        if let Some(id) = self.open_group() {
            if self.group(&id).is_some() {
                self.update_group_header();
            }
        }
        self.read_open_chat();
    }

    fn update_group_rows(&self) {
        let groups = self.groups.borrow();
        for r in self.group_rows.borrow().iter() {
            if let Some(g) = groups.iter().find(|g| g.id == r.id) {
                r.update(g);
            }
        }
    }

    /// `groups.get`: the members (sidebar avatars; the header when open).
    fn load_members(&self, id: &str, then_header: bool) {
        let gid = id.to_string();
        self.call(
            move |e| e.groups_get(&gid),
            move |m, result| {
                if let Ok(d) = result {
                    let id = d.group.id.clone();
                    if let Some(r) = m.group_rows.borrow().iter().find(|r| r.id == id) {
                        r.set_members(&d.members);
                    }
                    m.members.borrow_mut().insert(id.clone(), d.members);
                    if let Some(g) = m.groups.borrow_mut().iter_mut().find(|g| g.id == id) {
                        *g = d.group;
                    }
                    if then_header && m.open_group().as_deref() == Some(id.as_str()) {
                        m.update_group_header();
                    }
                }
            },
        );
    }

    fn select_group(&self, id: &str) {
        self.page.set(Page::Chat);
        self.selecting.set(true);
        self.side.nav.unselect_all();
        self.side.friends.unselect_all();
        self.selecting.set(false);
        self.content.set_visible_child_name("chat");
        self.chat.route.flush();
        self.open.replace(Some(Open::Group(id.to_string())));
        self.chat.enter();
        self.update_group_header();
        self.load_members(id, true);
        self.load_group_history(id);
        let name = self.group(id).map(|g| g.name).unwrap_or_default();
        self.chat
            .entry
            .set_placeholder_text(Some(&trf("home.composer", &[("name", &name)])));
        self.chat.e2e.set_text(tr("chat.e2e.group"));
        self.chat.banner.set_reveal_child(false);
        self.chat.entry.grab_focus();
    }

    fn update_group_header(&self) {
        let Some(id) = self.open_group() else { return };
        let Some(g) = self.group(&id) else { return };
        if self.page.get() == Page::Chat {
            self.title.set_text(&g.name);
        }
        widgets::clear_box(&self.chat.avatar_slot);
        let stack = gtk::Fixed::new();
        stack.set_size_request(54, 40);
        let members = self.members.borrow().get(&id).cloned().unwrap_or_default();
        for (i, m) in members.iter().filter(|m| !m.me).take(4).enumerate() {
            let a = Avatar::sized(&m.name, &m.address, AvatarSize::Small);
            a.stacked();
            stack.put(&a.root, i as f64 * 11.0, 9.0);
        }
        self.chat.avatar_slot.append(&stack);
        self.chat.name.set_text(&g.name);
        self.chat.set_verified(None);
        self.chat.sub.set_text(&groupview::header_line(&g));
        self.chat.sub.remove_css_class("away");
        self.chat.pill.hide();
        self.chat.routes.set_visible(false);
        self.chat.members.set_visible(true);
        self.chat.route.set(
            tr("you.title"),
            &trf("group.members", &[("n", &g.members.to_string())]),
            &trf(
                "group.route",
                &[("n", &g.members.saturating_sub(1).to_string())],
            ),
            PathStyle {
                class: None,
                icon: Icon::Friend,
            },
            false,
        );
        // Left or removed: the history stays, the composer goes.
        match groupview::closed_banner(&g) {
            Some(text) => {
                self.chat.closed.set_text(text);
                self.chat.closed.set_visible(true);
                self.chat.composer_box.set_visible(false);
            }
            None => {
                self.chat.closed.set_visible(false);
                self.chat.composer_box.set_visible(true);
            }
        }
    }

    fn load_group_history(&self, id: &str) {
        self.clear_chat();
        let gid = id.to_string();
        let want = id.to_string();
        self.call(
            move |e| e.groups_history(&gid, None, HISTORY_LIMIT),
            move |m, result| {
                if m.open_group().as_deref() != Some(want.as_str()) {
                    return;
                }
                m.clear_chat();
                match result {
                    Ok(list) => {
                        for msg in list {
                            m.append_group(&msg, false);
                        }
                    }
                    Err(e) => m.toast(&e.message, "bad"),
                }
                m.scroll_to_bottom();
                m.read_open_chat();
            },
        );
    }

    /// Re-read the open group's history after a `group_changed`: rows are
    /// updated in place (the ring fills), new ones appended.
    fn reload_group_history(&self, id: &str) {
        let gid = id.to_string();
        let want = id.to_string();
        self.call(
            move |e| e.groups_history(&gid, None, HISTORY_LIMIT),
            move |m, result| {
                if m.open_group().as_deref() != Some(want.as_str()) {
                    return;
                }
                if let Ok(list) = result {
                    for msg in list {
                        let exists = m.shown.borrow().contains_key(&msg.id);
                        if exists {
                            m.group_known
                                .borrow_mut()
                                .insert(msg.id.clone(), msg.clone());
                            let row = m.shown.borrow().get(&msg.id).cloned();
                            if let Some(gm) = row.as_ref().and_then(|r| r.group.as_ref()) {
                                gm.set(&msg, &m.style);
                            }
                        } else {
                            m.append_group(&msg, true);
                        }
                    }
                }
            },
        );
    }

    fn append_group(&self, msg: &GroupMessage, arriving: bool) {
        if self.shown.borrow().contains_key(&msg.id) {
            return;
        }
        self.group_known
            .borrow_mut()
            .insert(msg.id.clone(), msg.clone());
        let row = Rc::new(chat_view::group_row(msg, &self.style));
        MsgRow::connect_group_details(&row);
        self.push_row(msg.at_ms, &row.root);
        if arriving {
            if msg.mine {
                self.chat.lift(&row.root);
            } else {
                self.chat.arrive(&row.root);
            }
        }
        self.shown.borrow_mut().insert(msg.id.clone(), row);
        self.chat.empty.set_visible(false);
    }

    fn new_group(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let contacts = self.contacts.borrow().clone();
        let me = self.me.clone();
        group_sheets::new_group(
            &ctx,
            self.window.upcast_ref(),
            contacts,
            Box::new(move |id| {
                if let Some(m) = me.upgrade() {
                    // Open it once the list has it.
                    m.refresh_groups_then(move |m| m.select_group_row(&id));
                }
            }),
        );
    }

    fn select_group_row(&self, id: &str) {
        let row = self
            .group_rows
            .borrow()
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.row.clone());
        if let Some(row) = row {
            self.selecting.set(true);
            self.side.groups.select_row(Some(&row));
            self.selecting.set(false);
        }
        self.select_group(id);
    }

    fn open_members(&self) {
        let (Some(ctx), Some(id)) = (self.ctx.upgrade(), self.open_group()) else {
            return;
        };
        let Some(g) = self.group(&id) else { return };
        let members = self.members.borrow().get(&id).cloned().unwrap_or_default();
        let contacts = self.contacts.borrow().clone();
        let me = self.me.clone();
        group_sheets::members(
            &ctx,
            self.window.upcast_ref(),
            g,
            members,
            contacts,
            Box::new(move || {
                if let Some(m) = me.upgrade() {
                    m.refresh_groups();
                    if let Some(id) = m.open_group() {
                        m.load_members(&id, true);
                    }
                }
            }),
        );
    }

    // ------------------------------------------------------------ chats

    /// Select a friend from code (after adding one, at start, the tray).
    pub fn select_row_for(&self, address: &str) {
        let row = self
            .rows
            .borrow()
            .iter()
            .find(|r| r.address == address)
            .map(|r| r.row.clone());
        if let Some(row) = row {
            self.selecting.set(true);
            self.side.friends.select_row(Some(&row));
            self.selecting.set(false);
        }
        self.select(address);
    }

    fn select(&self, address: &str) {
        self.page.set(Page::Chat);
        self.selecting.set(true);
        self.side.nav.unselect_all();
        self.side.groups.unselect_all();
        self.selecting.set(false);
        self.content.set_visible_child_name("chat");
        // Finish pending route runs for the old chat first.
        self.chat.route.flush();
        self.open.replace(Some(Open::Friend(address.to_string())));
        self.chat.enter();
        self.chat.routes.set_visible(true);
        self.chat.members.set_visible(false);
        self.chat.closed.set_visible(false);
        self.chat.composer_box.set_visible(true);
        self.update_header();
        // Read once its history is on screen.
        self.load_history(address);
        let name = self.name_of(address);
        self.chat
            .entry
            .set_placeholder_text(Some(&trf("home.composer", &[("name", &name)])));
        self.chat.e2e.set_text(&trf("chat.e2e", &[("name", &name)]));
        self.chat.entry.grab_focus();
    }

    fn last_path_of(&self, address: &str) -> Option<(String, Option<String>)> {
        self.last_path.borrow().get(address).cloned()
    }

    fn update_header(&self) {
        let Some(a) = self.open_friend() else { return };
        let Some(c) = self.contact(&a) else { return };
        if self.page.get() == Page::Chat {
            self.title.set_text(&c.name);
        }
        widgets::clear_box(&self.chat.avatar_slot);
        let av = Avatar::sized(&c.name, &c.address, AvatarSize::Large);
        av.set_presence(c.presence, false, &self.style);
        self.chat.avatar_slot.append(&av.root);
        self.chat.name.set_text(&c.name);
        self.chat.set_verified(Some(c.verified));
        let path = self.last_path_of(&a);
        let now = timefmt::now_ms();
        self.chat.sub.set_text(&present::header_line(
            &c,
            path.as_ref().map(|(w, _)| w.as_str()),
            now,
        ));
        let waiting = c.waiting.is_some_and(|w| w.count > 0);
        if waiting {
            self.chat.sub.add_css_class("away");
        } else {
            self.chat.sub.remove_css_class("away");
        }
        let in_touch = c.presence == Presence::InTouch && !waiting;
        // The pill: how the last message travelled, while in touch.
        match path.as_ref().filter(|_| in_touch) {
            Some((words, kind)) => {
                self.chat
                    .pill
                    .set(words, present::path_style(kind.as_deref()), words)
            }
            None => self.chat.pill.hide(),
        }
        // The route line.
        let (text, style) = if waiting {
            let w = c.waiting.unwrap_or(raven_desktop::api::Waiting {
                count: 0,
                until_ms: now,
            });
            (
                trf(
                    "home.waiting",
                    &[
                        ("n", &w.count.to_string()),
                        ("time", &timefmt::deadline(now, w.until_ms)),
                    ],
                ),
                present::path_style(Some("friend")),
            )
        } else {
            match &path {
                Some((words, kind)) => (words.clone(), present::path_style(kind.as_deref())),
                None => (String::new(), present::path_style(None)),
            }
        };
        self.chat
            .route
            .set(tr("you.title"), &c.name, &text, style, !in_touch);
        self.chat
            .banner_text
            .set_text(&trf("home.banner.notVerified", &[("name", &c.name)]));
        self.chat.banner.set_reveal_child(!c.verified);
    }

    // ------------------------------------------------------------ messages

    fn clear_chat(&self) {
        self.chat.clear();
        self.shown.borrow_mut().clear();
        self.group_known.borrow_mut().clear();
        self.last_day.set(None);
        self.rows_count.set(0);
        self.away_message.replace(None);
    }

    /// Append a row, with a day line before it when its day differs from
    /// the row before.
    fn push_row(&self, at_ms: u64, row: &gtk::Box) {
        let day = timefmt::local_day(at_ms);
        if self.last_day.replace(Some(day)) != Some(day) {
            self.chat.push(&chat_view::day_line(&timefmt::day_line(
                timefmt::now_ms(),
                at_ms,
            )));
        }
        self.chat.push(row);
        // Keep the view light: drop the oldest rows past the limit.
        let n = self.rows_count.get() + 1;
        self.rows_count.set(n);
        if n > MAX_ROWS {
            if let Some(first) = self.chat.list.first_child() {
                self.chat.list.remove(&first);
            }
        }
    }

    fn load_history(&self, address: &str) {
        self.clear_chat();
        self.chat.empty.set_visible(false);
        let (a, address) = (address.to_string(), address.to_string());
        self.call(
            move |e| e.messages_history(&a, None, HISTORY_LIMIT),
            move |m, result| {
                if m.open_friend().as_deref() != Some(address.as_str()) {
                    return; // another chat was chosen meanwhile
                }
                // Rebuilt from the answer, which already has what arrived
                // while it was on its way.
                m.clear_chat();
                match result {
                    Ok(list) => {
                        for msg in list {
                            m.note_path(&msg);
                            m.known.borrow_mut().insert(msg.id.clone(), msg.clone());
                            m.append_message(&msg, false);
                        }
                    }
                    Err(e) => m.toast(&e.message, "bad"),
                }
                let name = m.name_of(&address);
                m.chat
                    .empty
                    .set_text(&trf("home.emptyChat", &[("name", &name)]));
                m.chat.empty.set_visible(m.shown.borrow().is_empty());
                m.update_header();
                m.update_away();
                m.update_badges();
                m.scroll_to_bottom();
                m.read_open_chat();
            },
        );
    }

    /// Remember how a message travelled (the engine's words and kind).
    fn note_path(&self, m: &Message) {
        if m.state != MessageState::Delivered {
            return;
        }
        if let Some(p) = m.path.as_deref().filter(|p| !p.is_empty()) {
            let newer = self
                .known
                .borrow()
                .values()
                .filter(|k| k.address == m.address && k.path.is_some() && k.id != m.id)
                .all(|k| k.at_ms <= m.at_ms);
            if newer {
                self.last_path
                    .borrow_mut()
                    .insert(m.address.clone(), (p.to_string(), m.path_kind.clone()));
            }
        }
    }

    /// Add a 1:1 bubble.
    fn append_message(&self, m: &Message, arriving: bool) {
        if self.shown.borrow().contains_key(&m.id) {
            return;
        }
        let id = m.id.clone();
        let me = self.me.clone();
        let row = Rc::new(chat_view::message_row(m, &self.style, move || {
            if let Some(w) = me.upgrade() {
                w.open_details(&id);
            }
        }));
        self.push_row(m.at_ms, &row.root);
        if arriving {
            if m.direction == Direction::Out {
                self.chat.lift(&row.root);
            } else {
                self.chat.arrive(&row.root);
            }
        }
        self.shown.borrow_mut().insert(m.id.clone(), row);
        self.update_row(&m.id, false);
        self.chat.empty.set_visible(false);
    }

    /// Update a 1:1 row's tick and chip. `arriving`: a receipt just came.
    fn update_row(&self, id: &str, arriving: bool) {
        let row = self.shown.borrow().get(id).cloned();
        let Some(row) = row else { return };
        let Some(m) = self.known.borrow().get(id).cloned() else {
            return;
        };
        let name = self.name_of(&m.address);
        row.update(&m, &name, self.style.details(), arriving, &self.style);
    }

    fn update_waiting_ticks(&self) {
        let ids: Vec<String> = self
            .shown
            .borrow()
            .keys()
            .filter(|id| {
                self.known
                    .borrow()
                    .get(*id)
                    .is_some_and(|m| m.state == MessageState::Waiting)
            })
            .cloned()
            .collect();
        for id in ids {
            self.update_row(&id, false);
        }
    }

    /// The away card under the open friend's waiting messages.
    fn update_away(&self) {
        let Some(a) = self.open_friend() else { return };
        let Some(c) = self.contact(&a) else { return };
        let waiting = c.waiting.filter(|w| w.count > 0);
        widgets::clear_box(&self.chat.tail);
        let Some(w) = waiting else {
            self.away_message.replace(None);
            return;
        };
        self.away.set(&c.name, w.until_ms, w.count);
        self.chat.tail.append(&self.away.root);
        let newest = self
            .known
            .borrow()
            .values()
            .filter(|m| m.address == a && m.state == MessageState::Waiting)
            .max_by_key(|m| m.at_ms)
            .map(|m| m.id.clone());
        self.away_message.replace(newest.clone());
        // Who can carry it, and the engine's own words about the newest.
        let (addr, name) = (a.clone(), c.name.clone());
        self.call(
            move |e| {
                let routes = e.contacts_routes(&addr)?;
                let details = match newest {
                    Some(id) => e.messages_details(&id).ok(),
                    None => None,
                };
                Ok((routes, details))
            },
            move |m, result| {
                if m.open_friend().as_deref() != Some(a.as_str()) {
                    return;
                }
                if let Ok((routes, details)) = result {
                    let cov = routes.carriers.coverage.map(|c| (c.kind, c.name, c.until));
                    m.away.set_coverage(&name, &a, cov);
                    if let Some(d) = details {
                        m.away
                            .set_details(&d.status_text, &d.why_text, &d.action_text);
                    }
                }
            },
        );
    }

    /// Try now: every waiting message to the open friend, again.
    fn try_now(&self) {
        let Some(a) = self.open_friend() else { return };
        let ids: Vec<String> = self
            .known
            .borrow()
            .values()
            .filter(|m| m.address == a && m.state == MessageState::Waiting)
            .map(|m| m.id.clone())
            .collect();
        self.away.try_now.set_sensitive(false);
        self.call(
            move |e| {
                let mut last = Err(ApiError::internal("nothing waits"));
                for id in ids {
                    last = e.messages_retry(&id);
                }
                last
            },
            |m, result| {
                m.away.try_now.set_sensitive(true);
                match result {
                    Ok(msg) => m.message_changed(msg),
                    Err(e) => m.toast(&e.message, "bad"),
                }
            },
        );
    }

    /// Stop trying (one message waits): it ends here and is not sent.
    fn stop_trying(&self) {
        let Some(id) = self.away_message.borrow().clone() else {
            return;
        };
        self.call(
            move |e| e.messages_stop(&id),
            |m, result| match result {
                Ok(msg) => m.message_changed(msg),
                Err(e) => m.toast(&e.message, "bad"),
            },
        );
    }

    fn scroll_to_bottom(&self) {
        self.stick_bottom.set(true);
        let adj = self.chat.scroller.vadjustment();
        chat_view::after_layout(&self.chat.scroller, move || {
            adj.set_value(adj.upper() - adj.page_size());
        });
    }

    fn send(&self) {
        let text = self.chat.entry.text().trim().to_string();
        let open = self.open.borrow().clone();
        let Some(open) = open else { return };
        if text.is_empty() || self.sending.replace(true) {
            return;
        }
        self.chat.entry.set_text("");
        let t = text.clone();
        match open {
            Open::Friend(address) => self.call(
                move |e| e.messages_send(&address, &t),
                move |m, result| {
                    m.sending.set(false);
                    match result {
                        Ok(msg) => {
                            m.known.borrow_mut().insert(msg.id.clone(), msg.clone());
                            if m.open_friend().as_deref() == Some(msg.address.as_str()) {
                                m.append_message(&msg, true);
                                m.scroll_to_bottom();
                            }
                        }
                        Err(e) => m.give_back(&text, &e),
                    }
                },
            ),
            Open::Group(id) => self.call(
                move |e| e.groups_send(&id, &t),
                move |m, result| {
                    m.sending.set(false);
                    match result {
                        Ok((msg, note)) => {
                            if m.open_group().as_deref() == Some(msg.group.as_str()) {
                                m.append_group(&msg, true);
                                m.scroll_to_bottom();
                            }
                            if let Some(n) = note {
                                m.toast(&n, "wait");
                            }
                            m.refresh_groups();
                        }
                        Err(e) => m.give_back(&text, &e),
                    }
                },
            ),
        }
    }

    /// Not sent: give the text back unless something new was typed meanwhile.
    fn give_back(&self, text: &str, e: &ApiError) {
        if self.chat.entry.text().is_empty() {
            self.chat.entry.set_text(text);
            self.chat.entry.set_position(-1);
        }
        self.toast(&e.message, "bad");
    }

    /// A message changed outside the event stream (Message details, Try now).
    pub fn message_changed(&self, m: Message) {
        let id = m.id.clone();
        let is_new = !self.known.borrow().contains_key(&id);
        let for_open = self.open_friend().as_deref() == Some(m.address.as_str());
        self.known.borrow_mut().insert(id.clone(), m.clone());
        if is_new && for_open {
            self.append_message(&m, false);
            self.scroll_to_bottom();
        } else {
            self.update_row(&id, false);
        }
        self.refresh_contacts();
    }

    fn open_details(self: &Rc<Self>, id: &str) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let Some(m) = self.known.borrow().get(id).cloned() else {
            return;
        };
        let name = self.name_of(&m.address);
        let me = Rc::downgrade(self);
        message_details::show(
            &ctx,
            self.window.upcast_ref(),
            id,
            &name,
            Box::new(move |msg| {
                if let Some(me) = me.upgrade() {
                    me.message_changed(msg);
                }
            }),
        );
    }

    fn open_check_words(&self) {
        let (Some(ctx), Some(a)) = (self.ctx.upgrade(), self.open_friend()) else {
            return;
        };
        let Some(c) = self.contact(&a) else { return };
        let me = self.me.clone();
        check_words::show(
            &ctx,
            self.window.upcast_ref(),
            &c,
            Box::new(move || {
                if let Some(me) = me.upgrade() {
                    me.refresh_contacts();
                }
            }),
        );
    }

    /// A friend's routes panel (the chat header's Routes).
    fn open_routes(&self) {
        let (Some(ctx), Some(a)) = (self.ctx.upgrade(), self.open_friend()) else {
            return;
        };
        let Some(c) = self.contact(&a) else { return };
        let contacts = self.contacts.borrow().clone();
        let me = self.me.clone();
        routes::show(
            &ctx,
            self.window.upcast_ref(),
            &c,
            contacts,
            Box::new(move || {
                if let Some(me) = me.upgrade() {
                    me.refresh_contacts();
                }
            }),
        );
    }

    // ------------------------------------------------------------ pages

    pub fn show_page(&self, page: Page) {
        let was = self.page.replace(page);
        self.selecting.set(true);
        if page != Page::Chat {
            self.side.friends.unselect_all();
            self.side.groups.unselect_all();
        } else {
            self.side.nav.unselect_all();
        }
        self.selecting.set(false);
        let (name, title) = match page {
            Page::Chat => {
                let empty = self.contacts.borrow().is_empty() && self.open.borrow().is_none();
                self.content
                    .set_visible_child_name(if empty { "none" } else { "chat" });
                return;
            }
            Page::Network => {
                self.network_page.refresh();
                ("network", tr("network.title"))
            }
            Page::Security => {
                self.security_page.refresh();
                ("security", tr("security.title"))
            }
            Page::Settings => {
                self.settings_page.refresh();
                ("settings", tr("settings.title"))
            }
            Page::Words => ("words", tr("nav.securityWords")),
        };
        self.title.set_text(title);
        self.content.set_visible_child_name(name);
        if was != page {
            if let Some(child) = self.content.visible_child() {
                calm::play_class(&child, &self.style, "view-in", tokens().calm.view_in);
            }
        }
    }

    fn select_nav(&self, index: i32, page: Page) {
        let row = self.side.nav.row_at_index(index);
        self.selecting.set(true);
        self.side.nav.select_row(row.as_ref());
        self.selecting.set(false);
        self.show_page(page);
    }

    /// Open the Network page (the window menu, the tray, the heartbeat).
    pub fn show_network(&self) {
        self.select_nav(NAV_NETWORK, Page::Network);
    }

    /// Open Settings, at a section when given (Ctrl+comma, the tray's Lock).
    pub fn show_settings(&self, section: Option<raven_desktop::settingsview::Section>) {
        self.select_nav(NAV_SETTINGS, Page::Settings);
        if let Some(s) = section {
            self.settings_page.open(s);
        }
    }

    /// Security words (the sidebar's You, About's button).
    pub fn show_words(&self) {
        self.select_nav(NAV_WORDS, Page::Words);
    }

    /// The network summary: "n ways on" beside Network.
    pub fn set_network_summary(&self, on: Option<usize>) {
        self.side.set_ways_on(on);
    }

    /// After "Add a friend": show the new friend.
    pub fn friend_added(&self, address: &str) {
        self.refresh_contacts();
        self.select_row_for(address);
    }

    /// The friends in the sidebar's order with their unread counts (tray menu).
    pub fn tray_friends(&self) -> Vec<(Contact, u32)> {
        let unread = self.unread.borrow();
        self.contacts
            .borrow()
            .iter()
            .map(|c| (c.clone(), unread.get(&c.address).copied().unwrap_or(0)))
            .collect()
    }

    /// An engine error from outside the window (for example the tray).
    pub fn show_error(&self, text: &str) {
        self.toast(text, "bad");
    }

    /// A toast in the pill: above the composer in a chat, at the bottom of
    /// any other page.
    pub fn toast(&self, text: &str, tone: &str) {
        if self.page.get() == Page::Chat {
            self.chat.toast.show(text, tone);
        } else {
            self.page_toast.show(text, tone);
        }
    }

    // ------------------------------------------------------------ status

    fn update_node_chip(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let node = ctx.node.borrow().clone();
        let interval = tokens().durations.status_check_interval as u64;
        let chip = present::node_chip(&node, self.last_ok_ms.get(), timefmt::now_ms(), interval);
        let (text, ok) = match chip {
            NodeChip::Starting(t) => (t, false),
            NodeChip::Ok(t) => (t, true),
            NodeChip::Down(t) => (t, false),
        };
        self.side.node_text.set_text(&text);
        if ok {
            self.side.heart.remove_css_class("down");
            self.side.node_text.remove_css_class("warning");
        } else {
            self.side.heart.add_css_class("down");
            self.side.node_text.add_css_class("warning");
        }
        super::a11y::set_label(
            &self.side.node_button,
            &format!("{}, {text}", tr("app.name")),
        );
    }

    fn pulse_heart(&self) {
        calm::play_class(
            &self.side.heart,
            &self.style,
            "beat",
            tokens().durations.heartbeat_pulse,
        );
    }

    /// Called by the 1-second clock.
    pub fn clock_tick(&self, n: u64) {
        self.update_node_chip();
        if n.is_multiple_of(15) {
            self.update_row_texts();
            self.update_waiting_ticks();
            if self.open_friend().is_some() {
                self.update_header();
            }
        }
    }

    /// After the first poll: take the latest status check without motion.
    pub fn primed(&self, last_ok_ms: Option<u64>) {
        if let Some(at) = last_ok_ms {
            self.last_ok_ms.set(at.max(1));
        }
        self.update_node_chip();
    }

    /// The engine dropped events (`events.poll` said `reset`): read the
    /// friends, the groups, the open chat and the node status again.
    pub fn reload(&self) {
        self.refresh_contacts();
        self.refresh_groups();
        match self.open.borrow().clone() {
            Some(Open::Friend(a)) => self.load_history(&a),
            Some(Open::Group(g)) => self.load_group_history(&g),
            None => {}
        }
        self.call(
            |e| e.node_status(),
            |m, result| {
                if let (Ok(node), Some(ctx)) = (result, m.ctx.upgrade()) {
                    ctx.node.replace(node);
                }
                m.update_node_chip();
            },
        );
    }

    /// `events.poll` failed: say so in the pill and the heartbeat.
    pub fn engine_unreachable(&self, e: &ApiError) {
        self.last_ok_ms.set(0);
        self.update_node_chip();
        self.toast(&e.message, "bad");
    }

    // ------------------------------------------------------------ events

    pub fn handle_event(self: &Rc<Self>, ev: &Event) {
        match &ev.kind {
            EventKind::StatusOk => {
                self.last_ok_ms.set(ev.at_ms.max(1));
                self.pulse_heart();
                self.update_node_chip();
                if self.page.get() == Page::Network {
                    self.network_page.pulse();
                }
            }
            EventKind::SendStarted { id } => {
                if self.shown.borrow().contains_key(id) && self.open_friend().is_some() {
                    self.chat.route.play(Packet::Out, None);
                }
            }
            EventKind::Receipt {
                id,
                path,
                path_kind,
                ms,
                delivered_at_ms,
            } => self.on_receipt(id, path, path_kind, *ms, *delivered_at_ms),
            EventKind::Incoming { message } => self.on_incoming(message),
            EventKind::Outgoing { message } => {
                // Sent from the terminal app or the CLI: it appears in the
                // open chat (a receipt follows like for any of yours).
                self.known
                    .borrow_mut()
                    .insert(message.id.clone(), message.clone());
                if self.open_friend().as_deref() == Some(message.address.as_str()) {
                    self.append_message(message, false);
                    if self.stick_bottom.get() {
                        self.scroll_to_bottom();
                    }
                }
                self.refresh_contacts();
            }
            EventKind::Retry {
                id,
                address,
                attempts,
            } => {
                if let Some(r) = self.rows.borrow().iter().find(|r| &r.address == address) {
                    r.avatar.ring(&self.style);
                }
                let until = self
                    .known
                    .borrow()
                    .get(id)
                    .and_then(|m| m.until_ms)
                    .or_else(|| {
                        self.contact(address)
                            .and_then(|c| c.waiting)
                            .map(|w| w.until_ms)
                    });
                if let Some(m) = self.known.borrow_mut().get_mut(id) {
                    m.attempts = Some(*attempts);
                }
                let now = timefmt::now_ms();
                let text = match until {
                    Some(u) => trf(
                        "ticker.retry",
                        &[
                            ("name", &self.name_of(address)),
                            ("time", &timefmt::deadline(now, u)),
                        ],
                    ),
                    None => trf("ticker.retryNoTime", &[("name", &self.name_of(address))]),
                };
                self.toast(&text, "wait");
            }
            EventKind::Presence { address, presence } => {
                if let Some(r) = self.rows.borrow().iter().find(|r| &r.address == address) {
                    r.avatar.set_presence(*presence, true, &self.style);
                }
                self.refresh_contacts();
            }
            EventKind::State { id, state } => {
                let known = self.known.borrow().contains_key(id);
                if known && *state == MessageState::Waiting {
                    // The waiting deadline comes with the message: show the
                    // new state once it is here.
                    let (job_id, id, st) = (id.clone(), id.clone(), *state);
                    self.call(
                        move |e| e.messages_details(&job_id),
                        move |m, result| match result {
                            Ok(d) => {
                                m.known.borrow_mut().insert(id.clone(), d.message);
                                m.update_row(&id, false);
                                m.refresh_contacts();
                            }
                            Err(_) => m.set_state(&id, st),
                        },
                    );
                    return;
                }
                self.set_state(id, *state);
            }
            EventKind::Node { .. } => {
                self.update_node_chip();
                if self.page.get() == Page::Network {
                    self.network_page.refresh();
                }
                if self.page.get() == Page::Settings {
                    self.settings_page.node_changed();
                }
            }
            EventKind::Unread { address, unread } => {
                self.unread.borrow_mut().insert(address.clone(), *unread);
                self.update_badges();
                self.read_open_chat();
            }
            EventKind::GroupMessage { group, message } => self.on_group_message(group, message),
            EventKind::GroupChanged { group } => {
                self.refresh_groups();
                if self.open_group().as_deref() == Some(group.as_str()) {
                    self.load_members(group, true);
                    self.reload_group_history(group);
                }
            }
            EventKind::Unknown => {}
        }
    }

    /// A message changed state (from a `state` event).
    fn set_state(&self, id: &str, state: MessageState) {
        // A receipt already made it Delivered: its check fills when the
        // receipt has travelled back (`announce_receipt`), not now.
        let already = self
            .known
            .borrow()
            .get(id)
            .is_some_and(|m| m.state == MessageState::Delivered);
        if already && state == MessageState::Delivered {
            return;
        }
        if let Some(m) = self.known.borrow_mut().get_mut(id) {
            m.state = state;
        }
        self.update_row(id, false);
        if matches!(
            state,
            MessageState::Waiting | MessageState::Failed | MessageState::TimeRanOut
        ) {
            self.refresh_contacts();
        }
    }

    fn on_receipt(
        self: &Rc<Self>,
        id: &str,
        path: &str,
        path_kind: &str,
        ms: u64,
        delivered_at_ms: Option<u64>,
    ) {
        let known = self.known.borrow().get(id).cloned();
        if let Some(mut msg) = known {
            // A late delivery says when it came (E1).
            msg.delivered_at_ms = delivered_at_ms.or(msg.delivered_at_ms);
            self.receipt_for(msg, path, path_kind, ms);
            return;
        }
        // A message this window hasn't seen (sent from elsewhere): ask the
        // engine for it first.
        let (job_id, path, kind) = (id.to_string(), path.to_string(), path_kind.to_string());
        self.call(
            move |e| e.messages_details(&job_id),
            move |m, result| match result {
                Ok(d) => m.receipt_for(d.message, &path, &kind, ms),
                Err(_) => m.refresh_contacts(),
            },
        );
    }

    fn receipt_for(self: &Rc<Self>, mut msg: Message, path: &str, path_kind: &str, ms: u64) {
        let id = msg.id.clone();
        let address = msg.address.clone();
        let waiting_before = self
            .contact(&address)
            .and_then(|c| c.waiting)
            .map(|w| w.count)
            .unwrap_or(0);
        let was_waiting = msg.state == MessageState::Waiting;
        msg.state = MessageState::Delivered;
        msg.path = Some(path.to_string());
        if !path_kind.is_empty() {
            msg.path_kind = Some(path_kind.to_string());
        }
        msg.until_ms = None;
        self.known.borrow_mut().insert(id.clone(), msg.clone());
        self.note_path(&msg);
        let path = path.to_string();
        self.refresh_contacts_then(move |m| {
            let waiting_after = m
                .contact(&address)
                .and_then(|c| c.waiting)
                .map(|w| w.count)
                .unwrap_or(0);
            let was_waiting = was_waiting || waiting_after < waiting_before;
            m.announce_receipt(&id, &address, &path, ms, was_waiting);
        });
    }

    /// The toast for a receipt and, for a message on screen, the check
    /// travelling back before the tick fills.
    fn announce_receipt(
        self: &Rc<Self>,
        id: &str,
        address: &str,
        path: &str,
        ms: u64,
        was_waiting: bool,
    ) {
        let name = self.name_of(address);
        let text = if was_waiting {
            trf("ticker.back", &[("name", &name)])
        } else {
            trf(
                "ticker.delivered",
                &[
                    ("name", &name),
                    ("path", path),
                    ("secs", &timefmt::secs_one_decimal(ms)),
                ],
            )
        };
        let on_screen =
            self.open_friend().as_deref() == Some(address) && self.shown.borrow().contains_key(id);
        if on_screen {
            // The check fills when the receipt has travelled back.
            let me = Rc::downgrade(self);
            let id = id.to_string();
            self.chat.route.play(
                Packet::ReceiptBack,
                Some(Box::new(move || {
                    if let Some(m) = me.upgrade() {
                        m.update_row(&id, true);
                        m.update_header();
                        m.update_badges();
                        if was_waiting {
                            m.chat.push(&chat_view::back_note(&text, &m.style));
                            m.scroll_to_bottom();
                        }
                        m.toast(&text, "ok");
                    }
                })),
            );
        } else {
            self.update_badges();
            self.toast(&text, "ok");
        }
    }

    fn on_incoming(self: &Rc<Self>, message: &Message) {
        self.known
            .borrow_mut()
            .insert(message.id.clone(), message.clone());
        self.note_path(message);
        let address = message.address.clone();
        let name = self.name_of(&address);
        if self.open_friend().as_deref() == Some(address.as_str()) {
            let text = match message.path.as_deref().filter(|p| !p.is_empty()) {
                Some(p) => trf("ticker.incoming", &[("name", &name), ("path", p)]),
                None => trf("ticker.incoming.noPath", &[("name", &name)]),
            };
            let me = Rc::downgrade(self);
            let msg = message.clone();
            self.chat.route.play(
                Packet::In,
                Some(Box::new(move || {
                    let Some(m) = me.upgrade() else { return };
                    if m.open_friend().as_deref() == Some(msg.address.as_str()) {
                        let at_bottom = m.stick_bottom.get();
                        m.append_message(&msg, true);
                        m.update_header();
                        if at_bottom {
                            m.scroll_to_bottom();
                            m.toast(&text, "");
                        } else {
                            m.chat.toast.show_jump();
                        }
                    }
                })),
            );
        } else {
            // At once for the toast; the engine's `unread` event that follows
            // sets the count.
            let n = {
                let mut unread = self.unread.borrow_mut();
                let n = unread.entry(address.clone()).or_insert(0);
                *n += 1;
                *n
            };
            self.update_badges();
            self.toast(
                &trf("home.newFrom", &[("n", &n.to_string()), ("name", &name)]),
                "",
            );
        }
        self.refresh_contacts();
    }

    fn on_group_message(&self, group: &str, message: &GroupMessage) {
        let gname = self.group(group).map(|g| g.name).unwrap_or_default();
        if self.open_group().as_deref() == Some(group) {
            let at_bottom = self.stick_bottom.get();
            self.append_group(message, true);
            if at_bottom {
                self.scroll_to_bottom();
                self.toast(
                    &trf(
                        "group.incoming",
                        &[("name", &message.from_name), ("group", &gname)],
                    ),
                    "",
                );
            } else {
                self.chat.toast.show_jump();
            }
        } else if !message.mine {
            let n = self.group(group).map_or(0, |g| g.unread) + 1;
            if let Some(g) = self.groups.borrow_mut().iter_mut().find(|g| g.id == group) {
                g.unread = n;
            }
            self.update_group_rows();
            self.toast(
                &trf("group.newIn", &[("n", &n.to_string()), ("name", &gname)]),
                "",
            );
        }
        self.refresh_groups();
    }
}
