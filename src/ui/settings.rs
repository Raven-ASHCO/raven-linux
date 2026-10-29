//! Settings (Calm), a page of the main window: seven sections (General,
//! Appearance, Privacy, Lock, Network, Updates, About) in a 228-point list
//! whose selection glides to the chosen one; the section's content slides
//! in from the direction of travel. Each section is grouped cards of rows
//! (title, description, control on the right).
//!
//! Every value is the engine's (`settings.get/set`, `privacy.state`,
//! `lock.state`, `network.status`, `updates.status`, `app.info`) except
//! Theme and Show technical details, which only change how this app looks
//! (`appprefs`). A switch moves to the engine's answer; a network change
//! takes the Network page's own path (consent sheet, refusal of a held way,
//! restart prompt).

use gtk::prelude::*;
use raven_desktop::api::{Build, Me, PrivacyState, SettingsPatch, WhenQuit};
use raven_desktop::appprefs::ThemeChoice;
use raven_desktop::icons::{self, Icon};
use raven_desktop::netflow;
use raven_desktop::netview::{self, Chip as WayChip};
use raven_desktop::present::{self, TickIcon};
use raven_desktop::secview;
use raven_desktop::settingsview::{self, Section, Slide};
use raven_desktop::strings::{tr, tr_list, trf};
use raven_desktop::theme::tokens;
use raven_desktop::APP_ID;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::calm::{
    self, group_card, group_label, item, section_head, stat, Chip, Clamp, GlideList, MarkView,
    Segmented, TickMark, Tween,
};
use super::network_page::NetworkPage;
use super::widgets::{self, action_row, after, dialog, hbox, label, line, vbox, OneShot};

const WHEN_QUIT: [WhenQuit; 3] = [WhenQuit::Keep, WhenQuit::StopIfStarted, WhenQuit::Ask];
const KEEP_CHATS: [Option<u32>; 4] = [Some(7), Some(30), Some(90), None];
/// Developer builds name the lab switches (as the Network page).
const APP_DEBUG: bool = cfg!(debug_assertions);

