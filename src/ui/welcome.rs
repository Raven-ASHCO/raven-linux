//! First run (Calm): Welcome (the dotted mark fades and scales in once, the
//! R A V E N wordmark, the tagline, three steps), then Setup on a card with
//! its step strip (Your name, Receive messages, Your identity, Security
//! words): the name, receiving (with the Linux firewall sentence), the
//! identity being made (the mark's dots gather while the engine makes the
//! key: a real wait, finished by the engine's answer), and your security
//! words as cards with the read-aloud highlight.

use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::{ApiError, SettingsPatch};
use raven_desktop::netview;
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use raven_desktop::words::group_old_code;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::app::Ctx;
use super::calm::{self, heading_label, Clamp, MarkView, Tween};
use super::widgets::{
    self, details_expander, hbox, label, primary_button, sp, vbox, StepStrip, WordGrid, LG, MD, SM,
    XL,
};

struct Setup {
    ctx: Rc<Ctx>,
    stack: gtk::Stack,
    steps: StepStrip,
    steps_box: gtk::Box,
    name: gtk::Entry,
    receive: Cell<bool>,
    forge: MarkView,
    forge_tween: Tween,
    creating_error: gtk::Box,
    words_page: gtk::Box,
    words_grid: RefCell<Option<Rc<WordGrid>>>,
}

pub fn show(ctx: &Rc<Ctx>) {
    let window = gtk::ApplicationWindow::new(&ctx.app);
    window.set_title(Some(tr("welcome.title")));
    window.set_default_size(720, 680);
    ctx.style.decorate_window(&window);
    let stack = gtk::Stack::new();
    // Moving between setup steps is the person's own action: a short fade.
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(ctx.style.duration(tokens().calm.view_in));
    let steps = StepStrip::new(&[
        tr("setup.step.name"),
        tr("setup.step.receive"),
        tr("setup.step.identity"),
        tr("setup.step.words"),
    ]);
    let steps_box = vbox(0);
    steps_box.append(&steps.root);
    steps_box.set_margin_bottom(sp(LG));
    steps_box.set_visible(false);
    let forge = MarkView::new(132);
    let setup = Rc::new(Setup {
        ctx: ctx.clone(),
        stack: stack.clone(),
        steps,
        steps_box: steps_box.clone(),
        name: gtk::Entry::new(),
        receive: Cell::new(true),
        forge,
        forge_tween: Tween::default(),
        creating_error: vbox(sp(SM)),
        words_page: page(),
        words_grid: RefCell::new(None),
    });
    stack.add_named(&setup.welcome_page(), Some("welcome"));
    stack.add_named(&setup.name_page(), Some("name"));
    stack.add_named(&setup.receive_page(), Some("receive"));
    stack.add_named(&setup.creating_page(), Some("creating"));
    stack.add_named(&setup.words_page, Some("words"));
    // A sheet-like card in the middle of the window.
    let card = vbox(0);
    card.add_css_class("card");
    card.add_css_class("sheet-body");
    card.append(&steps_box);
    card.append(&stack);
    card.set_valign(gtk::Align::Center);
    let clamp = Clamp::new(680, &card);
    clamp.set_margin_top(sp(XL));
    clamp.set_margin_bottom(sp(XL));
    clamp.set_margin_start(sp(LG));
    clamp.set_margin_end(sp(LG));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_child(Some(&clamp));
    window.set_child(Some(&scroll));
    // Signal handlers hold only weak references; this keeps the setup state
    // alive exactly as long as the window.
    widgets::keep_alive(&window, setup);
    if let Some(old) = ctx.first_window.replace(Some(window.clone().upcast())) {
        old.destroy();
    }
    window.present();
}

fn page() -> gtk::Box {
    vbox(sp(MD))
}

impl Setup {
    /// Show a setup step (0 name, 1 receive, 2 identity, 3 words).
    fn go(&self, name: &str, step: usize) {
        self.steps_box.set_visible(true);
        self.steps.set(step);
        self.stack.set_visible_child_name(name);
    }

