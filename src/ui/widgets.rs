//! Small widget helpers shared by the screens. Every label is plain text
//! (never Pango markup), so engine text cannot inject formatting.

use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::Presence;
use raven_desktop::present;
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::{avatar_index, tokens};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::style::Style;

pub fn sp(i: usize) -> i32 {
    tokens().spacing[i.min(4)] as i32
}

pub const XS: usize = 0;
pub const SM: usize = 1;
pub const MD: usize = 2;
pub const LG: usize = 3;
pub const XL: usize = 4;

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

/// A wrapping, left-aligned plain-text label.
pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_use_markup(false);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// A single-line label that ellipsizes instead of wrapping.
pub fn line(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label(text, classes);
    l.set_wrap(false);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

/// A title a screen reader announces as a heading.
pub fn heading(text: &str) -> gtk::Label {
    super::calm::heading_label(text, &["title"])
}

fn plain_button(text: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.set_use_underline(false);
    b
}

/// A Calm button (surface, fine border).
pub fn button(text: &str) -> gtk::Button {
    let b = plain_button(text);
    b.add_css_class("raven-btn");
    b
}

/// The one main action of a screen (accent fill).
pub fn primary_button(text: &str) -> gtk::Button {
    let b = plain_button(text);
    b.add_css_class("raven-primary");
    b
}

/// A text-only action in the accent colour.
pub fn quiet_button(text: &str) -> gtk::Button {
    let b = plain_button(text);
    b.add_css_class("raven-quiet");
    b
}

pub fn flat_button(text: &str) -> gtk::Button {
    let b = plain_button(text);
    b.add_css_class("raven-flat");
    b
}

/// A Calm button with an icon before its text.
pub fn icon_button(icon: raven_desktop::icons::Icon, text: &str) -> gtk::Button {
    let row = hbox(6);
    row.append(&super::calm::icon(icon, 16));
    row.append(&gtk::Label::new(Some(text)));
    let b = gtk::Button::new();
    b.set_child(Some(&row));
    b.add_css_class("raven-btn");
    super::a11y::set_label(&b, text);
    b
}

/// A row of buttons aligned to the end (dialog actions).
pub fn action_row() -> gtk::Box {
    let b = hbox(sp(SM));
    b.set_halign(gtk::Align::End);
    b.set_margin_top(sp(LG));
    b
}

/// A "Details" expander with selectable technical text.
pub fn details_expander(detail: &str) -> gtk::Expander {
    let e = gtk::Expander::new(Some(tr("message.details")));
    let l = label(detail, &["mono", "dim"]);
    l.set_selectable(true);
    e.set_child(Some(&l));
    e.set_visible(!detail.is_empty());
    e
}

/// Preferences row: a label that takes the width and a control at the end.
/// The label is the control's accessible name (GTK 4 labels the mnemonic
/// widget with it), so a screen reader says what a switch is for.
pub fn pref_row(text: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = hbox(sp(MD));
    let l = label(text, &[]);
    l.set_hexpand(true);
    l.set_mnemonic_widget(Some(control));
    row.append(&l);
    control.set_valign(gtk::Align::Center);
    row.append(control);
    row
}

/// A modal sheet: a dialog window attached to its parent (GNOME's way of
/// showing a modal question; plain GTK 4.0 API, GTK's own dialog classes
/// are deprecated from 4.10), on the surface colour, whose content rises
/// in (sheetIn). Escape closes it.
pub fn dialog(
    parent: Option<&gtk::Window>,
    title: &str,
    style: &Style,
    width: i32,
) -> (gtk::Window, gtk::Box) {
    let w = gtk::Window::new();
    w.set_title(Some(title));
    w.set_modal(true);
    w.set_destroy_with_parent(true);
    w.set_default_size(width, -1);
    if let Some(p) = parent {
        w.set_transient_for(Some(p));
    }
    style.decorate_window(&w);
    w.add_css_class("raven-sheet");
    let content = vbox(sp(SM));
    content.add_css_class("sheet-body");
    content.add_css_class("raven-surface");
    w.set_child(Some(&content));
    let body = content.downgrade();
    let ms = tokens().calm.sheet_in;
    let motion = style.motion_allowed();
    w.connect_map(move |_| {
        if let (true, Some(b)) = (motion, body.upgrade()) {
            b.add_css_class("sheet-in");
            let b2 = b.downgrade();
            after(ms + 40, move || {
                if let Some(b) = b2.upgrade() {
                    b.remove_css_class("sheet-in");
                }
            });
        }
    });
    let key = gtk::EventControllerKey::new();
    let weak = w.downgrade();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            if let Some(w) = weak.upgrade() {
                w.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    w.add_controller(key);
    (w, content)
}

/// Keep `state` alive until `window` is destroyed (its handlers are then
/// dropped, which drops the state and breaks any reference cycle).
pub fn keep_alive<T: 'static>(window: &impl IsA<gtk::Widget>, state: Rc<T>) {
    window.connect_destroy(move |_| {
        let _ = &state;
    });
}

/// Remove every child of a box.
pub fn clear_box(b: &gtk::Box) {
    while let Some(child) = b.first_child() {
        b.remove(&child);
    }
}

/// Run `f` once after `ms` milliseconds. The returned id must not be
/// removed after the callback ran (glib-rs panics on a stale id); use
/// [`OneShot`] for timers that may be cancelled.
pub fn after(ms: u32, f: impl FnOnce() + 'static) -> glib::SourceId {
    glib::timeout_add_local_once(Duration::from_millis(ms as u64), f)
}

/// A cancellable one-shot timer: starting it again cancels the pending one,
/// and the slot is cleared when it fires, so a stale id is never removed.
#[derive(Default, Clone)]
pub struct OneShot(Rc<RefCell<Option<glib::SourceId>>>);

impl OneShot {
    pub fn start(&self, ms: u32, f: impl FnOnce() + 'static) {
        self.cancel();
        let slot = self.0.clone();
        let id = after(ms, move || {
            slot.borrow_mut().take();
            f();
        });
        self.0.replace(Some(id));
    }

    pub fn cancel(&self) {
        if let Some(id) = self.0.borrow_mut().take() {
            id.remove();
        }
    }
}

// ---------------------------------------------------------------- avatar

/// Avatar sizes (tokens' layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarSize {
    /// 36: the sidebar.
    Normal,
    /// 40: the chat header.
    Large,
    /// 22: beside a group bubble, in Details.
    Small,
    /// 21: stacked in a group's sidebar row.
    Stacked,
}