pub struct SettingsPage {
    ctx: Weak<Ctx>,
    me: Weak<SettingsPage>,
    pub root: gtk::Box,
    nav: Rc<GlideList>,
    body: gtk::ScrolledWindow,
    current: Cell<Option<Section>>,
    identity: RefCell<Option<Me>>,
    /// The Network section's switches by setting, and the one the person
    /// just moved (focus goes back to it after the engine's answer).
    net_switches: RefCell<Vec<(&'static str, gtk::Switch)>>,
    refocus: Cell<Option<&'static str>>,
    /// What a restart from Settings > Network said (kept across re-renders).
    net_out: gtk::Box,
    /// The Appearance preview's two steps (cancelled when the page changes).
    preview: (OneShot, OneShot),
    /// The Network page: Settings > Network uses its flow and its states.
    net: Weak<NetworkPage>,
}

impl SettingsPage {
    pub fn new(ctx: &Rc<Ctx>, net: &Rc<NetworkPage>) -> Rc<Self> {
        let style = &ctx.style;
        let items: Vec<(&str, Icon, &str)> = Section::ALL
            .iter()
            .map(|s| (s.title(), s.icon(), s.id()))
            .collect();
        let nav = GlideList::new(style, &items);
        let nav_col = vbox(0);
        nav_col.add_css_class("setnav");
        nav_col.set_size_request(tokens().layout.settings_nav_width as i32, -1);
        let title = calm::heading_label(tr("settings.title"), &["setnav-title"]);
        nav_col.append(&title);
        nav_col.append(&nav.root);
        let body = gtk::ScrolledWindow::new();
        body.set_hscrollbar_policy(gtk::PolicyType::Never);
        body.set_hexpand(true);
        body.set_vexpand(true);
        let root = hbox(0);
        root.append(&nav_col);
        root.append(&body);
        let page = Rc::new_cyclic(|me| Self {
            ctx: Rc::downgrade(ctx),
            me: me.clone(),
            root,
            nav,
            body,
            current: Cell::new(None),
            identity: RefCell::new(None),
            net_switches: RefCell::new(Vec::new()),
            refocus: Cell::new(None),
            net_out: vbox(4),
            preview: (OneShot::default(), OneShot::default()),
            net: Rc::downgrade(net),
        });
        for (i, b) in page.nav.buttons.iter().enumerate() {
            let me = page.me.clone();
            b.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    p.open(Section::ALL[i]);
                }
            });
        }
        // Settings > Network shows the states the Network page reads.
        let me = page.me.clone();
        net.on_status(move || {
            if let Some(p) = me.upgrade() {
                if p.current.get() == Some(Section::Network) {
                    p.render(Section::Network, Slide::None);
                }
            }
        });
        page
    }

    /// Fill the page again (it was opened).
    pub fn refresh(&self) {
        let s = self.current.get().unwrap_or(Section::General);
        self.nav.select(s.index(), false);
        self.current.set(Some(s));
        self.render(s, Slide::None);
    }

    /// Show a section; the selection glides, the content slides in.
    pub fn open(&self, s: Section) {
        let slide = settingsview::slide(self.current.get(), s);
        self.current.set(Some(s));
        self.nav.select(s.index(), self.root.is_mapped());
        self.render(s, slide);
    }

    /// The background program changed (`node` event). Only the Network
    /// section shows its state live; General keeps what is being typed.
    pub fn node_changed(&self) {
        if self.current.get() == Some(Section::Network) {
            self.render(Section::Network, Slide::None);
        }
    }

    pub fn set_me(&self, me: &Me) {
        self.identity.replace(Some(me.clone()));
        if self.current.get() == Some(Section::About) {
            self.render(Section::About, Slide::None);
        }
    }

    fn render(&self, s: Section, slide: Slide) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        self.preview.0.cancel();
        self.preview.1.cancel();
        let page = vbox(0);
        page.add_css_class("sec-in");
        match s {
            Section::General => self.general(&ctx, &page),
            Section::Appearance => self.appearance(&ctx, &page),
            Section::Privacy => self.privacy(&ctx, &page),
            Section::Lock => self.lock(&ctx, &page),
            Section::Network => self.network(&ctx, &page),
            Section::Updates => self.updates(&ctx, &page),
            Section::About => self.about(&ctx, &page),
        }
        let clamp = Clamp::new(tokens().layout.settings_max_width, &page);
        // The same section again (a new engine answer): keep the place and
        // the focus; another section starts at the top.
        let adj = self.body.vadjustment();
        let keep = (slide == Slide::None).then(|| adj.value());
        self.body.set_child(Some(&clamp));
        match keep {
            Some(v) => super::chat_view::after_layout(&self.body, move || adj.set_value(v)),
            None => adj.set_value(0.0),
        }
        if let Some(setting) = self.refocus.take() {
            let switches = self.net_switches.borrow();
            if let Some((_, s)) = switches.iter().find(|(k, _)| *k == setting) {
                s.grab_focus();
            }
        }
        let class = match slide {
            Slide::None => return,
            Slide::FromBelow => "from-below",
            Slide::FromAbove => "from-above",
        };
        calm::play_class(&page, &ctx.style, class, tokens().calm.section_in);
    }

    /// Save a change. Refused: say why, and (`revert`) show the section
    /// again from the settings Raven keeps, so a switch moves back.
    fn save_then(&self, patch: SettingsPatch, revert: bool) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.call(
            move |e| e.settings_set(&patch),
            move |ctx, result| match result {
                Ok(s) => ctx.settings_changed(s),
                Err(e) => {
                    ctx.toast(&e.message, "bad");
                    if let (true, Some(p)) = (revert, me.upgrade()) {
                        if let Some(s) = p.current.get() {
                            p.render(s, Slide::None);
                        }
                    }
                }
            },
        );
    }

    fn save(&self, patch: SettingsPatch) {
        self.save_then(patch, true);
    }

    fn switch(&self, active: bool, patch: fn(bool) -> SettingsPatch) -> gtk::Switch {
        let s = gtk::Switch::new();
        // Set before the handler is connected: only the person's change saves.
        s.set_active(active);
        let me = self.me.clone();
        s.connect_active_notify(move |s| {
            if let Some(p) = me.upgrade() {
                p.save(patch(s.is_active()));
            }
        });
        s
    }

    // ------------------------------------------------------------ General

    fn general(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        let settings = ctx.settings.borrow().clone();
        section_head(
            page,
            tr("settings.general"),
            Some(tr("settings.general.lead")),
        );
        let card = group_card();
        let name = gtk::Entry::new();
        name.add_css_class("raven-field");
        name.set_width_chars(22);
        name.set_text(settings.display_name.as_deref().unwrap_or(""));
        name.set_placeholder_text(Some(tr("setup.name.placeholder")));
        let me = self.me.clone();
        let save_name = move |e: &gtk::Entry| {
            if let Some(p) = me.upgrade() {
                // A refused name keeps what was typed (no re-render).
                p.save_then(
                    SettingsPatch {
                        display_name: Some(e.text().trim().to_string()),
                        ..Default::default()
                    },
                    false,
                );
            }
        };
        let save2 = save_name.clone();
        name.connect_activate(save_name);
        let focus = gtk::EventControllerFocus::new();
        let entry = name.downgrade();
        focus.connect_leave(move |_| {
            if let Some(e) = entry.upgrade() {
                save2(&e);
            }
        });
        name.add_controller(focus);
        item(
            &card,
            tr("settings.name"),
            Some(tr("setup.name.body")),
            Some(name.upcast_ref()),
        );
        // Receive messages: the setting, then the background program's
        // own answer.
        let receive = gtk::Switch::new();
        receive.set_active(settings.receive);
        let me = self.me.clone();
        receive.connect_active_notify(move |s| {
            let Some(p) = me.upgrade() else { return };
            let Some(ctx) = p.ctx.upgrade() else { return };
            let patch = SettingsPatch {
                receive: Some(s.is_active()),
                ..Default::default()
            };
            ctx.run(
                move |e| (e.settings_set(&patch), e.node_status()),
                move |ctx, (settings, node)| {
                    match settings {
                        Ok(s) => ctx.settings_changed(s),
                        Err(e) => ctx.toast(&e.message, "bad"),
                    }
                    if let Ok(n) = node {
                        ctx.node.replace(n);
                    }
                },
            );
        });
        item(
            &card,
            tr("settings.receive"),
            Some(&present::listening_line(&ctx.node.borrow())),
            Some(receive.upcast_ref()),
        );
        page.append(&card);

        page.append(&group_label(tr("settings.whenQuit")));
        let card = group_card();
        let row = item(
            &card,
            tr("settings.whenQuit"),
            Some(tr("quit.running")),
            None,
        );
        let radios = vbox(2);
        radios.set_margin_top(6);
        let mut first: Option<gtk::CheckButton> = None;
        for (i, text) in tr_list("settings.whenQuit.options").iter().enumerate() {
            let r = gtk::CheckButton::with_label(text);
            r.add_css_class("raven-radio");
            if let Some(f) = &first {
                r.set_group(Some(f));
            } else {
                first = Some(r.clone());
            }
            r.set_active(WHEN_QUIT.get(i) == Some(&settings.when_quit));
            let me = self.me.clone();
            r.connect_toggled(move |r| {
                if !r.is_active() {
                    return;
                }
                if let (Some(p), Some(w)) = (me.upgrade(), WHEN_QUIT.get(i)) {
                    p.save(SettingsPatch {
                        when_quit: Some(*w),
                        ..Default::default()
                    });
                }
            });
            radios.append(&r);
        }
        // The radios go under the row's texts.
        if let Some(texts) = row.first_child().and_downcast::<gtk::Box>() {
            texts.append(&radios);
        }
        page.append(&card);
    }

    // ------------------------------------------------------------ Appearance

    fn appearance(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        let style = &ctx.style;
        section_head(
            page,
            tr("settings.appearance"),
            Some(tr("settings.appearance.lead")),
        );
        let card = group_card();
        let options = tr_list("settings.theme.options");
        let theme = Segmented::new(style, &options, style.theme().index(), tr("settings.theme"));
        let st = style.clone();
        theme.connect_changed(move |i| {
            if let Some(t) = ThemeChoice::ALL.get(i) {
                st.set_theme(*t);
            }
        });
        item(
            &card,
            tr("settings.theme"),
            Some(tr("settings.theme.body")),
            Some(theme.root.upcast_ref()),
        );
        widgets::keep_alive(&page.clone(), theme);
        let reduced = self.switch(ctx.settings.borrow().reduced_motion, |v| SettingsPatch {
            reduced_motion: Some(v),
            ..Default::default()
        });
        item(
            &card,
            tr("settings.reducedMotion"),
            Some(tr("settings.reducedMotion.body")),
            Some(reduced.upcast_ref()),
        );
        let details = gtk::Switch::new();
        details.set_active(style.details());
        let st = style.clone();
        details.connect_active_notify(move |s| st.set_details(s.is_active()));
        item(
            &card,
            tr("settings.details"),
            Some(tr("settings.details.body")),
            Some(details.upcast_ref()),
        );
        page.append(&card);

        // A preview of a send (the person's own action; nothing is sent).
        page.append(&group_label(tr("settings.preview")));
        let card = group_card();
        let row = hbox(18);
        row.add_css_class("preview-box");
        let msg = vbox(0);
        msg.add_css_class("msg");
        msg.add_css_class("out");
        msg.set_halign(gtk::Align::Start);
        let bubble = vbox(0);
        bubble.add_css_class("bubble");
        bubble.append(&label(tr("settings.preview.text"), &[]));
        msg.append(&bubble);
        let meta = hbox(8);
        meta.add_css_class("meta");
        let mark = TickMark::new(style);
        let tick = line("", &[]);
        let tick_box = hbox(5);
        tick_box.add_css_class("tick");
        tick_box.append(&mark.root);
        tick_box.append(&tick);
        meta.append(&tick_box);
        let chip = Chip::new(false);
        meta.append(&chip.root);
        msg.append(&meta);
        row.append(&msg);
        let play = widgets::button(tr("settings.preview.play"));
        play.set_valign(gtk::Align::Center);
        play.set_halign(gtk::Align::End);
        play.set_hexpand(true);
        row.append(&play);
        card.append(&row);
        let note = label(tr("settings.preview.note"), &["item-desc"]);
        note.set_margin_start(20);
        note.set_margin_bottom(14);
        card.append(&note);
        page.append(&card);
        let show = {
            let (mark, tick, tick_box, chip) =
                (mark.clone(), tick.clone(), tick_box.clone(), Rc::new(chip));
            let st = style.clone();
            move |state: TickIcon, arriving: bool| {
                let (text, tone) = match state {
                    TickIcon::Spinner => (tr("tick.sending"), ""),
                    TickIcon::Sent => (tr("tick.sent"), ""),
                    _ => (tr("tick.delivered"), "ok"),
                };
                tick.set_text(text);
                tick_box.remove_css_class("ok");
                if !tone.is_empty() {
                    tick_box.add_css_class(tone);
                }
                mark.set(state, arriving);
                if state == TickIcon::Delivered {
                    let words = tr("network.path.lan");
                    chip.set(words, present::path_style(Some("lan")), words);
                    if arriving {
                        calm::play_class(&chip.root, &st, "slide", tokens().calm.chip_slide);
                    }
                } else {
                    chip.hide();
                }
            }
        };
        show(TickIcon::Delivered, false);
        let show = Rc::new(show);
        let (me, st, msg_w) = (self.me.clone(), style.clone(), msg.downgrade());
        play.connect_clicked(move |_| {
            let Some(p) = me.upgrade() else { return };
            show(TickIcon::Spinner, false);
            if let Some(m) = msg_w.upgrade() {
                calm::play_class(&m, &st, "lift", tokens().calm.bubble_lift);
            }
            let (s1, s2) = (show.clone(), show.clone());
            p.preview.0.start(450, move || s1(TickIcon::Sent, false));
            p.preview
                .1
                .start(950, move || s2(TickIcon::Delivered, true));
        });
    }

    // ------------------------------------------------------------ Privacy

    fn privacy(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        let settings = ctx.settings.borrow().clone();
        section_head(
            page,
            tr("settings.privacy"),
            Some(tr("settings.privacy.lead")),
        );
        page.append(&group_label(tr("settings.stored.title")));
        let stored = group_card();
        let stored_box = vbox(10);
        stored_box.set_margin_top(14);
        stored_box.set_margin_bottom(14);
        stored_box.set_margin_start(16);
        stored_box.set_margin_end(16);
        stored.append(&stored_box);
        page.append(&stored);
        let card = group_card();
        let encrypt_slot = hbox(0);
        item(
            &card,
            tr("settings.encrypt"),
            None,
            Some(encrypt_slot.upcast_ref()),
        );
        let options = tr_list("settings.keepChats.options");
        let keep_index = KEEP_CHATS
            .iter()
            .position(|k| *k == settings.keep_chats_days)
            .unwrap_or(1);
        let keep = Segmented::new(&ctx.style, &options, keep_index, tr("settings.keepChats"));
        let me = self.me.clone();
        keep.connect_changed(move |i| {
            if let (Some(p), Some(k)) = (me.upgrade(), KEEP_CHATS.get(i)) {
                p.save(SettingsPatch {
                    keep_chats_days: Some(*k),
                    ..Default::default()
                });
            }
        });
        item(
            &card,
            tr("settings.keepChats"),
            Some(tr("settings.keepChats.body")),
            Some(keep.root.upcast_ref()),
        );
        widgets::keep_alive(&page.clone(), keep);
        let forget = self.switch(settings.forget_addresses, |v| SettingsPatch {
            forget_addresses: Some(v),
            ..Default::default()
        });
        item(
            &card,
            tr("settings.forget"),
            None,
            Some(forget.upcast_ref()),
        );
        page.append(&card);
        let card = group_card();
        let erase = widgets::button(tr("settings.erase.button"));
        erase.add_css_class("raven-danger");
        erase.set_sensitive(false);
        item(
            &card,
            tr("settings.erase"),
            Some(tr("erase.body")),
            Some(erase.upcast_ref()),
        );
        page.append(&card);
        let me = self.me.clone();
        erase.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                p.confirm_erase();
            }
        });
        // What is stored, and whether encryption and Erase are here yet.
        let (slot, stored_box, erase) = (
            encrypt_slot.downgrade(),
            stored_box.downgrade(),
            erase.downgrade(),
        );
        let me = self.me.clone();
        ctx.call(
            |e| e.privacy_state(),
            move |ctx, result| {
                // A failed call shows the same as "not available".
                let state: PrivacyState = result.unwrap_or_default();
                ctx.privacy_available.set(state.available);
                let (Some(slot), Some(stored_box), Some(erase)) =
                    (slot.upgrade(), stored_box.upgrade(), erase.upgrade())
                else {
                    return;
                };
                let row = |icon: Icon, class: &str, text: &str| {
                    let r = hbox(10);
                    r.add_css_class("stored-row");
                    let i = calm::icon(icon, 18);
                    i.add_css_class(class);
                    i.set_valign(gtk::Align::Start);
                    r.append(&i);
                    let l = label(text, &[]);
                    l.set_hexpand(true);
                    r.append(&l);
                    r
                };
                if state.encrypted {
                    stored_box.append(&row(Icon::Lock, "e", tr("settings.stored.encrypted")));
                    stored_box.append(&row(Icon::Lock, "p", tr("settings.stored.plain")));
                } else {
                    stored_box.append(&row(Icon::Lock, "p", tr("settings.stored.notEncrypted")));
                }
                widgets::clear_box(&slot);
                if !state.available {
                    slot.append(&stat(tr("net.preview"), "w"));
                } else if state.encrypted {
                    // There is no "decrypt": once on, it stays on.
                    slot.append(&stat(tr("net.on"), ""));
                } else {
                    let s = gtk::Switch::new();
                    let me = me.clone();
                    s.connect_active_notify(move |s| {
                        if s.is_active() {
                            s.set_sensitive(false);
                            if let Some(p) = me.upgrade() {
                                p.encrypt_now();
                            }
                        }
                    });
                    slot.append(&s);
                }
                erase.set_sensitive(state.available);
            },
        );
    }

    fn encrypt_now(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let me = self.me.clone();
        ctx.call(
            |e| {
                e.privacy_encrypt()?;
                e.privacy_state()
            },
            move |ctx, result| {
                if let Err(e) = result {
                    ctx.toast(&e.message, "bad");
                }
                if let Some(p) = me.upgrade() {
                    if p.current.get() == Some(Section::Privacy) {
                        p.render(Section::Privacy, Slide::None);
                    }
                }
            },
        );
    }

    /// "Erase Raven from this computer?": what is removed; Erase only after
    /// the person typed "erase".
    fn confirm_erase(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        if !ctx.privacy_available.get() {
            return;
        }
        let parent = ctx.main_window();
        let (window, content) = dialog(parent.as_ref(), tr("erase.title"), &ctx.style, 460);
        let ic = hbox(0);
        ic.add_css_class("lkic");
        ic.add_css_class("danger");
        ic.set_halign(gtk::Align::Start);
        let i = calm::icon(Icon::Lock, 24);
        i.set_hexpand(true);
        ic.append(&i);
        content.append(&ic);
        content.append(&calm::heading_label(tr("erase.title"), &["sheet-title"]));
        content.append(&label(tr("erase.body"), &["sheet-text"]));
        let prompt = label(tr("erase.typeToConfirm"), &["item-desc"]);
        prompt.set_margin_top(6);
        content.append(&prompt);
        let entry = gtk::Entry::new();
        entry.add_css_class("raven-field");
        prompt.set_mnemonic_widget(Some(&entry));
        content.append(&entry);
        let error = label("", &["danger"]);
        error.set_visible(false);
        content.append(&error);
        let row = action_row();
        let cancel = widgets::button(tr("common.cancel"));
        let erase = widgets::primary_button(tr("erase.confirm"));
        erase.add_css_class("danger-fill");
        erase.set_sensitive(false);
        row.append(&cancel);
        row.append(&erase);
        content.append(&row);
        let typed_erase = |e: &gtk::Entry| e.text().trim().eq_ignore_ascii_case("erase");
        let button = erase.downgrade();
        entry.connect_changed(move |e| {
            if let Some(b) = button.upgrade() {
                b.set_sensitive(typed_erase(e));
            }
        });
        let w = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        let (w, input, err, c) = (
            window.downgrade(),
            entry.downgrade(),
            error.downgrade(),
            Rc::downgrade(&ctx),
        );
        erase.connect_clicked(move |b| {
            let (Some(e), Some(ctx)) = (input.upgrade(), c.upgrade()) else {
                return;
            };
            if !typed_erase(&e) {
                return;
            }
            b.set_sensitive(false);
            let (w, b, err) = (w.clone(), b.downgrade(), err.clone());
            ctx.call(
                |e| e.privacy_erase(),
                move |ctx, result| match result {
                    Ok(()) => {
                        if let Some(w) = w.upgrade() {
                            w.close();
                        }
                        ctx.erased();
                    }
                    Err(e) => {
                        if let Some(b) = b.upgrade() {
                            b.set_sensitive(true);
                        }
                        if let Some(l) = err.upgrade() {
                            l.set_text(&e.message);
                            l.set_visible(true);
                        }
                    }
                },
            );
        });
        window.present();
        entry.grab_focus();
    }

    // ------------------------------------------------------------ Lock

    fn lock(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        section_head(page, tr("lock.section"), Some(tr("lock.v2.body")));
        let card = group_card();
        let lockrow = hbox(14);
        lockrow.add_css_class("lockrow");
        let lk = hbox(0);
        lk.add_css_class("lk");
        lk.set_valign(gtk::Align::Center);
        let i = calm::icon(Icon::Lock, 22);
        i.set_hexpand(true);
        lk.append(&i);
        lockrow.append(&lk);
        let texts = vbox(2);
        texts.set_hexpand(true);
        texts.set_valign(gtk::Align::Center);
        let status = label("", &["item-title"]);
        let body = label("", &["item-desc"]);
        texts.append(&status);
        texts.append(&body);
        lockrow.append(&texts);
        let lock_now = widgets::primary_button(tr("lock.nowEllipsis"));
        lock_now.set_valign(gtk::Align::Center);
        lock_now.set_sensitive(false);
        lockrow.append(&lock_now);
        card.append(&lockrow);
        page.append(&card);
        let card = group_card();
        let sleep = self.switch(ctx.settings.borrow().lock_on_sleep, |v| SettingsPatch {
            lock_on_sleep: Some(v),
            ..Default::default()
        });
        item(
            &card,
            tr("settings.lockOnSleep"),
            Some(tr("settings.lockOnSleep.body")),
            Some(sleep.upcast_ref()),
        );
        let pass_row = item(
            &card,
            tr("lock.passphrase"),
            Some(tr("lock.v2.passphraseNote")),
            None,
        );
        page.append(&card);
        let lab = label("", &["small", "accent"]);
        lab.set_visible(false);
        page.append(&lab);
        let me = self.me.clone();
        lock_now.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                p.lock_sheet();
            }
        });
        let (status, body, lock_now, lk, lab, pass_row) = (
            status.downgrade(),
            body.downgrade(),
            lock_now.downgrade(),
            lk.downgrade(),
            lab.downgrade(),
            pass_row.downgrade(),
        );
        ctx.call(
            |e| e.lock_state(),
            move |_, result| {
                let (Some(status), Some(body), Some(lock_now), Some(lk), Some(lab), Some(pass_row)) = (
                    status.upgrade(),
                    body.upgrade(),
                    lock_now.upgrade(),
                    lk.upgrade(),
                    lab.upgrade(),
                    pass_row.upgrade(),
                ) else {
                    return;
                };
                let state = result.unwrap_or_default();
                let v = secview::lock_view(&state);
                status.set_text(if v.locked {
                    tr("lock.isLocked")
                } else {
                    tr("lock.isOpen")
                });
                body.set_text(if v.can_lock { tr("lock.ready") } else { v.status });
                lock_now.set_sensitive(v.can_lock);
                if v.locked {
                    lk.add_css_class("locked");
                }
                if let Some(l) = v.lab {
                    lab.set_text(l);
                    lab.set_visible(true);
                }
                // The passphrase row: what unlocking asks for.
                if let Some(texts) = pass_row.first_child().and_downcast::<gtk::Box>() {
                    if let Some(note) = v.no_passphrase {
                        texts.append(&label(note, &["item-desc"]));
                    }
                    texts.append(&label(tr("lock.passphraseTerminal"), &["item-desc"]));
                }
            },
        );
    }

    /// "Lock Raven now?": what Lock does and doesn't do; Lock closes the
    /// shackle, then Raven locks and the locked window fades in.
    fn lock_sheet(&self) {
        let Some(ctx) = self.ctx.upgrade() else {
            return;
        };
        let parent = ctx.main_window();
        let (window, content) = dialog(parent.as_ref(), tr("lock.confirm.title"), &ctx.style, 460);
        let icon = LockIcon::new();
        let ic = hbox(0);
        ic.add_css_class("lkic");
        ic.set_halign(gtk::Align::Start);
        icon.area.set_hexpand(true);
        ic.append(&icon.area);
        content.append(&ic);
        content.append(&calm::heading_label(
            tr("lock.confirm.title"),
            &["sheet-title"],
        ));
        content.append(&label(tr("lock.confirm.does"), &["sheet-text"]));
        content.append(&label(tr("lock.confirm.doesNot"), &["sheet-text"]));
        let row = action_row();
        let cancel = widgets::button(tr("common.cancel"));
        let lock = widgets::primary_button(tr("lock.confirm.lock"));
        row.append(&cancel);
        row.append(&lock);
        content.append(&row);
        let w = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        let (w, c) = (window.downgrade(), Rc::downgrade(&ctx));
        let icon = Rc::new(icon);
        lock.connect_clicked(move |b| {
            let Some(ctx) = c.upgrade() else { return };
            b.set_sensitive(false);
            let w = w.clone();
            let c2 = c.clone();
            icon.close(&ctx.style, move || {
                if let Some(w) = w.upgrade() {
                    w.close();
                }
                if let Some(ctx) = c2.upgrade() {
                    ctx.lock_now();
                }
            });
        });
        window.present();
        lock.grab_focus();
    }

    // ------------------------------------------------------------ Network

    fn network(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        section_head(page, tr("settings.network"), Some(tr("network.intro")));
        let open = widgets::quiet_button(&format!("{} \u{2192}", tr("network.openPage")));
        open.set_halign(gtk::Align::Start);
        open.set_margin_bottom(14);
        open.set_action_name(Some("app.network"));
        page.append(&open);
        let Some(net) = self.net.upgrade() else {
            return;
        };
        // Changes that wait for a restart.
        if ctx.restart_pending.get() {
            let pend = hbox(12);
            pend.add_css_class("pend");
            pend.append(&calm::icon(Icon::Relay, 16));
            let text = label(tr("network.restart.pending"), &[]);
            text.set_hexpand(true);
            pend.append(&text);
            let restart = widgets::primary_button(tr("network.restart.now"));
            restart.set_valign(gtk::Align::Center);
            pend.append(&restart);
            page.append(&pend);
            let (n, o) = (Rc::downgrade(&net), self.net_out.downgrade());
            restart.connect_clicked(move |b| {
                if let (Some(n), Some(o)) = (n.upgrade(), o.upgrade()) {
                    n.restart_into(&o, b);
                }
            });
        }
        // What the last restart said stays (the section is rebuilt when the
        // engine answers).
        if let Some(old) = self.net_out.parent().and_downcast::<gtk::Box>() {
            old.remove(&self.net_out);
        }
        page.append(&self.net_out);
        self.net_switches.borrow_mut().clear();
        let Some(status) = net.last_status() else {
            // The first answer re-renders this section.
            net.refresh();
            return;
        };
        page.append(&group_label(tr("net.ways")));
        let card = group_card();
        for row in netview::way_rows(&status, APP_DEBUG) {
            let mut desc = row.chip.label().to_string();
            if let Some(r) = row.running {
                desc = if desc.is_empty() {
                    r.to_string()
                } else {
                    format!("{desc} \u{00B7} {r}")
                };
            }
            let control = row.toggle.as_ref().map(|t| {
                let s = gtk::Switch::new();
                s.set_active(t.active);
                s.set_sensitive(t.sensitive);
                let (n, setting, me) = (Rc::downgrade(&net), t.setting, self.me.clone());
                s.connect_active_notify(move |s| {
                    let (Some(n), Some(p)) = (n.upgrade(), me.upgrade()) else {
                        return;
                    };
                    if n.is_busy() {
                        // Another change is on its way: this one isn't sent,
                        // so the switch goes back.
                        p.render(Section::Network, Slide::None);
                        return;
                    }
                    p.refocus.set(Some(setting));
                    n.apply_patch(netflow::patch_for(setting, s.is_active()));
                });
                self.net_switches.borrow_mut().push((t.setting, s.clone()));
                s.upcast::<gtk::Widget>()
            });
            let r = item(&card, &row.name, Some(&desc), control.as_ref());
            if row.chip == WayChip::Off {
                r.add_css_class("dis");
            }
        }
        page.append(&card);
        let prot = netview::protection_rows(&status, APP_DEBUG);
        if !prot.is_empty() {
            page.append(&group_label(tr("network.protections")));
            let card = group_card();
            for row in prot {
                let desc = if row.line.is_empty() {
                    row.chip.label().to_string()
                } else {
                    format!("{} \u{00B7} {}", row.chip.label(), row.line)
                };
                item(&card, &row.name, Some(&desc), None);
            }
            page.append(&card);
        }
    }

    // ------------------------------------------------------------ Updates

    fn updates(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        section_head(page, tr("updates.title"), Some(tr("updates.intro")));
        let card = group_card();
        page.append(&card);
        let c = card.downgrade();
        ctx.call(
            |e| e.updates_status(),
            move |_, result| {
                let Some(card) = c.upgrade() else { return };
                match result {
                    Ok(u) => {
                        let v = secview::updates_view(&u);
                        let summary = v.summary.clone().or(v.state.map(str::to_string));
                        item(
                            &card,
                            tr("updates.status"),
                            summary.as_deref(),
                            v.state
                                .map(|_| stat(tr("net.preview"), "w").upcast::<gtk::Widget>())
                                .as_ref(),
                        );
                        let lines = v.lines.join("\n");
                        item(
                            &card,
                            tr("updates.fromFriends"),
                            Some(if lines.is_empty() {
                                tr("updates.available")
                            } else {
                                &lines
                            }),
                            None,
                        );
                        for o in &v.offer {
                            item(&card, tr("updates.offer"), Some(o), None);
                        }
                        item(&card, v.install, v.install_text.as_deref(), None);
                    }
                    Err(e) => {
                        item(&card, &e.message, Some(&e.detail), None);
                    }
                }
            },
        );
    }

    // ------------------------------------------------------------ About

    fn about(&self, ctx: &Rc<Ctx>, page: &gtk::Box) {
        let card = group_card();
        let about = vbox(4);
        about.add_css_class("about");
        let mark = MarkView::new(74);
        mark.area.set_margin_bottom(8);
        about.append(&mark.area);
        let name = calm::heading_label(tr("app.name"), &["about-name"]);
        name.set_xalign(0.5);
        about.append(&name);
        let tagline = label(tr("app.tagline"), &["dim"]);
        tagline.set_xalign(0.5);
        tagline.set_justify(gtk::Justification::Center);
        about.append(&tagline);
        let version = label("", &["dim"]);
        version.set_xalign(0.5);
        about.append(&version);
        let id = label(APP_ID, &["faint", "small"]);
        id.set_xalign(0.5);
        id.set_selectable(true);
        about.append(&id);
        card.append(&about);
        page.append(&card);
        let card = group_card();
        item(
            &card,
            tr("about.openSource"),
            Some(tr("about.openSource.body")),
            None,
        );
        let words = widgets::button(tr("nav.securityWords"));
        let c = Rc::downgrade(ctx);
        words.connect_clicked(move |_| {
            if let Some(m) = c.upgrade().and_then(|c| c.main.borrow().clone()) {
                m.show_words();
            }
        });
        let fp = self
            .identity
            .borrow()
            .as_ref()
            .map(|m| trf("you.fingerprint", &[("code", &m.old_code)]));
        item(
            &card,
            tr("about.fingerprint"),
            fp.as_deref(),
            Some(words.upcast_ref()),
        );
        page.append(&card);
        let v = version.downgrade();
        ctx.call(
            |e| e.app_info(),
            move |_, result| {
                let Some(v) = v.upgrade() else { return };
                if let Ok(info) = result {
                    let build = if info.build == Build::Release {
                        tr("net.build.release")
                    } else {
                        tr("net.build.dev")
                    };
                    v.set_text(&format!(
                        "{} \u{00B7} {build}",
                        trf("net.version", &[("version", &info.version)])
                    ));
                }
            },
        );
    }
}

