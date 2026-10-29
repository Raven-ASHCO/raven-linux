//! The Network page (Calm), in the main window's sidebar (NETWORK_PARITY.md,
//! "Linux"): "This computer" (the mark with its radar rings, which spread
//! once per successful status check, listening, the background program,
//! the heartbeat, Check network), then every way to reach friends from
//! `network.status` as a card with its coloured icon, state chip, plain
//! line, "in use" state and a switch where `network.set` allows it; router
//! port opening, relays in use, the web proxy, Bluetooth nearby (what BlueZ
//! needs), who carries mail, helping other devices and the protections. A
//! developer build with lab switches shows "Lab switches on" at the top; a
//! release build never does.
//!
//! A change goes through `netflow` on the engine thread: the consent sheet
//! first for the three settings that need a yes (`agreed: true` is sent
//! only from its Agree button), the engine's refusal for a held way, and
//! the "Restart the background program?" prompt when a change waits for
//! a restart. The page is rebuilt from the engine's answer after each
//! change or refresh; the only motion is the radar answering a status check.
//!
//! Check network (`network.check`, the router question on request) is a
//! dialog opened from here.

use gtk::prelude::*;
use raven_desktop::api::{Contact, NetworkPatch, NetworkStatus};
use raven_desktop::icons::Icon;
use raven_desktop::netflow::{self, Consent, ConsentSheet, RestartPrompt, SetOutcome};
use raven_desktop::netview::{self, Chip, ProxyChoice, Toggle, WayRow};
use raven_desktop::present::{self, NodeChip};
use raven_desktop::strings::{tr, tr_list};
use raven_desktop::theme::tokens;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::calm::{self, Clamp, MarkView};
use super::widgets::{
    self, after, clear_box, details_expander, dialog, hbox, heading, label, pref_row,
    primary_button, sp, vbox, MD, SM, XS,
};

/// Developer builds name the lab switches and show the lab banner.
const APP_DEBUG: bool = cfg!(debug_assertions);

pub struct NetworkPage {
    ctx: Weak<Ctx>,
    /// This page, for engine answers that arrive later.
    me: Weak<NetworkPage>,
    pub root: gtk::ScrolledWindow,
    banner: gtk::Label,
    pending: gtk::Box,
    pending_out: gtk::Box,
    restart_button: gtk::Button,
    error: gtk::Box,
    changes: gtk::Box,
    content: gtk::Box,
    last: RefCell<Option<(NetworkStatus, Vec<Contact>)>>,
    /// A change is on its way to the engine: the switches wait for it.
    busy: Cell<bool>,
    /// The switches on the page by setting, and the one the person just
    /// moved: the page is rebuilt from the engine's answer and keyboard
    /// focus goes back to that switch.
    switches: RefCell<Vec<(&'static str, gtk::Switch)>>,
    refocus: Cell<Option<&'static str>>,
    /// Called after each new `network.status` is on screen (Settings >
    /// Network shows the same states).
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    /// The radar around the mark and the "in use" dots: they answer a
    /// status check.
    radar: RefCell<Vec<gtk::Box>>,
    live: RefCell<Vec<gtk::Box>>,
    heart: RefCell<Option<(gtk::Box, gtk::Label)>>,
}

/// A section label ("WAYS TO REACH FRIENDS").
fn section(title: &str) -> gtk::Label {
    let l = label(&title.to_uppercase(), &["h2"]);
    l.set_wrap(false);
    l
}

/// A card for a section's lines.
fn card() -> gtk::Box {
    let c = vbox(sp(SM));
    c.add_css_class("card");
    c
}

/// The state chip of a way.
pub fn chip(c: Chip) -> gtk::Label {
    let l = gtk::Label::new(Some(c.label()));
    l.add_css_class("state-chip");
    l.add_css_class(c.css());
    l.set_valign(gtk::Align::Center);
    l.set_visible(!c.label().is_empty());
    l
}

fn small(text: &str) -> gtk::Label {
    label(text, &["small", "dim"])
}

impl NetworkPage {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Self> {
        let page = vbox(sp(SM));
        page.add_css_class("page");
        let head = hbox(sp(SM));
        let title = calm::heading_label(tr("network.title"), &["page-title"]);
        title.set_hexpand(true);
        head.append(&title);
        let refresh = widgets::button(tr("network.refresh"));
        refresh.set_valign(gtk::Align::Center);
        head.append(&refresh);
        page.append(&head);
        page.append(&label(tr("network.page.intro"), &["intro"]));

