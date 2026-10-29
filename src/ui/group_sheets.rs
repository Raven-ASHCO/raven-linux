//! Group sheets (desktop/api/API.md, Groups): New group (a name and up to
//! 15 of your friends: you become its one admin), and Members (everyone in
//! the group with "admin" and "not your friend"; the admin adds friends and
//! removes members, anyone else can leave; the admin can't leave).
//!
//! Every change is the engine's: the sheet shows its answer or its error.

use gtk::prelude::*;
use raven_desktop::api::{Contact, Group, GroupMember, GroupStatus};
use raven_desktop::groupview::{self, MAX_MEMBERS};
use raven_desktop::strings::{tr, trf};
use std::cell::RefCell;
use std::rc::Rc;

use super::app::Ctx;
use super::widgets::{
    self, action_row, clear_box, dialog, hbox, heading, label, line, vbox, Avatar, AvatarSize,
};

/// Friends to choose from: a check button per friend, with the cap.
struct Picker {
    root: gtk::Box,
    checks: Vec<(String, gtk::CheckButton)>,
}

impl Picker {
    fn new(friends: &[Contact]) -> Self {
        let root = vbox(2);
        let mut checks = Vec::new();
        for c in friends {
            let row = hbox(10);
            let check = gtk::CheckButton::new();
            let av = Avatar::sized(&c.name, &c.address, AvatarSize::Small);
            let name = line(&c.name, &[]);
            name.set_mnemonic_widget(Some(&check));
            row.append(&check);
            row.append(&av.root);
            row.append(&name);
            root.append(&row);
            checks.push((c.address.clone(), check));
        }
        Self { root, checks }
    }

    fn chosen(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter(|(_, c)| c.is_active())
            .map(|(a, _)| a.clone())
            .collect()
    }

    fn connect_changed(&self, f: impl Fn() + Clone + 'static) {
        for (_, c) in &self.checks {
            let f = f.clone();
            c.connect_toggled(move |_| f());
        }
    }
}

/// New group: a name and friends; Create calls `groups.create`.
pub fn new_group(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    friends: Vec<Contact>,
    on_created: Box<dyn Fn(String)>,
) {
    let (window, content) = dialog(Some(parent), tr("group.create.title"), &ctx.style, 440);
    content.append(&heading(tr("group.create.title")));
    content.append(&label(tr("group.create.note"), &["sheet-text"]));
    let name_label = label(tr("group.create.name"), &["strong"]);
    name_label.set_margin_top(8);
    content.append(&name_label);
    let name = gtk::Entry::new();
    name.add_css_class("raven-field");
    name.set_max_length(64);
    name_label.set_mnemonic_widget(Some(&name));
    content.append(&name);
    let pick_label = label(tr("group.create.pick"), &["strong"]);
    pick_label.set_margin_top(8);
    content.append(&pick_label);
    let picker = Rc::new(Picker::new(&friends));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_max_content_height(260);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&picker.root));
    content.append(&scroll);
    let error = label("", &["danger"]);
    error.set_visible(false);
    content.append(&error);
    let row = action_row();
    let cancel = widgets::button(tr("common.cancel"));
    let create = widgets::primary_button(tr("group.create.button"));
    create.set_sensitive(false);
    row.append(&cancel);
    row.append(&create);
    content.append(&row);

    // Create only with a name and 1..=15 friends.
    let check = {
        let (name, picker, create, error) = (
            name.downgrade(),
            picker.clone(),
            create.downgrade(),
            error.downgrade(),
        );
        move || {
            let (Some(n), Some(b), Some(e)) = (name.upgrade(), create.upgrade(), error.upgrade())
            else {
                return;
            };
            let k = picker.chosen().len();
            let too_many = k + 1 > MAX_MEMBERS;
            e.set_text(tr("group.full"));
            e.set_visible(too_many);
            b.set_sensitive(!n.text().trim().is_empty() && k > 0 && !too_many);
        }
    };
    picker.connect_changed(check.clone());
    name.connect_changed(move |_| check());
    let w = window.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    let (c, w, n, p, e) = (
        Rc::downgrade(ctx),
        window.downgrade(),
        name.downgrade(),
        picker.clone(),
        error.downgrade(),
    );
    let on_created = Rc::new(on_created);
    create.connect_clicked(move |b| {
        let (Some(ctx), Some(n)) = (c.upgrade(), n.upgrade()) else {
            return;
        };
        let group_name = n.text().trim().to_string();
        let members = p.chosen();
        b.set_sensitive(false);
        let (w, e, b, done) = (w.clone(), e.clone(), b.downgrade(), on_created.clone());
        ctx.call(
            move |eng| eng.groups_create(&group_name, &members),
            move |_, result| match result {
                Ok(d) => {
                    if let Some(w) = w.upgrade() {
                        w.close();
                    }
                    done(d.group.id);
                }
                Err(err) => {
                    if let Some(b) = b.upgrade() {
                        b.set_sensitive(true);
                    }
                    if let Some(e) = e.upgrade() {
                        e.set_text(&err.message);
                        e.set_visible(true);
                    }
                }
            },
        );
    });
    window.present();
    name.grab_focus();
}

