//! A friend's routes panel (`contacts.routes` / `contacts.set_routes`),
//! opened from the chat toolbar: the saved routes with their state chips
//! (each LAN, Internet or p2p route can be removed), what the friend told
//! this computer (route hints), who carries the friend's mail, hide this
//! computer's IP from the friend, and "Change routes" (an Internet route,
//! a p2p route with its relays, up to two verified mail friends). The
//! engine checks every route before it saves anything; its answer
//! rebuilds the panel. Never the friend's security words or old code.

use gtk::prelude::*;
use raven_desktop::api::{Contact, ContactRoutes, RoutesPatch};
use raven_desktop::routesview::{self, RoutesForm};
use raven_desktop::strings::{tr, trf};
use raven_desktop::timefmt;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::network_page::chip;
use super::widgets::{
    self, clear_box, details_expander, dialog, hbox, heading, label, pref_row, primary_button, sp,
    vbox, LG, MD, SM, XS,
};

struct Routes {
    ctx: Weak<Ctx>,
    me: Weak<Routes>,
    friend: Contact,
    contacts: Vec<Contact>,
    title: gtk::Label,
    body: gtk::Box,
    error: gtk::Box,
    current: RefCell<Option<ContactRoutes>>,
    /// Called after a save (the friends list is read again).
    on_change: Box<dyn Fn()>,
}

pub fn show(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    friend: &Contact,
    contacts: Vec<Contact>,
    on_change: Box<dyn Fn()>,
) {
    let title_text = trf("network.routes.title", &[("name", &friend.name)]);
    let (window, content) = dialog(Some(parent), &title_text, &ctx.style, 560);
    let title = heading(&title_text);
    content.append(&title);
    let error = vbox(sp(XS));
    error.set_visible(false);
    content.append(&error);
    let body = vbox(sp(SM));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(200);
    scroll.set_max_content_height(560);
    scroll.set_child(Some(&body));
    content.append(&scroll);
    let row = widgets::action_row();
    let close = primary_button(tr("common.close"));
    row.append(&close);
    content.append(&row);
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    let routes = Rc::new_cyclic(|me| Routes {
        ctx: Rc::downgrade(ctx),
        me: me.clone(),
        friend: friend.clone(),
        contacts,
        title,
        body,
        error,
        current: RefCell::new(None),
        on_change,
    });
    widgets::keep_alive(&window, routes.clone());
    window.present();
    close.grab_focus();
    routes.load();
}