        let banner = label("", &["lab-banner"]);
        banner.set_visible(false);
        page.append(&banner);

        // Changes that wait for a restart (the person chose Later).
        let pending = vbox(sp(XS));
        pending.add_css_class("pend");
        let pending_row = hbox(sp(MD));
        pending_row.append(&calm::icon(Icon::Relay, 16));
        let pending_text = label(tr("network.restart.pending"), &[]);
        pending_text.set_hexpand(true);
        let restart_button = primary_button(tr("network.restart.now"));
        restart_button.set_valign(gtk::Align::Center);
        pending_row.append(&pending_text);
        pending_row.append(&restart_button);
        pending.append(&pending_row);
        let pending_out = vbox(sp(XS));
        pending.append(&pending_out);
        pending.set_visible(false);
        page.append(&pending);

        let error = vbox(sp(XS));
        error.set_visible(false);
        page.append(&error);
        let changes = card();
        changes.set_visible(false);
        page.append(&changes);
        let content = vbox(sp(SM));
        page.append(&content);

        let clamp = Clamp::new(tokens().layout.page_max_width, &page);
        let root = gtk::ScrolledWindow::new();
        root.set_hscrollbar_policy(gtk::PolicyType::Never);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.set_child(Some(&clamp));

        let p = Rc::new_cyclic(|me| Self {
            ctx: Rc::downgrade(ctx),
            me: me.clone(),
            root,
            banner,
            pending,
            pending_out,
            restart_button,
            error,
            changes,
            content,
            last: RefCell::new(None),
            busy: Cell::new(false),
            switches: RefCell::new(Vec::new()),
            refocus: Cell::new(None),
            listeners: RefCell::new(Vec::new()),
            radar: RefCell::new(Vec::new()),
            live: RefCell::new(Vec::new()),
            heart: RefCell::new(None),
        });
        let me = Rc::downgrade(&p);
        refresh.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                p.show_error(None);
                p.show_changes(&[]);
                p.refresh();
            }
        });
        let me = Rc::downgrade(&p);
        p.restart_button.connect_clicked(move |b| {
            if let Some(p) = me.upgrade() {
                p.restart(p.pending_out.clone(), vec![b.clone()], None);
            }
        });
        p
    }

    /// Read `network.status` (and the friends, for relay names) again.
    /// Errors stay on screen until the next change or Refresh.
    pub fn refresh(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.run(
            |e| (e.network_status(), e.contacts_list()),
            move |ctx, (status, contacts)| {
                let Some(p) = me.upgrade() else { return };
                if p.busy.get() {
                    return; // the change's own answer rebuilds the page
                }
                match status {
                    Ok(s) => {
                        ctx.network_changed(&s);
                        p.render(s, contacts.unwrap_or_default());
                    }
                    Err(e) => p.show_error(Some((&e.message, &e.detail))),
                }
            },
        );
    }

    fn show_error(&self, e: Option<(&str, &str)>) {
        clear_box(&self.error);
        match e {
            Some((message, detail)) => {
                self.error.append(&label(message, &["danger"]));
                self.error.append(&details_expander(detail));
                self.error.set_visible(true);
            }
            None => self.error.set_visible(false),
        }
    }

    /// "What changed": each change's own words.
    fn show_changes(&self, effects: &[String]) {
        clear_box(&self.changes);
        if effects.is_empty() {
            self.changes.set_visible(false);
            return;
        }
        self.changes
            .append(&label(tr("network.changed"), &["strong"]));
        for e in effects {
            self.changes.append(&label(e, &[]));
        }
        self.changes.set_visible(true);
    }

    /// "This computer": the mark with its radar, what it listens on, the
    /// background program, the heartbeat, Check network.
    fn this_computer(&self, ctx: &Rc<Ctx>, status: &NetworkStatus) -> gtk::Box {
        let c = card();
        c.add_css_class("this-card");
        let top = hbox(18);
        let mark = MarkView::new(30);
        let radar = gtk::Overlay::new();
        let frame = hbox(0);
        frame.set_size_request(64, 64);
        radar.set_child(Some(&frame));
        let mut rings = Vec::new();
        for late in [false, true] {
            let ring = hbox(0);
            ring.add_css_class("radar-ring");
            if late {
                ring.add_css_class("late");
            }
            ring.set_can_target(false);
            radar.add_overlay(&ring);
            rings.push(ring);
        }
        radar.add_overlay(&mark.area);
        radar.set_valign(gtk::Align::Center);
        self.radar.replace(rings);
        top.append(&radar);
        let texts = vbox(2);
        texts.set_hexpand(true);
        texts.set_valign(gtk::Align::Center);
        texts.append(&calm::heading_label(tr("net.thisComputer"), &["t-label"]));
        let node = ctx.node.borrow().clone();
        texts.append(&label(&present::listening_line(&node), &["small", "dim"]));
        texts.append(&label(&present::process_line(&node), &["small", "dim"]));
        let hb = hbox(6);
        hb.set_margin_top(6);
        let heart = hbox(0);
        heart.add_css_class("heart");
        heart.set_valign(gtk::Align::Center);
        let hb_text = label("", &["small", "success"]);
        hb.append(&heart);
        hb.append(&hb_text);
        texts.append(&hb);
        self.heart.replace(Some((heart, hb_text)));
        self.update_heart();
        top.append(&texts);
        let check = widgets::button(tr("network.check.open"));
        check.set_valign(gtk::Align::Center);
        let cx = Rc::downgrade(ctx);
        check.connect_clicked(move |_| {
            if let Some(ctx) = cx.upgrade() {
                show_check(&ctx);
            }
        });
        top.append(&check);
        c.append(&top);
        for line in netview::this_computer(status, raven_desktop::timefmt::now_ms()) {
            c.append(&small(&line));
        }
        let details = netview::this_computer_details(status);
        if !details.is_empty() {
            c.append(&details_expander(&details.join("\n")));
        }
        c.append(&self.lan_for_row());
        if !node.running {
            let start = widgets::button(tr("net.start"));
            start.set_halign(gtk::Align::Start);
            let me = self.me.clone();
            start.connect_clicked(move |b| {
                if let Some(p) = me.upgrade() {
                    b.set_sensitive(false);
                    p.start_node();
                }
            });
            c.append(&start);
        }
        c
    }

    /// "your node: ok · now" (or that the background program isn't
    /// responding), from the last status check.
    fn update_heart(&self) {
        let (Some(ctx), Some((heart, text))) = (self.ctx.upgrade(), self.heart.borrow().clone())
        else {
            return;
        };
        let node = ctx.node.borrow().clone();
        let chip = present::node_chip(
            &node,
            ctx.last_status_ok(),
            raven_desktop::timefmt::now_ms(),
            tokens().durations.status_check_interval as u64,
        );
        let (t, ok) = match chip {
            NodeChip::Ok(t) => (t, true),
            NodeChip::Starting(t) | NodeChip::Down(t) => (t, false),
        };
        text.set_text(&t);
        for c in ["success", "warning"] {
            text.remove_css_class(c);
        }
        text.add_css_class(if ok { "success" } else { "warning" });
        if ok {
            heart.remove_css_class("down");
        } else {
            heart.add_css_class("down");
        }
    }

    /// A status check answered while the page is on screen: the radar
    /// spreads once around the mark and the "in use" dots beat.
    pub fn pulse(&self) {
        self.update_heart();
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        if !self.root.is_mapped() {
            return;
        }
        let m = &tokens().calm;
        for ring in self.radar.borrow().iter() {
            calm::play_class(ring, &ctx.style, "go", m.radar_pulse + 350);
        }
        if let Some((heart, _)) = self.heart.borrow().as_ref() {
            calm::play_class(
                heart,
                &ctx.style,
                "beat",
                tokens().durations.heartbeat_pulse,
            );
        }
        for (i, dot) in self.live.borrow().iter().enumerate() {
            let (d, st) = (dot.downgrade(), ctx.style.clone());
            after(200 + 140 * i as u32, move || {
                if let Some(d) = d.upgrade() {
                    calm::play_class(&d, &st, "go", 600);
                }
            });
        }
    }

    /// Build the page from one `network.status`.
    fn render(&self, status: NetworkStatus, contacts: Vec<Contact>) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        self.busy.set(false);
        self.content.set_sensitive(true);
        match netview::lab_banner(&status, APP_DEBUG) {
            Some(text) => {
                self.banner.set_text(&text);
                self.banner.set_visible(true);
            }
            None => self.banner.set_visible(false),
        }
        self.pending.set_visible(ctx.restart_pending.get());
        let c = &self.content;
        clear_box(c);
        self.switches.borrow_mut().clear();
        self.live.borrow_mut().clear();

        c.append(&self.this_computer(&ctx, &status));

        // Ways to reach friends: two columns of cards.
        c.append(&section(tr("net.ways")));
        let ways = gtk::FlowBox::new();
        ways.set_selection_mode(gtk::SelectionMode::None);
        ways.set_max_children_per_line(2);
        ways.set_min_children_per_line(1);
        ways.set_homogeneous(true);
        ways.set_column_spacing(12);
        ways.set_row_spacing(12);
        for row in netview::way_rows(&status, APP_DEBUG) {
            let child = gtk::FlowBoxChild::new();
            child.set_focusable(false);
            child.set_child(Some(&self.way_card(&row)));
            ways.insert(&child, -1);
        }
        c.append(&ways);

        c.append(&section(tr("network.portMapping.title")));
        let pm = card();
        for line in netview::port_mapping_lines(&status) {
            pm.append(&small(&line));
        }
        c.append(&pm);

        c.append(&section(tr("network.relays.title")));
        let relays = card();
        for line in netview::relay_lines(&status, &contacts) {
            let l = small(&line);
            l.set_selectable(true);
            relays.append(&l);
        }
        c.append(&relays);

        c.append(&section(tr("network.proxy.title")));
        let proxy = card();
        proxy.append(&self.proxy_section(&status));
        c.append(&proxy);

        c.append(&section(tr("network.nearby.title")));
        let nb = card();
        let nearby = netview::nearby_view(&status);
        for line in &nearby.lines {
            nb.append(&small(line));
        }
        if let Some(w) = nearby.warning {
            nb.append(&label(w, &["warning"]));
        }
        if let Some(note) = nearby.linux_note {
            let l = label(note, &["small"]);
            l.set_selectable(true);
            nb.append(&l);
        }
        c.append(&nb);

        c.append(&section(tr("network.carry.title")));
        let carry_card = card();
        let mail = netview::mail_view(&status);
        for line in &mail.lines {
            carry_card.append(&small(line));
        }
        let carry = tr("network.carry.forFriends");
        carry_card.append(&pref_row(carry, &self.switch(&mail.carry_for_friends)));
        let hide = tr("network.hideIp");
        carry_card.append(&pref_row(hide, &self.switch(&mail.hide_ip)));
        carry_card.append(&small(tr("network.hideIp.body")));
        c.append(&carry_card);

        c.append(&section(tr("network.help.title")));
        let help_card = card();
        let (help, how) = netview::help_rows(&status);
        help_card.append(&small(how));
        for r in help {
            let b = vbox(2);
            b.append(&pref_row(r.label, &self.switch(&r.toggle)));
            b.append(&small(&r.effect));
            help_card.append(&b);
        }
        c.append(&help_card);

        let prot = netview::protection_rows(&status, APP_DEBUG);
        if !prot.is_empty() {
            c.append(&section(tr("network.protections")));
            let pc = card();
            let n = prot.len();
            for (i, row) in prot.iter().enumerate() {
                let r = self.prot_row(row);
                if i + 1 == n {
                    r.add_css_class("last");
                }
                pc.append(&r);
            }
            c.append(&pc);
        }

        self.last.replace(Some((status, contacts)));
        for f in self.listeners.borrow().iter() {
            f();
        }
        if let Some(setting) = self.refocus.take() {
            let switches = self.switches.borrow();
            if let Some((_, s)) = switches.iter().find(|(k, _)| *k == setting) {
                s.grab_focus();
            }
        }
    }

    /// Build the page again from the last answer (a switch the engine did
    /// not change goes back to where it was).
    fn render_last(&self) {
        let last = self.last.borrow().clone();
        if let Some((s, c)) = last {
            self.render(s, c);
        }
    }

    /// One way as a card: its coloured icon, name and state chip, the
    /// switch, what it does and why it is in this state, whether it is in
    /// use now, and Details (open when technical details are on).
    fn way_card(&self, row: &WayRow) -> gtk::Box {
        let style = netview::way_style(&row.id);
        let card = hbox(12);
        card.add_css_class("card");
        card.add_css_class("way");
        if let Some(class) = style.class {
            card.add_css_class(&format!("p-{class}"));
        }
        if row.chip == Chip::Off {
            card.add_css_class("off");
        }
        let ic = hbox(0);
        ic.add_css_class("way-ic");
        ic.set_valign(gtk::Align::Start);
        let i = calm::icon(style.icon, 18);
        i.set_hexpand(true);
        ic.append(&i);
        card.append(&ic);
        let texts = vbox(4);
        texts.set_hexpand(true);
        let title = hbox(6);
        let name = label(&row.name, &["way-title"]);
        title.append(&name);
        title.append(&chip(row.chip));
        // Tor: its state as raven-node reports it, in a chip of its own.
        if let Some(t) = &row.tor {
            let state = if t.trouble {
                "state-preview"
            } else if t.ready {
                "state-on"
            } else {
                "state-off"
            };
            let p = label(&t.phase, &["state-chip", state]);
            p.set_valign(gtk::Align::Center);
            title.append(&p);
        }
        texts.append(&title);
        texts.append(&label(&row.line, &["way-desc"]));
        if let Some(t) = &row.tor {
            for line in &t.lines {
                let l = small(line);
                l.set_selectable(true);
                texts.append(&l);
            }
        }
        if let Some(r) = row.running {
            let state = hbox(6);
            state.add_css_class("way-state");
            if r == tr("network.running.yes") {
                let dot = hbox(0);
                dot.add_css_class("live");
                dot.set_valign(gtk::Align::Center);
                state.append(&dot);
                self.live.borrow_mut().push(dot);
            }
            state.append(&label(r, &[]));
            texts.append(&state);
        }
        if !row.detail.is_empty() {
            let e = details_expander(&row.detail);
            if let Some(ctx) = self.ctx.upgrade() {
                e.set_expanded(ctx.style.details());
            }
            texts.append(&e);
        }
        card.append(&texts);
        if let Some(t) = &row.toggle {
            let s = self.switch(t);
            s.set_valign(gtk::Align::Start);
            // The way's name is the switch's accessible name.
            name.set_mnemonic_widget(Some(&s));
            card.append(&s);
        }
        card
    }

    /// A protection: name, chip and what it does.
    fn prot_row(&self, row: &WayRow) -> gtk::Box {
        let outer = hbox(sp(MD));
        outer.add_css_class("prot-row");
        let texts = vbox(2);
        texts.set_hexpand(true);
        let title = hbox(sp(SM));
        title.append(&label(&row.name, &["t-label"]));
        title.append(&chip(row.chip));
        texts.append(&title);
        texts.append(&small(&row.line));
        if !row.detail.is_empty() {
            let e = details_expander(&row.detail);
            if let Some(ctx) = self.ctx.upgrade() {
                e.set_expanded(ctx.style.details());
            }
            texts.append(&e);
        }
        outer.append(&texts);
        outer
    }

    /// A switch for one `network.set` setting. It sends the change and
    /// waits: the page is rebuilt from the engine's answer.
    fn switch(&self, t: &Toggle) -> gtk::Switch {
        let s = gtk::Switch::new();
        s.set_active(t.active);
        s.set_sensitive(t.sensitive);
        s.set_valign(gtk::Align::Center);
        let me = self.me.clone();
        let setting = t.setting;
        s.connect_active_notify(move |s| {
            if let Some(p) = me.upgrade() {
                p.refocus.set(Some(setting));
                p.apply(netflow::patch_for(setting, s.is_active()));
            }
        });
        self.switches.borrow_mut().push((setting, s.clone()));
        s
    }

    /// "Open on this network for a while": LAN direct on for 30 minutes to
    /// 24 hours, then off by itself (`network.set {lan, lan_for}`).
    fn lan_for_row(&self) -> gtk::Box {
        let row = hbox(sp(SM));
        let text = label(tr("network.lanFor.title"), &[]);
        text.set_hexpand(true);
        let drop = gtk::DropDown::from_strings(&tr_list("network.lanFor.options"));
        drop.set_valign(gtk::Align::Center);
        text.set_mnemonic_widget(Some(&drop));
        let open = widgets::button(tr("network.lanFor.open"));
        open.set_valign(gtk::Align::Center);
        row.append(&text);
        row.append(&drop);
        row.append(&open);
        let (d, me) = (drop.downgrade(), self.me.clone());
        open.connect_clicked(move |_| {
            let (Some(d), Some(p)) = (d.upgrade(), me.upgrade()) else {
                return;
            };
            if let Some(patch) = netflow::lan_for_patch(d.selected()) {
                p.apply(patch);
            }
        });
        row
    }

    fn proxy_section(&self, status: &NetworkStatus) -> gtk::Box {
        let b = vbox(sp(XS));
        let choice = ProxyChoice::of(&status.proxy.setting);
        let drop = gtk::DropDown::from_strings(&netview::proxy_options());
        drop.set_selected(choice.index());
        drop.set_tooltip_text(Some(tr("network.proxy.title")));
        let entry = gtk::Entry::new();
        entry.add_css_class("raven-field");
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(tr("network.proxy.placeholder")));
        if let ProxyChoice::Custom(t) = &choice {
            entry.set_text(t);
        }
        entry.set_sensitive(choice.index() == 2);
        let apply = widgets::button(tr("network.proxy.apply"));
        let row = hbox(sp(SM));
        row.append(&drop);
        row.append(&entry);
        row.append(&apply);
        b.append(&row);
        for line in netview::proxy_lines(status) {
            b.append(&small(&line));
        }
        b.append(&small(tr("network.proxy.noCredentials")));
        let err = label("", &["danger", "small"]);
        err.set_visible(false);
        b.append(&err);

        let e = entry.downgrade();
        drop.connect_selected_notify(move |d| {
            if let Some(e) = e.upgrade() {
                e.set_sensitive(d.selected() == 2);
            }
        });
        let (d, e, err, me) = (
            drop.downgrade(),
            entry.downgrade(),
            err.downgrade(),
            self.me.clone(),
        );
        let send = move || {
            let (Some(d), Some(e), Some(err), Some(p)) =
                (d.upgrade(), e.upgrade(), err.upgrade(), me.upgrade())
            else {
                return;
            };
            match netview::proxy_value(d.selected(), &e.text()) {
                Ok(v) => {
                    err.set_visible(false);
                    p.apply(netflow::proxy_patch(&v));
                }
                Err(text) => {
                    err.set_text(&text);
                    err.set_visible(true);
                }
            }
        };
        let send2 = send.clone();
        apply.connect_clicked(move |_| send());
        entry.connect_activate(move |_| send2());
        b
    }

    /// Run `f` after each new `network.status` is on screen.
    pub fn on_status(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// A change is on its way to the engine (a new one would be dropped).
    pub fn is_busy(&self) -> bool {
        self.busy.get()
    }

    /// The last `network.status` shown.
    pub fn last_status(&self) -> Option<NetworkStatus> {
        self.last.borrow().as_ref().map(|(s, _)| s.clone())
    }

    /// A change from Settings > Network: the same flow as a switch here
    /// (consent sheet, refusal of a held way, restart prompt).
    pub fn apply_patch(&self, patch: NetworkPatch) {
        self.apply(patch);
    }

    /// Restart now, from the reminder in Settings > Network.
    pub fn restart_into(&self, out: &gtk::Box, button: &gtk::Button) {
        self.restart(out.clone(), vec![button.clone()], None);
    }

    /// Send one change (on the engine thread) and show what came of it.
    fn apply(&self, patch: NetworkPatch) {
        self.run(move |e| netflow::set(e, patch));
    }

    /// The person agreed on the consent sheet.
    fn agree(&self, consent: Consent) {
        self.run(move |e| netflow::agree(e, consent));
    }

    fn run(
        &self,
        job: impl FnOnce(&dyn raven_desktop::api::RavenEngine) -> SetOutcome + Send + 'static,
    ) {
        if self.busy.replace(true) {
            return;
        }
        self.content.set_sensitive(false);
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.run(job, move |ctx, outcome| {
            if let Some(p) = me.upgrade() {
                p.outcome(ctx, outcome);
            }
        });
    }

    fn outcome(&self, ctx: &Rc<Ctx>, outcome: SetOutcome) {
        self.busy.set(false);
        self.content.set_sensitive(true);
        match outcome {
            SetOutcome::Applied {
                changes,
                status,
                restart,
            } => {
                self.show_error(None);
                let effects: Vec<String> = changes.iter().map(|c| c.effect.clone()).collect();
                self.show_changes(&effects);
                ctx.network_changed(&status);
                let contacts = self
                    .last
                    .borrow()
                    .as_ref()
                    .map(|(_, c)| c.clone())
                    .unwrap_or_default();
                self.render(*status, contacts);
                if let Some(prompt) = restart {
                    self.ask_restart(ctx, prompt);
                }
            }
            SetOutcome::NeedsConsent(sheet) => {
                self.render_last();
                self.ask_consent(ctx, *sheet);
            }
            SetOutcome::Refused(e) | SetOutcome::Failed(e) => {
                self.show_error(Some((&e.message, &e.detail)));
                self.render_last();
                self.refresh();
            }
        }
    }

    /// The consent sheet: what it does, what it costs, what others can and
    /// can't see, and the engine's own words. Agree sends `agreed: true`;
    /// Cancel (the default) sends nothing.
    fn ask_consent(&self, ctx: &Rc<Ctx>, sheet: ConsentSheet) {
        let parent = ctx.main_window();
        let (window, content) = dialog(parent.as_ref(), &sheet.title, &ctx.style, 520);
        content.append(&heading(&sheet.title));
        let body = vbox(sp(SM));
        for (title, text) in [
            (tr("network.consent.does"), &sheet.does),
            (tr("network.consent.costs"), &sheet.costs),
            (tr("network.consent.sees"), &sheet.sees),
            (tr("network.consent.hidden"), &sheet.hidden),
        ] {
            let t = label(title, &["strong"]);
            t.set_margin_top(sp(SM));
            body.append(&t);
            body.append(&label(text, &[]));
        }
        if !sheet.engine_text.is_empty() {
            let t = label(tr("network.consent.engine"), &["strong"]);
            t.set_margin_top(sp(SM));
            body.append(&t);
            let words = label(&sheet.engine_text, &["dim"]);
            words.set_selectable(true);
            let card = vbox(0);
            card.add_css_class("card");
            card.append(&words);
            body.append(&card);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(460);
        scroll.set_child(Some(&body));
        content.append(&scroll);
        let row = widgets::action_row();
        let cancel = widgets::button(tr("network.consent.notNow"));
        let agree = primary_button(tr("network.consent.agree"));
        row.append(&cancel);
        row.append(&agree);
        content.append(&row);
        let w = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        let (w, me, consent) = (window.downgrade(), self.me.clone(), sheet.consent);
        agree.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
            if let Some(p) = me.upgrade() {
                p.agree(consent);
            }
        });
        window.present();
        cancel.grab_focus();
    }

    /// "Restart the background program?": what waits for it, honest about
    /// a program this app did not start; Later keeps a reminder on the page.
    fn ask_restart(&self, ctx: &Rc<Ctx>, prompt: RestartPrompt) {
        ctx.restart_pending.set(true);
        self.pending.set_visible(true);
        let parent = ctx.main_window();
        let (window, content) = dialog(parent.as_ref(), &prompt.title, &ctx.style, 480);
        content.append(&heading(&prompt.title));
        content.append(&label(&prompt.body, &[]));
        for c in &prompt.changes {
            content.append(&small(&format!("\u{2022} {c}")));
        }
        if let Some(n) = &prompt.not_ours {
            content.append(&label(n, &["warning"]));
        }
        let out = vbox(sp(XS));
        content.append(&out);
        let row = widgets::action_row();
        let later = widgets::button(tr("network.restart.later"));
        let now = primary_button(tr("network.restart.now"));
        row.append(&later);
        row.append(&now);
        content.append(&row);
        let w = window.downgrade();
        later.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        let (me, out2, later2) = (self.me.clone(), out.clone(), later.clone());
        now.connect_clicked(move |b| {
            if let Some(p) = me.upgrade() {
                p.restart(out2.clone(), vec![b.clone()], Some(later2.clone()));
            }
        });
        window.present();
        now.grab_focus();
    }

    /// `node.stop` then `node.start` on the engine thread; the result goes
    /// into `out`. `buttons` wait meanwhile; `close` (the dialog's Later)
    /// becomes Close afterwards.
    fn restart(&self, out: gtk::Box, buttons: Vec<gtk::Button>, close: Option<gtk::Button>) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        for b in &buttons {
            b.set_sensitive(false);
        }
        clear_box(&out);
        let working = hbox(sp(SM));
        if ctx.style.motion_allowed() {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            working.append(&spinner);
        }
        working.append(&label(tr("network.restart.restarting"), &["dim"]));
        out.append(&working);
        let me = self.me.clone();
        ctx.run(netflow::restart, move |ctx, r| {
            clear_box(&out);
            out.append(&label(
                &r.text,
                &[if r.done { "success" } else { "warning" }],
            ));
            if let Some(hint) = r.hint {
                let h = label(hint, &["mono"]);
                h.set_selectable(true);
                out.append(&h);
            }
            out.append(&details_expander(&r.detail));
            if let Some(n) = r.node {
                ctx.node.replace(n);
            }
            if r.done {
                ctx.restart_pending.set(false);
            }
            for b in &buttons {
                b.set_sensitive(!r.done);
            }
            if let Some(c) = &close {
                c.set_label(tr("common.close"));
            }
            if let Some(p) = me.upgrade() {
                if r.done {
                    p.pending.set_visible(false);
                }
                p.refresh();
            }
        });
    }

    fn start_node(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.call(
            |e| e.node_start(),
            move |ctx, result| {
                let Some(p) = me.upgrade() else { return };
                match result {
                    Ok(n) => {
                        ctx.node.replace(n);
                    }
                    Err(e) => p.show_error(Some((&e.message, &e.detail))),
                }
                p.refresh();
            },
        );
    }
}

