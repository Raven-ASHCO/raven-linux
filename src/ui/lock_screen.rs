//! The locked window (Calm) of Lock while running (Lock v2, API.md
//! `lock.*`): Raven's background program keeps running without its keys
//! until the person unlocks. The window shows nothing of Raven but a card:
//! the mark, "Raven is locked", what that means, the Raven passphrase field
//! (when the profile has one), Unlock. The card fades in; a wrong
//! passphrase shakes it. The passphrase goes only to the background
//! program, through `lock.unlock`; the field is emptied as soon as it is
//! sent and nothing keeps it.

use gtk::prelude::*;
use raven_desktop::api::LockState;
use raven_desktop::secview;
use raven_desktop::strings::tr;
use raven_desktop::theme::tokens;
use std::rc::Rc;

use super::app::Ctx;
use super::calm::{self, heading_label, MarkView};
use super::widgets::{label, primary_button, vbox};

/// The Calm card of a locked window: mark, title and body; the caller adds
/// the rest. Returns the window, the card and its error line.
pub fn locked_window(ctx: &Rc<Ctx>, body: &str) -> (gtk::ApplicationWindow, gtk::Box, gtk::Label) {
    let w = gtk::ApplicationWindow::new(&ctx.app);
    w.set_title(Some(tr("lock.title")));
    w.set_default_size(520, 520);
    ctx.style.decorate_window(&w);
    let card = vbox(8);
    card.add_css_class("locked-card");
    card.set_size_request(360, -1);
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    let mark = MarkView::new(76);
    mark.area.set_margin_bottom(6);
    card.append(&mark.area);
    let title = heading_label(tr("lock.title"), &["locked-title"]);
    title.set_xalign(0.5);
    card.append(&title);
    let text = label(body, &["dim"]);
    text.set_xalign(0.5);
    text.set_justify(gtk::Justification::Center);
    card.append(&text);
    let error = label("", &["locked-err"]);
    error.set_xalign(0.5);
    error.set_justify(gtk::Justification::Center);
    w.set_child(Some(&card));
    // The card fades in once.
    let (c, st) = (card.downgrade(), ctx.style.clone());
    w.connect_map(move |_| {
        if let Some(c) = c.upgrade() {
            calm::play_class(&c, &st, "fade-in", tokens().calm.locked_fade);
        }
    });
    (w, card, error)
}

/// Shake the card (a wrong passphrase).
pub fn shake(ctx: &Ctx, card: &gtk::Box) {
    calm::play_class(card, &ctx.style, "shake", tokens().calm.shake);
}

pub fn show(ctx: &Rc<Ctx>, state: &LockState) -> gtk::ApplicationWindow {
    let view = secview::lock_view(state);
    let (w, card, error) = locked_window(ctx, tr("lock.body"));
    if let Some(lab) = view.lab {
        let l = label(lab, &["small", "accent"]);
        l.set_xalign(0.5);
        card.append(&l);
    }
    let entry = gtk::PasswordEntry::new();
    entry.set_show_peek_icon(true);
    entry.add_css_class("raven-field");
    entry.set_hexpand(true);
    if view.passphrase_field {
        // The field's accessible name is its placeholder's words.
        entry.set_placeholder_text(Some(tr("lock.passphrase")));
        super::a11y::set_label(&entry, tr("lock.passphrase"));
        entry.set_margin_top(8);
        card.append(&entry);
    }
    card.append(&error);
    if let Some(note) = view.no_passphrase {
        let n = label(note, &["small", "dim"]);
        n.set_xalign(0.5);
        n.set_justify(gtk::Justification::Center);
        card.append(&n);
    }
    let unlock = primary_button(tr("lock.unlock"));
    unlock.set_hexpand(true);
    card.append(&unlock);
    if view.passphrase_field {
        let n = label(tr("lock.passphrase.note"), &["small", "dim"]);
        n.set_xalign(0.5);
        n.set_justify(gtk::Justification::Center);
        n.set_margin_top(4);
        card.append(&n);
    }

    // The entry is held here: without a passphrase it isn't in the window.
    let (c, entry_ref, err, cd) = (
        Rc::downgrade(ctx),
        entry.clone(),
        error.downgrade(),
        card.downgrade(),
    );
    let send = move |button: &gtk::Button| {
        let Some(ctx) = c.upgrade() else {
            return;
        };
        let entry = &entry_ref;
        let typed = entry.text().to_string();
        entry.set_text("");
        button.set_sensitive(false);
        let (b, err, e2, cd) = (
            button.downgrade(),
            err.clone(),
            entry.downgrade(),
            cd.clone(),
        );
        ctx.call(
            move |eng| eng.lock_unlock(secview::passphrase_arg(&typed)),
            move |ctx, result| {
                if let Some(b) = b.upgrade() {
                    b.set_sensitive(true);
                }
                match result {
                    Ok(l) if !l.locked => ctx.unlocked(l),
                    Ok(l) => ctx.lock_changed(l),
                    Err(e) => {
                        if let Some(l) = err.upgrade() {
                            let text = secview::unlock_error(&e);
                            l.set_text(&text);
                            super::a11y::set_description(&l, &text);
                        }
                        if let Some(c) = cd.upgrade() {
                            shake(ctx, &c);
                        }
                        if let Some(entry) = e2.upgrade() {
                            entry.grab_focus();
                        }
                    }
                }
            },
        );
    };
    let send = Rc::new(send);
    let s = send.clone();
    unlock.connect_clicked(move |b| s(b));
    let (s, b) = (send, unlock.downgrade());
    entry.connect_activate(move |_| {
        if let Some(b) = b.upgrade() {
            s(&b);
        }
    });
    w.present();
    if view.passphrase_field {
        entry.grab_focus();
    } else {
        unlock.grab_focus();
    }
    w
}