impl Routes {
    fn load(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let address = self.friend.address.clone();
        let me = self.me.clone();
        ctx.call(
            move |e| e.contacts_routes(&address),
            move |_, result| {
                let Some(r) = me.upgrade() else { return };
                match result {
                    Ok(routes) => r.render(routes),
                    Err(e) => r.show_error(Some((&e.message, &e.detail))),
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

    /// Save one change; the engine's answer rebuilds the panel.
    fn save(&self, patch: RoutesPatch, done_text: bool) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        self.body.set_sensitive(false);
        let me = self.me.clone();
        ctx.call(
            move |e| e.contacts_set_routes(&patch),
            move |_, result| {
                let Some(r) = me.upgrade() else { return };
                r.body.set_sensitive(true);
                match result {
                    Ok(saved) => {
                        r.show_error(None);
                        r.render(saved.routes);
                        if done_text {
                            r.body
                                .prepend(&label(tr("network.routes.done"), &["success"]));
                        }
                        (r.on_change)();
                    }
                    Err(e) => {
                        r.show_error(Some((&e.message, &e.detail)));
                        r.load();
                    }
                }
            },
        );
    }

    /// Saves the address Raven reaches the friend at now as its LAN address
    /// (`contacts.set_address`), then reads the routes again.
    fn use_lan_now(&self, addr: String) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        self.body.set_sensitive(false);
        let address = self.friend.address.clone();
        let me = self.me.clone();
        ctx.call(
            move |e| e.contacts_set_address(&address, &addr),
            move |_, result| {
                let Some(r) = me.upgrade() else { return };
                r.body.set_sensitive(true);
                match result {
                    Ok(_) => {
                        r.show_error(None);
                        (r.on_change)();
                    }
                    Err(e) => r.show_error(Some((&e.message, &e.detail))),
                }
                r.load();
            },
        );
    }

    fn render(&self, routes: ContactRoutes) {
        let name = self.friend.name.clone();
        let v = routesview::routes_view(&routes, &name, &self.contacts, timefmt::now_ms());
        self.title.set_text(&v.title);
        let b = &self.body;
        clear_box(b);
        if let Some(n) = &v.not_verified {
            let l = label(n, &[]);
            l.add_css_class("banner");
            b.append(&l);
        }

        b.append(&section(tr("network.routes.saved")));
        if let Some(e) = &v.empty {
            b.append(&label(e, &["dim"]));
        }
        for r in &v.routes {
            let row = hbox(sp(MD));
            row.add_css_class("way-row");
            let texts = vbox(2);
            texts.set_hexpand(true);
            let top = hbox(sp(SM));
            top.append(&label(&r.label, &["strong"]));
            top.append(&chip(r.chip));
            texts.append(&top);
            if !r.value.is_empty() {
                let value = label(&r.value, &["mono"]);
                value.set_selectable(true);
                value.set_wrap_mode(gtk::pango::WrapMode::Char);
                texts.append(&value);
            }
            for x in &r.extra {
                texts.append(&label(x, &["small", "dim"]));
            }
            row.append(&texts);
            if r.removable {
                let remove = widgets::button(tr("network.routes.remove"));
                remove.set_valign(gtk::Align::Center);
                remove.set_tooltip_text(Some(&format!(
                    "{} {}",
                    tr("network.routes.remove"),
                    r.label
                )));
                let (me, kind) = (self.me.clone(), r.kind.clone());
                remove.connect_clicked(move |_| {
                    if let Some(s) = me.upgrade() {
                        s.save(routesview::remove_patch(&s.friend.address, &kind), false);
                    }
                });
                row.append(&remove);
            }
            b.append(&row);
        }
        // The address in use, that the saved one is old, and "Use the new
        // address" (E1).
        for line in [&v.lan_now, &v.lan_stale].into_iter().flatten() {
            let l = label(line, &["small", "dim"]);
            l.set_selectable(true);
            b.append(&l);
        }
        if let Some(addr) = v.lan_save.clone() {
            let save = widgets::button(tr("network.routes.lanReplace"));
            save.set_halign(gtk::Align::Start);
            let me = self.me.clone();
            save.connect_clicked(move |_| {
                if let Some(r) = me.upgrade() {
                    r.use_lan_now(addr.clone());
                }
            });
            b.append(&save);
        }

        let hints_title = hbox(sp(SM));
        let ht = section(&v.hints_title);
        hints_title.append(&ht);
        let hc = super::network_page::chip(v.hints_chip);
        hc.set_margin_top(sp(LG));
        hints_title.append(&hc);
        b.append(&hints_title);
        for h in &v.hints {
            let l = label(h, &["small", "dim"]);
            l.set_selectable(true);
            b.append(&l);
        }

        b.append(&section(&v.mail_title));
        for m in &v.mail {
            b.append(&label(m, &["small"]));
        }

        let hide = gtk::Switch::new();
        hide.set_active(v.hide_ip);
        let me = self.me.clone();
        hide.connect_active_notify(move |s| {
            if let Some(r) = me.upgrade() {
                r.save(
                    routesview::hide_ip_patch(&r.friend.address, s.is_active()),
                    false,
                );
            }
        });
        let hide_row = pref_row(&v.hide_ip_label, &hide);
        hide_row.set_margin_top(sp(MD));
        b.append(&hide_row);
        b.append(&label(tr("network.hideIp.help"), &["small", "dim"]));

        b.append(&self.form(&routes));
        self.current.replace(Some(routes));
    }

    /// "Change routes": an Internet route, a p2p route and its relays, and
    /// up to two verified mail friends.
    fn form(&self, routes: &ContactRoutes) -> gtk::Expander {
        let ex = gtk::Expander::new(Some(tr("network.routes.edit")));
        ex.set_margin_top(sp(LG));
        let f = vbox(sp(XS));
        let entry = |placeholder: &str| {
            let e = gtk::Entry::new();
            e.add_css_class("raven-field");
            e.set_placeholder_text(Some(placeholder));
            e
        };
        // Each label names its field (GTK 4 uses the mnemonic widget).
        let named = |text: &str, field: &gtk::Widget| {
            let l = label(text, &[]);
            l.set_mnemonic_widget(Some(field));
            l
        };
        let internet = entry(tr("network.routes.internet"));
        f.append(&named(tr("network.routes.internet"), internet.upcast_ref()));
        f.append(&internet);
        let p2p = entry(tr("network.routes.p2p"));
        f.append(&named(tr("network.routes.p2p"), p2p.upcast_ref()));
        f.append(&p2p);
        let relays = gtk::TextView::new();
        relays.add_css_class("raven-field");
        relays.add_css_class("mono");
        relays.set_wrap_mode(gtk::WrapMode::Char);
        relays.set_size_request(-1, 56);
        f.append(&named(tr("network.routes.relays"), relays.upcast_ref()));
        f.append(&relays);

        let name = &self.friend.name;
        let mail_title = label(&trf("network.routes.mailFriends", &[("name", name)]), &[]);
        mail_title.set_margin_top(sp(SM));
        f.append(&mail_title);
        let listed = routesview::listed_now(routes);
        let mut checks: Vec<(String, gtk::CheckButton)> = Vec::new();
        for (address, friend) in
            routesview::mail_friend_choices(&self.contacts, &self.friend.address)
        {
            let c = gtk::CheckButton::with_label(&friend);
            c.set_active(listed.contains(&address));
            f.append(&c);
            checks.push((address, c));
        }
        let err = label("", &["danger", "small"]);
        err.set_visible(false);
        f.append(&err);
        let save = primary_button(tr("network.routes.save"));
        save.set_halign(gtk::Align::Start);
        f.append(&save);
        ex.set_child(Some(&f));

        let me = self.me.clone();
        let (internet, p2p, relays, err) = (
            internet.downgrade(),
            p2p.downgrade(),
            relays.downgrade(),
            err.downgrade(),
        );
        let checks: Vec<(String, gtk::glib::WeakRef<gtk::CheckButton>)> = checks
            .into_iter()
            .map(|(a, c)| (a, c.downgrade()))
            .collect();
        save.connect_clicked(move |_| {
            let (Some(r), Some(internet), Some(p2p), Some(relays), Some(err)) = (
                me.upgrade(),
                internet.upgrade(),
                p2p.upgrade(),
                relays.upgrade(),
                err.upgrade(),
            ) else {
                return;
            };
            let buf = relays.buffer();
            let form = RoutesForm {
                internet: internet.text().to_string(),
                p2p: p2p.text().to_string(),
                relays: buf
                    .text(&buf.start_iter(), &buf.end_iter(), false)
                    .to_string(),
                mail_friends: checks
                    .iter()
                    .filter(|(_, c)| c.upgrade().is_some_and(|c| c.is_active()))
                    .map(|(a, _)| a.clone())
                    .collect(),
            };
            let current = r.current.borrow().clone().unwrap_or_default();
            match routesview::form_patch(&r.friend.address, &form, &current) {
                Ok(Some(patch)) => {
                    err.set_visible(false);
                    r.save(patch, true);
                }
                Ok(None) => err.set_visible(false),
                Err(text) => {
                    err.set_text(&text);
                    err.set_visible(true);
                }
            }
        });
        ex
    }
}

fn section(title: &str) -> gtk::Label {
    let l = label(title, &["strong"]);
    l.set_margin_top(sp(LG));
    l
}