    fn welcome_page(self: &Rc<Self>) -> gtk::Box {
        let p = page();
        let logo = MarkView::new(110);
        logo.area.add_css_class("logo");
        // The only decorative motion: the mark fades and scales in once.
        let style = self.ctx.style.clone();
        let shown = Rc::new(Cell::new(false));
        logo.area.connect_map(move |logo| {
            if shown.replace(true) {
                return;
            }
            calm::play_class(logo, &style, "reveal", tokens().durations.logo_reveal);
        });
        p.append(&logo.area);
        let wordmark = gtk::Label::new(Some(&tr("app.name").to_uppercase()));
        wordmark.add_css_class("display");
        p.append(&wordmark);
        let tagline = label(tr("app.tagline"), &["dim"]);
        tagline.set_xalign(0.5);
        tagline.set_justify(gtk::Justification::Center);
        p.append(&tagline);
        let title = heading_label(tr("welcome.title"), &["t-title"]);
        title.set_margin_top(sp(LG));
        p.append(&title);
        p.append(&label(tr("welcome.body1"), &[]));
        // Whom Raven reaches follows the engine: "the same network" only
        // while LAN direct is the one way on (network.status needs no
        // identity), else the ways it has on.
        let reach = label("", &["dim"]);
        reach.set_visible(false);
        p.append(&reach);
        let reach = reach.downgrade();
        self.ctx.call(
            |e| e.network_status(),
            move |_, result| {
                if let Some(reach) = reach.upgrade() {
                    reach.set_text(&netview::welcome_reach_line(result.ok().as_ref()));
                    reach.set_visible(true);
                }
            },
        );
        let steps = vbox(sp(SM));
        steps.set_margin_top(sp(SM));
        for (i, key) in [
            tr("welcome.step1"),
            tr("welcome.step2"),
            tr("welcome.step3"),
        ]
        .iter()
        .enumerate()
        {
            let row = hbox(sp(SM));
            let n = gtk::Label::new(Some(&(i + 1).to_string()));
            n.add_css_class("step-i");
            n.set_size_request(18, 18);
            n.set_valign(gtk::Align::Start);
            row.append(&n);
            row.append(&label(key, &[]));
            steps.append(&row);
        }
        p.append(&steps);
        let start = primary_button(tr("welcome.start"));
        start.set_halign(gtk::Align::End);
        start.set_margin_top(sp(XL));
        let me = Rc::downgrade(self);
        start.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.go("name", 0);
                s.name.grab_focus();
            }
        });
        p.append(&start);
        p
    }

    fn name_page(self: &Rc<Self>) -> gtk::Box {
        let p = page();
        p.append(&heading_label(tr("setup.name.title"), &["t-title"]));
        p.append(&label(tr("setup.name.body"), &["dim"]));
        self.name
            .set_placeholder_text(Some(tr("setup.name.placeholder")));
        self.name.add_css_class("raven-field");
        super::a11y::set_label(&self.name, tr("setup.name.title"));
        p.append(&self.name);
        let row = widgets::action_row();
        let next = primary_button(tr("common.continue"));
        let me = Rc::downgrade(self);
        let go = move || {
            if let Some(s) = me.upgrade() {
                s.go("receive", 1);
            }
        };
        let go2 = go.clone();
        next.connect_clicked(move |_| go());
        self.name.connect_activate(move |_| go2());
        row.append(&next);
        p.append(&row);
        p
    }

    fn receive_page(self: &Rc<Self>) -> gtk::Box {
        let p = page();
        p.append(&heading_label(tr("setup.receive.title"), &["t-title"]));
        p.append(&label(tr("setup.receive.body"), &[]));
        p.append(&label(tr("setup.receive.linux"), &["dim"]));
        let row = widgets::action_row();
        let no = widgets::button(tr("setup.receive.no"));
        let yes = primary_button(tr("setup.receive.yes"));
        for (b, receive) in [(&no, false), (&yes, true)] {
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(s) = me.upgrade() {
                    s.receive.set(receive);
                    s.create_identity();
                }
            });
        }
        row.append(&no);
        row.append(&yes);
        p.append(&row);
        p
    }

    fn creating_page(self: &Rc<Self>) -> gtk::Box {
        let p = page();
        p.add_css_class("forge");
        p.append(&self.forge.area);
        let title = heading_label(tr("setup.creating"), &["t-title"]);
        title.set_xalign(0.5);
        p.append(&title);
        let body = label(tr("setup.creating.body"), &["dim"]);
        body.set_xalign(0.5);
        body.set_justify(gtk::Justification::Center);
        p.append(&body);
        self.creating_error.set_visible(false);
        p.append(&self.creating_error);
        p
    }

    /// "Creating…" stays on screen while the engine thread makes the key,
    /// saves the choice and starts the background program. The mark's dots
    /// gather meanwhile (most of them in 1.5 s, the last ones when the
    /// engine answers); without motion the mark is simply there.
    fn create_identity(self: &Rc<Self>) {
        self.go("creating", 2);
        self.creating_error.set_visible(false);
        let style = self.ctx.style.clone();
        if style.motion_allowed() {
            self.forge.set_reveal(0.0);
            let area = self.forge.area.downgrade();
            let me = Rc::downgrade(self);
            self.forge_tween
                .run(&self.forge.area, &style, 1_500, "arrive", move |t| {
                    if let (Some(s), Some(_)) = (me.upgrade(), area.upgrade()) {
                        s.forge.set_reveal(0.9 * t);
                    }
                });
        } else {
            self.forge.set_reveal(1.0);
        }
        let name = self.name.text().trim().to_string();
        let name = (!name.is_empty()).then_some(name);
        let receive = self.receive.get();
        let me = Rc::downgrade(self);
        self.ctx.call(
            move |e| {
                let id = e.identity_create(name.as_deref())?;
                let settings = e.settings_set(&SettingsPatch {
                    receive: Some(receive),
                    ..Default::default()
                })?;
                let node = e.node_start().ok();
                let old_code = e.identity_me().map(|m| m.old_code).unwrap_or(id.old_code);
                Ok((id.words, old_code, settings, node))
            },
            move |ctx, result| {
                let Some(s) = me.upgrade() else { return };
                match result {
                    Ok((words, old_code, settings, node)) => {
                        ctx.settings_changed(settings);
                        if let Some(n) = node {
                            ctx.node.replace(n);
                        }
                        s.forge_tween.stop();
                        s.forge.set_reveal(1.0);
                        s.fill_words_page(&words, &old_code);
                        let me = Rc::downgrade(&s);
                        let wait = if ctx.style.motion_allowed() { 350 } else { 0 };
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(wait),
                            move || {
                                if let Some(s) = me.upgrade() {
                                    s.go("words", 3);
                                }
                            },
                        );
                    }
                    Err(e) => s.show_create_error(&e),
                }
            },
        );
    }

    fn show_create_error(self: &Rc<Self>, e: &ApiError) {
        self.forge_tween.stop();
        widgets::clear_box(&self.creating_error);
        self.creating_error.append(&label(&e.message, &["danger"]));
        self.creating_error.append(&details_expander(&e.detail));
        let back = widgets::button(tr("common.back"));
        back.set_halign(gtk::Align::Start);
        let me = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.go("receive", 1);
            }
        });
        self.creating_error.append(&back);
        self.creating_error.set_visible(true);
    }

    fn fill_words_page(self: &Rc<Self>, words: &[String], old_code: &str) {
        let p = &self.words_page;
        widgets::clear_box(p);
        let title = hbox(10);
        let mark = MarkView::new(26);
        title.append(&mark.area);
        title.append(&heading_label(tr("words.title"), &["page-title"]));
        p.append(&title);
        p.append(&label(tr("words.body"), &["lead"]));
        let grid = WordGrid::new(words, &self.ctx.style);
        p.append(&grid.root);
        self.words_grid.replace(Some(grid));
        let fine = vbox(4);
        fine.add_css_class("fine");
        fine.append(&label(tr("words.check"), &[]));
        fine.append(&label(tr("words.never"), &[]));
        if !old_code.is_empty() {
            let old = label(
                &trf("words.oldCode", &[("code", &group_old_code(old_code))]),
                &[],
            );
            old.set_selectable(true);
            fine.append(&old);
        }
        p.append(&fine);
        let row = widgets::action_row();
        let done = primary_button(tr("common.done"));
        let me = Rc::downgrade(self);
        done.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                // Opens the main window and closes this one.
                s.ctx.open_main();
            }
        });
        row.append(&done);
        p.append(&row);
    }
}
