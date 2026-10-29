//! "Add a friend": a step strip (Share yours · Paste theirs · Check words).
//! Share shows your invite with Copy and the include-address toggle; Paste
//! takes the friend's invite or card, which collapses to a one-line summary
//! once the engine has read it. The friend's security words are never shown
//! (`card.parse` does not return them).

use gtk::prelude::*;
use raven_desktop::api::{codes, ApiError, Contact, ParsedCard};
use raven_desktop::present;
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::check_words;
use super::widgets::{
    self, dialog, hbox, label, primary_button, sp, vbox, OneShot, StepStrip, MD, SM,
};

struct AddFriend {
    ctx: Rc<Ctx>,
    /// This dialog, for engine answers that arrive later.
    me: Weak<AddFriend>,
    window: gtk::Window,
    strip: StepStrip,
    stack: gtk::Stack,
    invite: gtk::Label,
    copy: gtk::Button,
    copy_reset: OneShot,
    include: gtk::Switch,
    paste: gtk::TextView,
    paste_revealer: gtk::Revealer,
    summary_row: gtk::Box,
    summary: gtk::Label,
    /// The engine's note about routes it left out of the pasted card
    /// (`card.parse` `note`), under the summary.
    card_note: gtk::Label,
    paste_error: gtk::Label,
    name: gtk::Entry,
    name_error: gtk::Label,
    add: gtk::Button,
    must_add_back: gtk::Label,
    parsed: RefCell<Option<ParsedCard>>,
    added: RefCell<Option<Contact>>,
    /// Counts invite requests, so only the newest answer is shown.
    invite_gen: Cell<u64>,
    /// "Add friend" is on its way to the engine.
    saving: Cell<bool>,
}

pub fn show(ctx: &Rc<Ctx>) {
    let parent = ctx.main_window();
    let (window, content) = dialog(parent.as_ref(), tr("add.title"), &ctx.style, 520);
    content.append(&widgets::heading(tr("add.title")));
    let strip = StepStrip::new(&[tr("add.step1"), tr("add.step2"), tr("add.step3")]);
    strip.root.set_margin_bottom(sp(SM));
    content.append(&strip.root);
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_vhomogeneous(false);
    content.append(&stack);

    let af = Rc::new_cyclic(|me| AddFriend {
        ctx: ctx.clone(),
        me: me.clone(),
        window: window.clone(),
        strip,
        stack,
        invite: label("", &["mono"]),
        copy: widgets::button(tr("add.share.copy")),
        copy_reset: OneShot::default(),
        include: gtk::Switch::new(),
        paste: gtk::TextView::new(),
        paste_revealer: gtk::Revealer::new(),
        summary_row: hbox(sp(SM)),
        summary: label("", &[]),
        card_note: label("", &["dim", "small"]),
        paste_error: label("", &["danger"]),
        name: gtk::Entry::new(),
        name_error: label("", &["danger"]),
        add: primary_button(tr("add.save")),
        must_add_back: label("", &[]),
        parsed: RefCell::new(None),
        added: RefCell::new(None),
        invite_gen: Cell::new(0),
        saving: Cell::new(false),
    });
    af.stack.add_named(&af.share_page(), Some("share"));
    af.stack.add_named(&af.paste_page(), Some("paste"));
    af.stack.add_named(&af.check_page(), Some("check"));
    af.go("share");
    widgets::keep_alive(&window, af.clone());
    window.present();
}

impl AddFriend {
    fn go(&self, page: &str) {
        self.stack.set_visible_child_name(page);
        self.strip.set(match page {
            "share" => 0,
            "paste" => 1,
            _ => 2,
        });
    }

    fn close(&self) {
        self.window.close();
    }