// ---------------------------------------------------------------- check

/// Check network: `raven doctor`'s summary and network lines, and the
/// router question (read-only, up to about 4 s) only when asked for.
struct Check {
    ctx: Weak<Ctx>,
    me: Weak<Check>,
    body: gtk::Box,
    router_out: gtk::Box,
    again: gtk::Button,
    router: gtk::Button,
}

pub fn show_check(ctx: &Rc<Ctx>) {
    let parent = ctx.main_window();
    let (window, content) = dialog(parent.as_ref(), tr("network.check.title"), &ctx.style, 560);
    content.append(&heading(tr("network.check.title")));
    content.append(&label(tr("network.check.intro"), &["dim"]));
    let body = vbox(sp(SM));
    let router_box = vbox(sp(XS));
    router_box.set_margin_top(sp(MD));
    router_box.append(&label(tr("network.check.router.title"), &["strong"]));
    router_box.append(&small(tr("network.check.router.body")));
    let router_out = vbox(sp(XS));
    router_box.append(&router_out);
    body.append(&router_box);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(240);
    scroll.set_max_content_height(520);
    let column = vbox(sp(SM));
    let results = vbox(sp(SM));
    column.append(&results);
    column.append(&body);
    scroll.set_child(Some(&column));
    content.append(&scroll);
    let row = widgets::action_row();
    let again = widgets::button(tr("network.check.again"));
    let router = widgets::button(tr("network.check.router"));
    let close = primary_button(tr("common.close"));
    row.append(&again);
    row.append(&router);
    row.append(&close);
    content.append(&row);
    let check = Rc::new_cyclic(|me| Check {
        ctx: Rc::downgrade(ctx),
        me: me.clone(),
        body: results,
        router_out,
        again: again.clone(),
        router: router.clone(),
    });
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    let me = Rc::downgrade(&check);
    again.connect_clicked(move |_| {
        if let Some(c) = me.upgrade() {
            c.run(false);
        }
    });
    let me = Rc::downgrade(&check);
    router.connect_clicked(move |_| {
        if let Some(c) = me.upgrade() {
            c.run(true);
        }
    });
    widgets::keep_alive(&window, check.clone());
    window.present();
    close.grab_focus();
    check.run(false);
}

