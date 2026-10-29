//! Security words (the sidebar's "You" and "Security words"): your 10
//! words as Calm cards with the read-aloud highlight, why word 10 is a
//! check word, that Raven never verifies a friend by itself, your old code
//! (still accepted), your fingerprint and your card to send to a friend.

use gtk::prelude::*;
use raven_desktop::api::Me;
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use raven_desktop::words::group_old_code;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::calm::{heading_label, Clamp, MarkView};
use super::widgets::{self, clear_box, hbox, label, vbox, WordGrid};

pub struct WordsPage {
    ctx: Weak<Ctx>,
    pub root: gtk::ScrolledWindow,
    content: gtk::Box,
    grid: RefCell<Option<Rc<WordGrid>>>,
}

impl WordsPage {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<Self> {
        let content = vbox(10);
        content.add_css_class("page");
        let clamp = Clamp::new(tokens().layout.page_max_width.min(720), &content);
        let root = gtk::ScrolledWindow::new();
        root.set_hscrollbar_policy(gtk::PolicyType::Never);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.set_child(Some(&clamp));
        Rc::new(Self {
            ctx: Rc::downgrade(ctx),
            root,
            content,
            grid: RefCell::new(None),
        })
    }

    /// Fill the page from `identity.me`.
    pub fn set_me(&self, me: &Me) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let c = &self.content;
        clear_box(c);
        let title_row = hbox(12);
        let mark = MarkView::new(28);
        title_row.append(&mark.area);
        title_row.append(&heading_label(tr("words.title"), &["page-title"]));
        c.append(&title_row);
        c.append(&label(tr("words.body"), &["lead"]));
        let grid = WordGrid::new(&me.words, &ctx.style);
        grid.root.set_margin_top(8);
        c.append(&grid.root);
        self.grid.replace(Some(grid));
        let fine = vbox(4);
        fine.add_css_class("fine");
        fine.append(&label(tr("words.check"), &[]));
        fine.append(&label(tr("words.never"), &[]));
        if !me.old_code.is_empty() {
            let old = label(
                &trf("words.oldCode", &[("code", &group_old_code(&me.old_code))]),
                &[],
            );
            old.set_selectable(true);
            fine.append(&old);
            fine.append(&label(
                &trf("you.fingerprint", &[("code", &me.old_code)]),
                &[],
            ));
        }
        c.append(&fine);
        if !me.card.is_empty() {
            let card_title = label(tr("you.card"), &["dim"]);
            card_title.set_margin_top(18);
            c.append(&card_title);
            let text = label(&me.card, &["mono"]);
            text.set_selectable(true);
            let card = vbox(8);
            card.add_css_class("card");
            card.append(&text);
            let copy = widgets::button(tr("add.share.copy"));
            copy.set_halign(gtk::Align::Start);
            let value = me.card.clone();
            copy.connect_clicked(move |b| {
                b.clipboard().set_text(&value);
                b.set_label(tr("add.share.copied"));
            });
            card.append(&copy);
            c.append(&card);
        }
    }
}