/// Round avatar with the friend's initial, the presence dot and the retry
/// ring. Colour from the tokens' avatar rule (sum of address bytes mod 5).
pub struct Avatar {
    pub root: gtk::Overlay,
    initial: gtk::Label,
    dot: gtk::Box,
    ring: gtk::Box,
    presence: Cell<Option<Presence>>,
}

impl Avatar {
    pub fn new(name: &str, address: &str) -> Rc<Self> {
        Self::sized(name, address, AvatarSize::Normal)
    }

    pub fn sized(name: &str, address: &str, size: AvatarSize) -> Rc<Self> {
        let l = &tokens().layout;
        let (px, class) = match size {
            AvatarSize::Normal => (l.avatar_size, None),
            AvatarSize::Large => (l.avatar_large, Some("lg")),
            AvatarSize::Small => (l.avatar_small, Some("sm")),
            AvatarSize::Stacked => (21, Some("xs")),
        };
        let px = px as i32;
        let dot_size = l.presence_dot as i32;
        let initial = gtk::Label::new(Some(&present::avatar_initial(name)));
        initial.add_css_class("avatar");
        if let Some(c) = class {
            initial.add_css_class(c);
        }
        initial.add_css_class(&format!("avatar-{}", avatar_index(address)));
        initial.set_size_request(px, px);
        initial.set_halign(gtk::Align::Center);
        initial.set_valign(gtk::Align::Center);
        let root = gtk::Overlay::new();
        root.set_child(Some(&initial));
        root.set_valign(gtk::Align::Center);
        root.set_halign(gtk::Align::Center);
        let ring = hbox(0);
        ring.add_css_class("ring");
        ring.set_size_request(px, px);
        ring.set_can_target(false);
        let dot = hbox(0);
        dot.add_css_class("presence");
        dot.set_size_request(dot_size, dot_size);
        dot.set_halign(gtk::Align::End);
        dot.set_valign(gtk::Align::End);
        dot.set_margin_end(-1);
        dot.set_margin_bottom(-1);
        dot.set_can_target(false);
        root.add_overlay(&ring);
        root.add_overlay(&dot);
        // Only friends in the sidebar and the header show presence.
        let with_dot = matches!(size, AvatarSize::Normal | AvatarSize::Large);
        dot.set_visible(with_dot);
        ring.set_visible(with_dot);
        Rc::new(Self {
            root,
            initial,
            dot,
            ring,
            presence: Cell::new(None),
        })
    }