// ---------------------------------------------------------------- the lock icon

/// The Lock sheet's icon: a padlock whose shackle closes (lockShackle,
/// curve shackle) before Raven locks.
struct LockIcon {
    area: gtk::DrawingArea,
    offset: Rc<Cell<f64>>,
    tween: Tween,
}

impl LockIcon {
    fn new() -> Self {
        let area = calm::drawing();
        area.set_content_width(26);
        area.set_content_height(26);
        area.set_valign(gtk::Align::Center);
        let offset = Rc::new(Cell::new(-3.5));
        let o = offset.clone();
        let body = icons::rounded_rect(5.0, 11.0, 14.0, 10.0, 2.5);
        let shackle = icons::parse_path("M8 11V7.5a4 4 0 0 1 8 0V11");
        area.set_draw_func(move |a, cr, w, h| {
            let c = a.style_context().color();
            cr.set_source_rgba(
                c.red() as f64,
                c.green() as f64,
                c.blue() as f64,
                c.alpha() as f64,
            );
            let s = w.min(h) as f64 / 24.0;
            cr.scale(s, s);
            cr.set_line_width(1.8);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            let draw = |ops: &[icons::Op], dy: f64| {
                cr.new_path();
                for op in ops {
                    match *op {
                        icons::Op::Move(x, y) => cr.move_to(x, y + dy),
                        icons::Op::Line(x, y) => cr.line_to(x, y + dy),
                        icons::Op::Curve(a, b, c, d, e, f) => {
                            cr.curve_to(a, b + dy, c, d + dy, e, f + dy)
                        }
                        icons::Op::Close => cr.close_path(),
                    }
                }
                let _ = cr.stroke();
            };
            draw(&body, 0.0);
            draw(&shackle, o.get());
            cr.arc(12.0, 16.0, 1.2, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        });
        Self {
            area,
            offset,
            tween: Tween::default(),
        }
    }

    /// Close the shackle, then `done` (at once under reduced motion).
    fn close(&self, style: &super::style::Style, done: impl FnOnce() + 'static) {
        let (o, a) = (self.offset.clone(), self.area.downgrade());
        let ms = tokens().calm.lock_shackle;
        let motion = style.motion_allowed();
        self.tween.run(&self.area, style, ms, "shackle", move |t| {
            o.set(-3.5 * (1.0 - t));
            if let Some(a) = a.upgrade() {
                a.queue_draw();
            }
        });
        let delay = if motion { ms + 70 } else { 0 };
        after(delay, done);
    }
}