    fn share_page(self: &Rc<Self>) -> gtk::Box {
        let p = vbox(sp(SM));
        p.append(&label(tr("add.share.body"), &[]));
        let card = vbox(sp(SM));
        card.add_css_class("card");
        self.invite.set_selectable(true);
        self.invite.set_wrap_mode(gtk::pango::WrapMode::Char);
        card.append(&self.invite);
        let copy_row = hbox(sp(SM));
        self.copy.set_halign(gtk::Align::Start);
        copy_row.append(&self.copy);
        card.append(&copy_row);
        p.append(&card);

        // Include my network address (only when the engine knows it).
        let node = self.ctx.node.borrow().clone();
        if let Some(addr) = present::own_network_address(&node) {
            let text = trf("add.share.includeAddress", &[("addr", &addr)]);
            p.append(&widgets::pref_row(&text, &self.include));
        }
        p.append(&label(tr("add.share.public"), &["dim", "small"]));
        self.refresh_invite();

        let me = Rc::downgrade(self);
        self.include.connect_active_notify(move |_| {
            if let Some(s) = me.upgrade() {
                s.refresh_invite();
            }
        });
        let me = Rc::downgrade(self);
        self.copy.connect_clicked(move |b| {
            let Some(s) = me.upgrade() else { return };
            b.display().clipboard().set_text(&s.invite.text());
            b.set_label(tr("add.share.copied"));
            let b2 = b.downgrade();
            s.copy_reset
                .start(tokens().durations.toast_visible, move || {
                    if let Some(b) = b2.upgrade() {
                        b.set_label(tr("add.share.copy"));
                    }
                });
        });

        let row = widgets::action_row();
        let cancel = widgets::button(tr("common.cancel"));
        let next = primary_button(tr("common.next"));
        let me = Rc::downgrade(self);
        cancel.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.close();
            }
        });
        let me = Rc::downgrade(self);
        next.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.go("paste");
                s.paste.grab_focus();
            }
        });
        row.append(&cancel);
        row.append(&next);
        p.append(&row);
        p
    }

    fn refresh_invite(&self) {
        let generation = self.invite_gen.get() + 1;
        self.invite_gen.set(generation);
        let include = self.include.is_active();
        let me = self.me.clone();
        self.ctx.call(
            move |e| e.card_make(include),
            move |_, result| {
                let Some(s) = me.upgrade() else { return };
                if s.invite_gen.get() != generation {
                    return; // the switch changed again meanwhile
                }
                match result {
                    Ok(card) => s.invite.set_text(&card.invite),
                    Err(e) => s.invite.set_text(&e.message),
                }
            },
        );
    }

    fn paste_page(self: &Rc<Self>) -> gtk::Box {
        let p = vbox(sp(SM));
        p.append(&label(tr("add.paste.body"), &[]));
        self.paste.set_wrap_mode(gtk::WrapMode::Char);
        self.paste.add_css_class("mono");
        self.paste.add_css_class("raven-field");
        self.paste.set_accepts_tab(false);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_min_content_height(88);
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.add_css_class("card");
        scroll.set_child(Some(&self.paste));
        // The pasted card collapses to a summary once the engine read it.
        self.paste_revealer
            .set_transition_type(gtk::RevealerTransitionType::SlideUp);
        self.paste_revealer.set_child(Some(&scroll));
        self.paste_revealer.set_reveal_child(true);
        p.append(&self.paste_revealer);
        self.summary.set_hexpand(true);
        self.summary.add_css_class("success");
        let change = widgets::flat_button(tr("add.paste.change"));
        self.summary_row.add_css_class("card");
        self.summary_row.append(&self.summary);
        self.summary_row.append(&change);
        self.summary_row.set_visible(false);
        p.append(&self.summary_row);
        self.card_note.set_selectable(true);
        self.card_note.set_visible(false);
        p.append(&self.card_note);
        self.paste_error.set_visible(false);
        p.append(&self.paste_error);

        let name_title = label(tr("add.name.title"), &["strong"]);
        name_title.set_margin_top(sp(MD));
        p.append(&name_title);
        self.name.add_css_class("raven-field");
        p.append(&self.name);
        p.append(&label(tr("add.name.note"), &["dim", "small"]));
        self.name_error.set_visible(false);
        p.append(&self.name_error);

        let me = Rc::downgrade(self);
        self.paste.buffer().connect_changed(move |_| {
            if let Some(s) = me.upgrade() {
                s.parse_paste();
            }
        });
        let me = Rc::downgrade(self);
        change.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.paste_revealer.set_transition_duration(0);
                s.paste_revealer.set_reveal_child(true);
                s.summary_row.set_visible(false);
                s.card_note.set_visible(false);
                s.paste.grab_focus();
            }
        });
        let me = Rc::downgrade(self);
        self.name.connect_changed(move |_| {
            if let Some(s) = me.upgrade() {
                s.name_error.set_visible(false);
                s.update_add_sensitive();
            }
        });
        let me = Rc::downgrade(self);
        self.name.connect_activate(move |_| {
            if let Some(s) = me.upgrade() {
                if s.add.is_sensitive() {
                    s.save();
                }
            }
        });

        let row = widgets::action_row();
        let back = widgets::button(tr("common.back"));
        let me = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.go("share");
            }
        });
        self.add.set_sensitive(false);
        let me = Rc::downgrade(self);
        self.add.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.save();
            }
        });
        row.append(&back);
        row.append(&self.add);
        p.append(&row);
        p
    }

    fn paste_text(&self) -> String {
        let buffer = self.paste.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    fn parse_paste(&self) {
        let text = self.paste_text();
        self.parsed.replace(None);
        self.update_add_sensitive();
        if text.trim().is_empty() {
            self.paste_error.set_visible(false);
            self.card_note.set_visible(false);
            return;
        }
        let me = self.me.clone();
        let job_text = text.clone();
        self.ctx.call(
            move |e| e.card_parse(&job_text),
            move |_, result| {
                let Some(s) = me.upgrade() else { return };
                if s.paste_text() == text {
                    s.show_parsed(result); // else a newer paste is on its way
                }
            },
        );
    }

    fn show_parsed(&self, result: Result<ParsedCard, ApiError>) {
        match result {
            Ok(card) => {
                let summary = if card.has_address {
                    trf("add.paste.summary", &[("short", &card.short_address)])
                } else {
                    trf(
                        "add.paste.summaryNoAddress",
                        &[("short", &card.short_address)],
                    )
                };
                self.summary.set_text(&format!("\u{2713} {summary}"));
                self.summary_row.set_visible(true);
                // Routes the engine left out of the card (no key signs a
                // card's addresses): its own line, as `raven contact add`
                // prints it.
                match present::card_note(&card) {
                    Some(note) => {
                        self.card_note.set_text(&note);
                        self.card_note.set_visible(true);
                    }
                    None => self.card_note.set_visible(false),
                }
                self.paste_error.set_visible(false);
                // Collapsing follows the engine's answer: allowed to move.
                self.paste_revealer.set_transition_duration(
                    self.ctx.style.duration(tokens().durations.word_reveal),
                );
                self.paste_revealer.set_reveal_child(false);
                self.parsed.replace(Some(card));
                self.name.grab_focus();
            }
            Err(e) => {
                self.parsed.replace(None);
                self.card_note.set_visible(false);
                self.paste_error.set_text(&e.message);
                self.paste_error.set_visible(true);
            }
        }
        self.update_add_sensitive();
    }

    fn update_add_sensitive(&self) {
        let ok = self.parsed.borrow().is_some()
            && !self.name.text().trim().is_empty()
            && !self.saving.get();
        self.add.set_sensitive(ok);
    }

    fn save(self: &Rc<Self>) {
        if self.saving.replace(true) {
            return;
        }
        self.update_add_sensitive();
        let name = self.name.text().trim().to_string();
        let (text, job_name) = (self.paste_text(), name.clone());
        let me = self.me.clone();
        self.ctx.call(
            move |e| e.contacts_add(&text, &job_name),
            move |ctx, result| {
                let Some(s) = me.upgrade() else { return };
                s.saving.set(false);
                s.update_add_sensitive();
                match result {
                    Ok(contact) => {
                        s.must_add_back
                            .set_text(&trf("add.mustAddBack", &[("name", &contact.name)]));
                        let main = ctx.main.borrow().clone();
                        if let Some(m) = main {
                            m.friend_added(&contact.address);
                        }
                        s.added.replace(Some(contact));
                        s.go("check");
                    }
                    Err(e) if e.is(codes::NAME_CLASH) => {
                        s.name_error
                            .set_text(&trf("add.name.clash", &[("name", &name)]));
                        s.name_error.set_visible(true);
                    }
                    Err(e) => {
                        s.paste_error.set_text(&e.message);
                        s.paste_error.set_visible(true);
                    }
                }
            },
        );
    }

    fn check_page(self: &Rc<Self>) -> gtk::Box {
        let p = vbox(sp(SM));
        p.append(&self.must_add_back);
        let row = widgets::action_row();
        let later = widgets::button(tr("check.later"));
        let check = primary_button(tr("add.step3"));
        let me = Rc::downgrade(self);
        later.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.close();
            }
        });
        let me = Rc::downgrade(self);
        check.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            let contact = s.added.borrow().clone();
            let (Some(c), Some(parent)) = (contact, s.ctx.main_window()) else {
                return;
            };
            s.close();
            let ctx = Rc::downgrade(&s.ctx);
            check_words::show(
                &s.ctx,
                &parent,
                &c,
                Box::new(move || {
                    let main = ctx.upgrade().and_then(|c| c.main.borrow().clone());
                    if let Some(m) = main {
                        m.refresh_contacts();
                    }
                }),
            );
        });
        row.append(&later);
        row.append(&check);
        p.append(&row);
        p
    }
}