    /// The avatar of "You" (accent tint).
    pub fn you(name: &str) -> Rc<Self> {
        let a = Self::sized(name, "", AvatarSize::Normal);
        for i in 0..5 {
            a.initial.remove_css_class(&format!("avatar-{i}"));
        }
        a.initial.add_css_class("avatar-you");
        a.dot.set_visible(false);
        a.ring.set_visible(false);
        a
    }

    /// A stacked avatar has a ring of the sidebar colour around it.
    pub fn stacked(&self) {
        self.initial.add_css_class("stacked");
    }

    pub fn set_name(&self, name: &str) {
        self.initial.set_text(&present::avatar_initial(name));
    }

    /// Set the presence dot. `blink` (a `presence` event) blinks it when it
    /// changed; an in-touch dot breathes slowly while motion is on.
    pub fn set_presence(&self, p: Presence, blink: bool, style: &Style) {
        let changed = self.presence.replace(Some(p)) != Some(p);
        for c in ["in-touch", "waiting", "none", "breathe"] {
            self.dot.remove_css_class(c);
        }
        self.dot.add_css_class(present::presence_class(p));
        let motion = style.motion_allowed();
        if motion && p == Presence::InTouch {
            self.dot.add_css_class("breathe");
        }
        if blink && changed && motion {
            self.dot.remove_css_class("breathe");
            self.dot.add_css_class("blink");
            let dot = self.dot.downgrade();
            after(tokens().calm.presence_blink * 2, move || {
                if let Some(dot) = dot.upgrade() {
                    dot.remove_css_class("blink");
                    if dot.has_css_class("in-touch") {
                        dot.add_css_class("breathe");
                    }
                }
            });
        }
    }

    /// Re-apply the dot after a motion setting change.
    pub fn refresh_motion(&self, style: &Style) {
        if let Some(p) = self.presence.get() {
            self.set_presence(p, false, style);
        }
    }

    /// A `retry` event: a ring spreads once around the avatar.
    pub fn ring(&self, style: &Style) {
        super::calm::play_class(&self.ring, style, "go", tokens().durations.retry_ring);
    }
}

// ---------------------------------------------------------------- words

/// Your 10 security words as Calm cards (number above the word, word 10
/// marked "check"): they come in one after another when first shown, then
/// the read-aloud highlight steps through them (one word every
/// `readAloudStep` ms, while the grid is on screen and motion is allowed)
/// with "Say: lantern" under them.
pub struct WordGrid {
    pub root: gtk::Box,
    cards: Vec<gtk::Box>,
    words: Vec<String>,
    say: gtk::Label,
    step: Cell<usize>,
    timer: RefCell<Option<glib::SourceId>>,
    revealed: Cell<bool>,
}