struct Members {
    ctx: Rc<Ctx>,
    list: gtk::Box,
    add_box: gtk::Box,
    error: gtk::Label,
    group: RefCell<Group>,
    members: RefCell<Vec<GroupMember>>,
    friends: Vec<Contact>,
    on_change: Rc<dyn Fn()>,
}

/// Members of a group.
pub fn members(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    group: Group,
    members: Vec<GroupMember>,
    friends: Vec<Contact>,
    on_change: Box<dyn Fn()>,
) {
    let title = trf("group.membersTitle", &[("name", &group.name)]);
    let (window, content) = dialog(Some(parent), &title, &ctx.style, 460);
    content.append(&heading(&title));
    let note = if group.admin {
        tr("group.youAdmin").to_string()
    } else {
        trf("group.adminNote", &[("name", &group.admin_name)])
    };
    content.append(&label(&note, &["sheet-text"]));
    let list = vbox(6);
    list.set_margin_top(8);
    content.append(&list);
    let add_box = vbox(6);
    content.append(&add_box);
    let error = label("", &["danger"]);
    error.set_visible(false);
    content.append(&error);
    let on_change: Rc<dyn Fn()> = Rc::from(on_change);
    let row = action_row();
    // Only a member who isn't the admin can leave (the admin can't).
    if !group.admin && group.status == GroupStatus::Active {
        let leave = widgets::button(tr("group.leave"));
        leave.add_css_class("raven-danger");
        let (c, w, g, done) = (
            Rc::downgrade(ctx),
            window.downgrade(),
            group.clone(),
            on_change.clone(),
        );
        leave.connect_clicked(move |_| {
            if let (Some(ctx), Some(w)) = (c.upgrade(), w.upgrade()) {
                confirm_leave(&ctx, &w, &g, done.clone());
            }
        });
        row.append(&leave);
    }
    let close = widgets::primary_button(tr("common.close"));
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    row.append(&close);
    content.append(&row);
    let sheet = Rc::new(Members {
        ctx: ctx.clone(),
        list,
        add_box,
        error,
        group: RefCell::new(group),
        members: RefCell::new(members),
        friends,
        on_change,
    });
    sheet.render();
    // The sheet lives as long as its window.
    widgets::keep_alive(&window, sheet);
    window.present();
    close.grab_focus();
}

impl Members {
    fn render(self: &Rc<Self>) {
        clear_box(&self.list);
        clear_box(&self.add_box);
        let g = self.group.borrow().clone();
        for m in self.members.borrow().iter() {
            let row = hbox(10);
            let av = Avatar::sized(&m.name, &m.address, AvatarSize::Small);
            row.append(&av.root);
            let name = line(&groupview::member_name(m), &["strong"]);
            row.append(&name);
            for tag in groupview::member_tags(m) {
                row.append(&line(&format!("\u{00B7} {tag}"), &["dim", "small"]));
            }
            let spacer = hbox(0);
            spacer.set_hexpand(true);
            row.append(&spacer);
            if g.admin && !m.me && g.status == GroupStatus::Active {
                let remove = widgets::button(tr("group.remove"));
                super::a11y::set_label(&remove, &format!("{} {}", tr("group.remove"), m.name));
                let (me, address) = (Rc::downgrade(self), m.address.clone());
                remove.connect_clicked(move |b| {
                    if let Some(s) = me.upgrade() {
                        b.set_sensitive(false);
                        s.remove(&address);
                    }
                });
                row.append(&remove);
            }
            self.list.append(&row);
        }
        // The admin adds friends who aren't in it yet.
        if !g.admin || g.status != GroupStatus::Active {
            return;
        }
        let inside: Vec<String> = self
            .members
            .borrow()
            .iter()
            .map(|m| m.address.clone())
            .collect();
        let candidates: Vec<Contact> = self
            .friends
            .iter()
            .filter(|c| !inside.contains(&c.address))
            .cloned()
            .collect();
        if candidates.is_empty() {
            return;
        }
        let title = label(tr("group.add"), &["strong"]);
        title.set_margin_top(12);
        self.add_box.append(&title);
        let picker = Rc::new(Picker::new(&candidates));
        self.add_box.append(&picker.root);
        let add = widgets::button(tr("group.addButton"));
        add.set_halign(gtk::Align::Start);
        add.set_sensitive(false);
        let room = MAX_MEMBERS.saturating_sub(inside.len());
        {
            let (p, b, e) = (picker.clone(), add.downgrade(), self.error.downgrade());
            picker.connect_changed(move || {
                let k = p.chosen().len();
                if let (Some(b), Some(e)) = (b.upgrade(), e.upgrade()) {
                    b.set_sensitive(k > 0 && k <= room);
                    e.set_text(tr("group.full"));
                    e.set_visible(k > room);
                }
            });
        }
        let (me, p) = (Rc::downgrade(self), picker.clone());
        add.connect_clicked(move |b| {
            if let Some(s) = me.upgrade() {
                b.set_sensitive(false);
                s.add(p.chosen());
            }
        });
        self.add_box.append(&add);
    }

