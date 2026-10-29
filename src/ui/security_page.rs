//! The Security page (Calm), in the main window's sidebar: security
//! notices (`security.status`, read-only; resolving stays in the terminal)
//! as cards, and what Raven's background program counted. Lock while
//! running and Updates are in Settings (Lock, Updates).

use gtk::prelude::*;
use raven_desktop::secview;
use raven_desktop::strings::tr;
use raven_desktop::theme::tokens;
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::calm::{self, Clamp};
use super::widgets::{self, clear_box, details_expander, hbox, label, sp, vbox, SM, XS};

pub struct SecurityPage {
    ctx: Weak<Ctx>,
    me: Weak<SecurityPage>,
    pub root: gtk::ScrolledWindow,
    notices: gtk::Box,
}

fn section(title: &str) -> gtk::Label {
    let l = label(&title.to_uppercase(), &["h2"]);
    l.set_wrap(false);
    l
}

fn small(text: &str) -> gtk::Label {
    label(text, &["small", "dim"])
}

impl SecurityPage {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Self> {
        let page = vbox(sp(SM));
        page.add_css_class("page");
        let head = hbox(sp(SM));
        let title = calm::heading_label(tr("security.title"), &["page-title"]);
        title.set_hexpand(true);
        head.append(&title);
        let refresh = widgets::button(tr("network.refresh"));
        refresh.set_valign(gtk::Align::Center);
        head.append(&refresh);
        page.append(&head);
        page.append(&label(tr("security.intro"), &["intro"]));
        page.append(&section(tr("security.notices")));
        let notices = vbox(sp(SM));
        page.append(&notices);
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
            notices,
        });
        let me = Rc::downgrade(&p);
        refresh.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                p.refresh();
            }
        });
        p
    }

    /// Read the notices again.
    pub fn refresh(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.call(
            |e| e.security_status(),
            move |_, security| {
                let Some(p) = me.upgrade() else { return };
                clear_box(&p.notices);
                match security {
                    Ok(s) => p.show_security(&secview::security_view(&s)),
                    Err(e) => {
                        p.notices.append(&label(&e.message, &["danger"]));
                        p.notices.append(&details_expander(&e.detail));
                    }
                }
            },
        );
    }

    fn show_security(&self, v: &secview::SecurityView) {
        let b = &self.notices;
        if let Some(s) = v.state {
            b.append(&label(s, &["warning"]));
        }
        if let Some(e) = v.empty {
            let card = vbox(sp(XS));
            card.add_css_class("card");
            let row = hbox(10);
            let i = calm::icon(raven_desktop::icons::Icon::ShieldCheck, 18);
            i.add_css_class("success");
            row.append(&i);
            row.append(&label(e, &["t-label"]));
            card.append(&row);
            b.append(&card);
        }
        for n in &v.notices {
            let card = vbox(sp(XS));
            card.add_css_class("card");
            let top = hbox(sp(SM));
            let level = label(n.level, &["strong", n.css]);
            level.set_hexpand(false);
            top.append(&level);
            let headline = calm::heading_label(&n.headline, &["t-label"]);
            headline.set_hexpand(true);
            top.append(&headline);
            top.append(&small(&n.when));
            card.append(&top);
            for text in [&n.did, &n.todo].into_iter().flatten() {
                card.append(&label(text, &[]));
            }
            if !n.details.is_empty() {
                card.append(&details_expander(&n.details.join("\n")));
            }
            b.append(&card);
        }
        let counts = vbox(sp(XS));
        counts.add_css_class("card");
        if let Some(k) = v.kept {
            counts.append(&small(k));
        }
        if let Some(c) = &v.counted {
            counts.append(&small(c));
        }
        if let Some(n) = &v.node {
            counts.append(&small(n));
        }
        counts.append(&small(v.terminal));
        b.append(&counts);
    }
}