impl WordGrid {
    pub fn new(words: &[String], style: &Rc<Style>) -> Rc<Self> {
        let root = vbox(sp(SM));
        let grid = gtk::Grid::new();
        grid.set_column_spacing(8);
        grid.set_row_spacing(8);
        grid.set_column_homogeneous(true);
        let mut cards = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let card = vbox(0);
            card.add_css_class("w");
            let top = hbox(0);
            let n = gtk::Label::new(Some(&(i + 1).to_string()));
            n.add_css_class("w-n");
            n.set_xalign(0.0);
            n.set_hexpand(true);
            top.append(&n);
            if i == 9 {
                card.add_css_class("check");
                let mark = gtk::Label::new(Some(&tr("words.checkLabel").to_uppercase()));
                mark.add_css_class("w-ck");
                top.append(&mark);
            }
            card.append(&top);
            let word = gtk::Label::new(Some(w));
            word.add_css_class("w-word");
            word.set_xalign(0.0);
            word.set_selectable(true);
            card.append(&word);
            grid.attach(&card, (i % 5) as i32, (i / 5) as i32, 1, 1);
            cards.push(card);
        }
        root.append(&grid);
        let say = label("", &["sayline"]);
        say.set_margin_top(sp(MD));
        root.append(&say);
        let g = Rc::new(Self {
            root,
            cards,
            words: words.to_vec(),
            say,
            step: Cell::new(0),
            timer: RefCell::new(None),
            revealed: Cell::new(false),
        });
        let weak = Rc::downgrade(&g);
        let st = style.clone();
        g.root.connect_map(move |_| {
            if let Some(g) = weak.upgrade() {
                g.reveal(&st);
                g.start(&st);
            }
        });
        let weak = Rc::downgrade(&g);
        g.root.connect_unmap(move |_| {
            if let Some(g) = weak.upgrade() {
                g.stop();
            }
        });
        let st = style.clone();
        style.watch(&g, move |g| {
            if g.root.is_mapped() {
                g.stop();
                g.start(&st);
            }
        });
        g
    }

    /// The cards come in one after another (once).
    fn reveal(&self, style: &Style) {
        if self.revealed.replace(true) || !style.motion_allowed() {
            return;
        }
        let step = tokens().durations.word_reveal;
        for (i, c) in self.cards.iter().enumerate() {
            c.set_opacity(0.0);
            let c = c.downgrade();
            after(step * i as u32, move || {
                if let Some(c) = c.upgrade() {
                    c.set_opacity(1.0);
                    c.add_css_class("reveal");
                    let c2 = c.downgrade();
                    after(400, move || {
                        if let Some(c) = c2.upgrade() {
                            c.remove_css_class("reveal");
                        }
                    });
                }
            });
        }
    }

    fn start(self: &Rc<Self>, style: &Style) {
        self.stop();
        if !style.motion_allowed() || self.words.is_empty() {
            // Reduced motion: no stepping highlight, no "Say:" line.
            self.say.set_visible(false);
            return;
        }
        self.say.set_visible(true);
        self.show_step();
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local(
            Duration::from_millis(tokens().durations.read_aloud_step as u64),
            move || {
                let Some(g) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                g.step.set((g.step.get() + 1) % g.words.len());
                g.show_step();
                glib::ControlFlow::Continue
            },
        );
        self.timer.replace(Some(id));
    }

    fn stop(&self) {
        if let Some(id) = self.timer.take() {
            id.remove();
        }
        for c in &self.cards {
            c.remove_css_class("say");
        }
    }

    fn show_step(&self) {
        let k = self.step.get();
        for (i, c) in self.cards.iter().enumerate() {
            if i == k {
                c.add_css_class("say");
            } else {
                c.remove_css_class("say");
            }
        }
        if let Some(w) = self.words.get(k) {
            self.say.set_text(&trf("words.say", &[("word", w)]));
        }
    }
}

impl Drop for WordGrid {
    fn drop(&mut self) {
        if let Some(id) = self.timer.take() {
            id.remove();
        }
    }
}

// ---------------------------------------------------------------- steps

/// A step strip (Calm): a numbered circle per step, a check once done, and
/// a bar between steps that turns green as you pass it.
pub struct StepStrip {
    pub root: gtk::Box,
    steps: Vec<(gtk::Box, gtk::Label)>,
    bars: Vec<gtk::Box>,
}

impl StepStrip {
    pub fn new(texts: &[&str]) -> Self {
        let root = hbox(8);
        root.add_css_class("steps");
        let mut steps = Vec::new();
        let mut bars = Vec::new();
        for (i, t) in texts.iter().enumerate() {
            if i > 0 {
                let bar = hbox(0);
                bar.add_css_class("step-bar");
                bar.set_hexpand(true);
                bar.set_valign(gtk::Align::Center);
                bar.set_size_request(16, 2);
                root.append(&bar);
                bars.push(bar);
            }
            let step = hbox(6);
            step.add_css_class("step");
            let n = gtk::Label::new(Some(&(i + 1).to_string()));
            n.add_css_class("step-i");
            n.set_size_request(18, 18);
            step.append(&n);
            step.append(&gtk::Label::new(Some(t)));
            root.append(&step);
            steps.push((step, n));
        }
        Self { root, steps, bars }
    }

    pub fn set(&self, current: usize) {
        for (i, (step, n)) in self.steps.iter().enumerate() {
            for c in ["now", "done", "current"] {
                step.remove_css_class(c);
            }
            if i == current {
                step.add_css_class("now");
                n.set_text(&(i + 1).to_string());
            } else if i < current {
                step.add_css_class("done");
                n.set_text("\u{2713}");
            } else {
                n.set_text(&(i + 1).to_string());
            }
        }
        for (i, b) in self.bars.iter().enumerate() {
            if i < current {
                b.add_css_class("done");
            } else {
                b.remove_css_class("done");
            }
        }
    }
}