    fn apply(
        self: &Rc<Self>,
        result: Result<raven_desktop::api::GroupDetail, raven_desktop::api::ApiError>,
    ) {
        match result {
            Ok(d) => {
                self.error.set_visible(false);
                self.group.replace(d.group);
                self.members.replace(d.members);
                self.render();
                (self.on_change)();
            }
            Err(e) => {
                self.error.set_text(&e.message);
                self.error.set_visible(true);
                self.render();
            }
        }
    }

    fn remove(self: &Rc<Self>, address: &str) {
        let id = self.group.borrow().id.clone();
        let address = address.to_string();
        let me = Rc::downgrade(self);
        self.ctx.call(
            move |e| e.groups_remove(&id, &address),
            move |_, result| {
                if let Some(s) = me.upgrade() {
                    s.apply(result);
                }
            },
        );
    }

    fn add(self: &Rc<Self>, addresses: Vec<String>) {
        let id = self.group.borrow().id.clone();
        let me = Rc::downgrade(self);
        self.ctx.call(
            move |e| e.groups_add(&id, &addresses),
            move |_, result| {
                if let Some(s) = me.upgrade() {
                    s.apply(result);
                }
            },
        );
    }
}

/// "Leave {group}?": the history stays; sending is refused afterwards.
fn confirm_leave(ctx: &Rc<Ctx>, parent: &gtk::Window, group: &Group, on_left: Rc<dyn Fn()>) {
    let title = trf("group.leave.title", &[("name", &group.name)]);
    let (window, content) = dialog(Some(parent), &title, &ctx.style, 420);
    content.append(&heading(&title));
    content.append(&label(tr("group.leave.body"), &["sheet-text"]));
    let error = label("", &["danger"]);
    error.set_visible(false);
    content.append(&error);
    let row = action_row();
    let cancel = widgets::button(tr("common.cancel"));
    let leave = widgets::primary_button(tr("group.leave"));
    leave.add_css_class("danger-fill");
    row.append(&cancel);
    row.append(&leave);
    content.append(&row);
    let w = window.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    let (c, w, p, e, id) = (
        Rc::downgrade(ctx),
        window.downgrade(),
        parent.downgrade(),
        error.downgrade(),
        group.id.clone(),
    );
    leave.connect_clicked(move |b| {
        let Some(ctx) = c.upgrade() else { return };
        b.set_sensitive(false);
        let (w, p, e, b, id, done) = (
            w.clone(),
            p.clone(),
            e.clone(),
            b.downgrade(),
            id.clone(),
            on_left.clone(),
        );
        ctx.call(
            move |eng| eng.groups_leave(&id),
            move |_, result| match result {
                Ok(_) => {
                    done();
                    if let Some(w) = w.upgrade() {
                        w.close();
                    }
                    if let Some(p) = p.upgrade() {
                        p.close();
                    }
                }
                Err(err) => {
                    if let Some(b) = b.upgrade() {
                        b.set_sensitive(true);
                    }
                    if let Some(e) = e.upgrade() {
                        e.set_text(&err.message);
                        e.set_visible(true);
                    }
                }
            },
        );
    });
    window.present();
    cancel.grab_focus();
}