impl Check {
    fn run(&self, router: bool) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        self.again.set_sensitive(false);
        self.router.set_sensitive(false);
        let out = if router { &self.router_out } else { &self.body };
        clear_box(out);
        let working = hbox(sp(SM));
        if ctx.style.motion_allowed() {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            working.append(&spinner);
        }
        working.append(&label(
            if router {
                tr("network.check.router.asking")
            } else {
                tr("network.check.running")
            },
            &["dim"],
        ));
        out.append(&working);
        let me = self.me.clone();
        ctx.call(
            move |e| e.network_check(router),
            move |_, result| {
                let Some(c) = me.upgrade() else { return };
                c.again.set_sensitive(true);
                c.router.set_sensitive(true);
                match result {
                    Ok(check) => c.show(&netview::check_view(&check), router),
                    Err(e) => {
                        let out = if router { &c.router_out } else { &c.body };
                        clear_box(out);
                        out.append(&label(&e.message, &["danger"]));
                        out.append(&details_expander(&e.detail));
                    }
                }
            },
        );
    }

    fn show(&self, v: &netview::CheckView, router: bool) {
        if router {
            clear_box(&self.router_out);
            for line in v.router.iter().flatten() {
                let l = label(line, &["small"]);
                l.set_selectable(true);
                self.router_out.append(&l);
            }
            return;
        }
        let b = &self.body;
        clear_box(b);
        b.append(&label(tr("network.check.summary"), &["strong"]));
        let grid = gtk::Grid::new();
        grid.set_column_spacing(sp(MD) as u32);
        grid.set_row_spacing(sp(XS) as u32);
        for (i, (k, v)) in v.summary.iter().enumerate() {
            grid.attach(&label(k, &["dim"]), 0, i as i32, 1, 1);
            let value = label(v, &[]);
            value.set_selectable(true);
            value.set_hexpand(true);
            grid.attach(&value, 1, i as i32, 1, 1);
        }
        b.append(&grid);
        if let Some(step) = &v.next_step {
            b.append(&label(step, &["strong"]));
        }
        let t = label(tr("network.check.lines"), &["strong"]);
        t.set_margin_top(sp(SM));
        b.append(&t);
        for line in &v.rows {
            let l = label(line, &["small"]);
            l.set_selectable(true);
            b.append(&l);
        }
    }
}
